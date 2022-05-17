// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub use evm::backend::Basic as Account;
use sp_core::{H160, H256, U256};
use sp_std::vec::Vec;

pub struct CheckEvmTransactionInput {
	pub chain_id: Option<u64>,
	pub to: Option<H160>,
	pub input: Vec<u8>,
	pub nonce: U256,
	pub gas_limit: U256,
	pub gas_price: Option<U256>,
	pub max_fee_per_gas: Option<U256>,
	pub max_priority_fee_per_gas: Option<U256>,
	pub value: U256,
	pub access_list: Vec<(H160, Vec<H256>)>,
}

pub struct CheckEvmTransactionConfig<'config> {
	pub evm_config: &'config evm::Config,
	pub block_gas_limit: U256,
	pub base_fee: U256,
	pub chain_id: u64,
	pub is_transactional: bool,
}

pub struct CheckEvmTransaction<'config, E: From<InvalidEvmTransactionError>> {
	pub config: CheckEvmTransactionConfig<'config>,
	pub transaction: CheckEvmTransactionInput,
	_marker: sp_std::marker::PhantomData<E>,
}

pub enum InvalidEvmTransactionError {
	GasLimitTooLow,
	GasLimitTooHigh,
	GasPriceTooLow,
	PriorityFeeTooHigh,
	BalanceTooLow,
	TxNonceTooLow,
	TxNonceTooHigh,
	InvalidPaymentInput,
	InvalidChainId,
}

impl<'config, E: From<InvalidEvmTransactionError>> CheckEvmTransaction<'config, E> {
	pub fn new(
		config: CheckEvmTransactionConfig<'config>,
		transaction: CheckEvmTransactionInput,
	) -> Self {
		CheckEvmTransaction {
			config,
			transaction,
			_marker: Default::default(),
		}
	}

	pub fn validate_in_pool_for(&self, who: &Account) -> Result<&Self, E> {
		if self.transaction.nonce < who.nonce {
			return Err(InvalidEvmTransactionError::TxNonceTooLow.into());
		}
		self.validate_common()
	}

	pub fn validate_in_block_for(&self, who: &Account) -> Result<&Self, E> {
		if self.transaction.nonce > who.nonce {
			return Err(InvalidEvmTransactionError::TxNonceTooHigh.into());
		} else if self.transaction.nonce < who.nonce {
			return Err(InvalidEvmTransactionError::TxNonceTooLow.into());
		}
		self.validate_common()
	}

	pub fn with_chain_id(&self) -> Result<&Self, E> {
		// Chain id matches the one in the signature.
		if let Some(chain_id) = self.transaction.chain_id {
			if chain_id != self.config.chain_id {
				return Err(InvalidEvmTransactionError::InvalidChainId.into());
			}
		}
		Ok(self)
	}

	pub fn with_base_fee(&self) -> Result<&Self, E> {
		// Get fee data from either a legacy or typed transaction input.
		let max_fee_per_gas = self.max_fee_per_gas()?;
		if self.config.is_transactional
			|| (!self.config.is_transactional && max_fee_per_gas > U256::zero())
		{
			// Transaction max fee is at least the current base fee.
			if max_fee_per_gas < self.config.base_fee {
				return Err(InvalidEvmTransactionError::GasPriceTooLow.into());
			}
		}
		Ok(self)
	}

	pub fn with_balance_for(&self, who: &Account) -> Result<&Self, E> {
		// Get fee data from either a legacy or typed transaction input.
		let max_fee_per_gas = self.max_fee_per_gas()?;

		// Account has enough funds to pay for the transaction.
		// Check is skipper on non-transactional calls that don't provide
		// a gas price input.
		let fee = max_fee_per_gas.saturating_mul(self.transaction.gas_limit);
		if self.config.is_transactional || (!self.config.is_transactional && fee > U256::zero()) {
			let total_payment = self.transaction.value.saturating_add(fee);
			if who.balance < total_payment {
				return Err(InvalidEvmTransactionError::BalanceTooLow.into());
			}
		}
		Ok(self)
	}

	fn max_fee_per_gas(&self) -> Result<U256, E> {
		match (
			self.transaction.gas_price,
			self.transaction.max_fee_per_gas,
			self.transaction.max_priority_fee_per_gas,
		) {
			// Legacy or EIP-2930 transaction.
			(Some(gas_price), None, None) => Ok(gas_price),
			// EIP-1559 transaction without tip.
			(None, Some(max_fee_per_gas), None) => Ok(max_fee_per_gas),
			// EIP-1559 transaction with tip.
			(None, Some(max_fee_per_gas), Some(max_priority_fee_per_gas)) => {
				if max_priority_fee_per_gas > max_fee_per_gas {
					return Err(InvalidEvmTransactionError::PriorityFeeTooHigh.into());
				}
				Ok(max_fee_per_gas)
			}
			_ => {
				if self.config.is_transactional {
					return Err(InvalidEvmTransactionError::InvalidPaymentInput.into());
				} else {
					// Allow non-set fee input for non-transactional calls.
					Ok(U256::zero())
				}
			}
		}
	}

	fn validate_common(&self) -> Result<&Self, E> {
		// We must ensure a transaction can pay the cost of its data bytes.
		// If it can't it should not be included in a block.
		let mut gasometer = evm::gasometer::Gasometer::new(
			self.transaction.gas_limit.low_u64(),
			self.config.evm_config,
		);
		let transaction_cost = if self.transaction.to.is_some() {
			evm::gasometer::call_transaction_cost(
				&self.transaction.input,
				&self.transaction.access_list,
			)
		} else {
			evm::gasometer::create_transaction_cost(
				&self.transaction.input,
				&self.transaction.access_list,
			)
		};
		if gasometer.record_transaction(transaction_cost).is_err() {
			return Err(InvalidEvmTransactionError::GasLimitTooLow.into());
		}

		// Transaction gas limit is within the upper bound block gas limit.
		if self.transaction.gas_limit >= self.config.block_gas_limit {
			return Err(InvalidEvmTransactionError::GasLimitTooHigh.into());
		}

		Ok(self)
	}
}
