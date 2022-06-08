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
use ethereum::{
	AccessList, AccessListItem, EIP1559Transaction, EIP2930Transaction, LegacyTransaction,
	TransactionAction, TransactionRecoveryId, TransactionSignature, TransactionV2,
};
use ethereum_types::{H160, H256, U256};
use fp_evm::FeeCalculator;
use frame_support::codec::{Decode, Encode};
use sp_std::vec::Vec;

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, scale_info::TypeInfo)]
/// Manually sets a gas fee.
pub struct ManualEthereumXcmFee {
	/// Legacy or Eip-2930
	pub gas_price: Option<U256>,
	/// Eip-1559
	pub max_fee_per_gas: Option<U256>,
	/// Eip-1559
	pub max_priority_fee_per_gas: Option<U256>,
}

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, scale_info::TypeInfo)]
/// Authomatic gas fee based on the current on-chain values.
/// Will always produce an Eip-1559 transaction.
pub enum AutoEthereumXcmFee {
	/// base_fee_per_gas = BaseFee
	Low,
	/// max_fee_per_gas = 2 * BaseFee, max_priority_fee_per_gas = BaseFee
	Medium,
	/// max_fee_per_gas = 3 * BaseFee, max_priority_fee_per_gas = 2 * BaseFee
	High,
}

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, scale_info::TypeInfo)]
pub enum EthereumXcmFee {
	Manual(ManualEthereumXcmFee),
	Auto(AutoEthereumXcmFee),
}

/// Xcm transact's Ethereum transaction.
#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, scale_info::TypeInfo)]
pub struct EthereumXcmTransaction {
	/// Gas limit to be consumed by EVM execution.
	pub gas_limit: U256,
	/// Fee configuration of choice.
	pub fee_payment: EthereumXcmFee,
	/// Either a Call (the callee, account or contract address) or Create (currently unsupported).
	pub action: TransactionAction,
	/// Value to be transfered.
	pub value: U256,
	/// Input data for a contract call.
	pub input: Vec<u8>,
	/// Map of addresses to be pre-paid to warm storage.
	pub access_list: Option<Vec<(H160, Vec<H256>)>>,
}

pub trait XcmToEthereum {
	fn into_transaction_v2(&self, base_fee: U256, nonce: U256) -> Option<TransactionV2>;
}

impl XcmToEthereum for EthereumXcmTransaction {
	fn into_transaction_v2(&self, base_fee: U256, nonce: U256) -> Option<TransactionV2> {
		let from_tuple_to_access_list = |t: Vec<(H160, Vec<H256>)>| -> AccessList {
			t.iter()
				.map(|item| AccessListItem {
					address: item.0.clone(),
					storage_keys: item.1.clone(),
				})
				.collect::<Vec<AccessListItem>>()
		};

		let (gas_price, max_fee, max_priority_fee) = match &self.fee_payment {
			EthereumXcmFee::Manual(fee_config) => (
				fee_config.gas_price,
				fee_config.max_fee_per_gas,
				fee_config.max_priority_fee_per_gas,
			),
			EthereumXcmFee::Auto(auto_mode) => {
				let (max_fee, max_priority_fee) = match auto_mode {
					AutoEthereumXcmFee::Low => (Some(base_fee), None),
					AutoEthereumXcmFee::Medium => (
						Some(base_fee.saturating_mul(U256::from(2u64))),
						Some(base_fee),
					),
					AutoEthereumXcmFee::High => (
						Some(base_fee.saturating_mul(U256::from(3u64))),
						Some(base_fee.saturating_mul(U256::from(2u64))),
					),
				};
				(None, max_fee, max_priority_fee)
			}
		};
		match (gas_price, max_fee, max_priority_fee) {
			(Some(gas_price), None, None) => {
				// Legacy or Eip-2930
				if let Some(ref access_list) = self.access_list {
					// Eip-2930
					Some(TransactionV2::EIP2930(EIP2930Transaction {
						chain_id: 0,
						nonce,
						gas_price,
						gas_limit: self.gas_limit,
						action: self.action,
						value: self.value,
						input: self.input.clone(),
						access_list: from_tuple_to_access_list(access_list.to_vec()),
						odd_y_parity: true,
						r: H256::default(),
						s: H256::default(),
					}))
				} else {
					// Legacy
					Some(TransactionV2::Legacy(LegacyTransaction {
						nonce,
						gas_price,
						gas_limit: self.gas_limit,
						action: self.action,
						value: self.value,
						input: self.input.clone(),
						signature: TransactionSignature::new(42, H256::from_low_u64_be(1u64), H256::from_low_u64_be(1u64)).unwrap(), // TODO
					}))
				}
			}
			(None, Some(max_fee), _) => {
				// Eip-1559
				Some(TransactionV2::EIP1559(EIP1559Transaction {
					chain_id: 0,
					nonce,
					max_fee_per_gas: max_fee,
					max_priority_fee_per_gas: max_priority_fee.unwrap_or_else(U256::zero),
					gas_limit: self.gas_limit,
					action: self.action,
					value: self.value,
					input: self.input.clone(),
					access_list: if let Some(ref access_list) = self.access_list {
						from_tuple_to_access_list(access_list.to_vec())
					} else {
						Vec::new()
					},
					odd_y_parity: true,
					r: H256::default(),
					s: H256::default(),
				}))
			}
			_ => return None,
		}
	}
}

// /// Xcm transact's Ethereum transaction.
// #[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, scale_info::TypeInfo)]
// #[scale_info(skip_type_params(T))]
// pub struct EthereumXcmTransaction<T: crate::Config> {
//     /// Gas limit to be consumed by EVM execution.
//     pub gas_limit: U256,
//     /// Fee configuration of choice.
//     pub fee_payment: EthereumXcmFee,
//     /// Either a Call (the callee, account or contract address) or Create (currently unsupported).
//     pub action: TransactionAction,
//     /// Value to be transfered.
//     pub value: U256,
//     /// Input data for a contract call.
//     pub input: Vec<u8>,
//     /// Map of addresses to be pre-paid to warm storage.
//     pub access_list: Option<Vec<(H160, Vec<H256>)>>,
//     _marker: sp_std::marker::PhantomData<T>,
// }

// impl<T: crate::Config> From<EthereumXcmTransaction<T>> for Option<TransactionV2> {
//     fn from(t: EthereumXcmTransaction<T>) -> Self {

//         let from_tuple_to_access_list = |t: Vec<(H160, Vec<H256>)>| -> AccessList {
//             t.iter().map(|item| AccessListItem {
//                 address: item.0,
//                 storage_keys: item.1,

//             }).collect::<Vec<AccessListItem>>()
//         };

//         let (gas_price, max_fee, max_priority_fee) = match t.fee_payment {
//             EthereumXcmFee::Manual(fee_config) => {
//                 (fee_config.gas_price, fee_config.max_fee_per_gas, fee_config.max_priority_fee_per_gas)
//             },
//             EthereumXcmFee::Auto(auto_mode) => {
//                 let (base_fee, _) = T::FeeCalculator::min_gas_price();
//                 let (max_fee, max_priority_fee) = match auto_mode {
//                     AutoEthereumXcmFee::Low => (Some(base_fee), None),
//                     AutoEthereumXcmFee::Medium => (Some(base_fee.saturating_mul(U256::from(2))), Some(base_fee)),
//                     AutoEthereumXcmFee::High => (Some(base_fee.saturating_mul(U256::from(3))), Some(base_fee.saturating_mul(U256::from(2)))),
//                 };
//                 (None, max_fee, max_priority_fee)
//             }
//         };
//         match (gas_price, max_fee, max_priority_fee) {
//             (Some(gas_price), None, None) => {
//                 // Legacy or Eip-2930
//                 if let Some(access_list) = t.access_list {
//                     // Eip-2930
//                     Some(TransactionV2::EIP2930(EIP2930Transaction {
//                         chain_id: 0,
//                         nonce: U256::MAX, // To be set at pallet level
//                         gas_price,
//                         gas_limit: t.gas_limit,
//                         action: t.action,
//                         value: t.value,
//                         input: t.input,
//                         access_list: from_tuple_to_access_list(access_list),
//                         odd_y_parity: true,
//                         r: H256::default(),
//                         s: H256::default(),
//                     }))
//                 } else {
//                     // Legacy
//                     Some(TransactionV2::Legacy(LegacyTransaction {
//                         nonce: U256::MAX, // To be set at pallet level
//                         gas_price,
//                         gas_limit: t.gas_limit,
//                         action: t.action,
//                         value: t.value,
//                         input: t.input,
//                         signature: TransactionSignature {
//                             v: TransactionRecoveryId(0),
//                             r: H256::default(),
//                             s: H256::default(),
//                         },
//                     }))
//                 }
//             }
//             (None, Some(max_fee), _) => {
//                 // Eip-1559
//                 Some(TransactionV2::EIP1559(
//                     EIP1559Transaction {
//                         chain_id: 0,
//                         nonce: U256::MAX, // To be set at pallet level
//                         max_fee_per_gas: max_fee,
//                         max_priority_fee_per_gas: max_priority_fee.unwrap_or_else(U256::zero),
//                         gas_limit: t.gas_limit,
//                         action: t.action,
//                         value: t.value,
//                         input: t.input,
//                         access_list: from_tuple_to_access_list(t.access_list.unwrap_or_default()),
//                         odd_y_parity: true,
//                         r: H256::default(),
//                         s: H256::default(),
//                     }
//                 ))
//             }
//             _ => return None
//         }
//     }
// }
