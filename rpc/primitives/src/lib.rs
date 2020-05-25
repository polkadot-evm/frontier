#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::prelude::*;
use ethereum_types::{H160, H256, U256};

use ethereum::{Block as EthBlock, Account as EthAccount};

sp_api::decl_runtime_apis! {
	/// API necessary for Ethereum-compatibility layer.
	pub trait EthRuntimeApi {
		// eth_gasPrice
		/// pallet_evm::FeeCalculator::min_gas_price()
		fn min_gas_price() -> U256;

		// eth_accounts
		/// <pallet_evm::Module<T>>::accounts()
		/// The list of keys in the EVM pallet Accounts StorageMap.
		fn evm_accounts() -> Vec<H160>;

		// eth_blockNumber
		/// <frame_system::Module<T>>::block_number()
		fn current_block_number() -> u32;

		// eth_getBalance
		/// <pallet_evm::Module<T>>::accounts(H160), pallet_evm::backend::Account.balance.
		/// The Account struct is part of the EVM pallet and has a balance field.
		fn account_balance(_: H160) -> U256;

		// eth_getBlockByHash
		/// ethereum::Block in <pallet_evm::Module<T>>::BlocksAndReceipts
		/// Requires conversion from Block::Hash to BlockNumber.
		// fn block_by_hash(_: Block::Hash) -> EthBlock;

		// eth_getBlockByNumber
		// ethereum::Block in <pallet_evm::Module<T>>::BlocksAndReceipts
		// fn block_by_number(_: u32) -> EthBlock;

		// eth_getTransactionCount
		/// This requires previous conversion from 'latest','earliest','pending' to BlockNumber.
		/// Is this up to BlockNumber or in the specified BlockNumber?
		fn address_transaction_count(_: H160, _: u32) -> U256;

		// eth_getBlockTransactionCountByHash
		fn transaction_count_by_hash(_: H160) -> U256;

		// eth_getBlockTransactionCountByNumber
		fn transaction_count_by_number(_: u32) -> U256;

		// eth_getCode
		/// This should be in pallet_evm::backend::Account,
		/// but the field is not implemented?
		fn bytecode_from_address(_: H160, _: u32) -> Vec<u8>;

		// eth_sendRawTransaction and eth_submitTransaction
		// <pallet_ethereum::Module<T>>::execute()?
		fn execute(_: Vec<u8>) -> H256;

		// eth_call
		// /// <pallet_evm::Module<T>>::execute_call()?
		// fn execute_call(_: CallRequest, _: u32) -> Vec<u8>;

		// eth_estimateGas
		// /// Requires source bytecode gas conversion table. Didn't find a reference in the pallet_evm.
		// /// Related to the EVM crate https://github.com/sorpaas/rust-evm/blob/master/src/executor/stack.rs#L746
		// fn virtual_call(_: CallRequest, _: u32) -> U256;

		// // eth_getTransactionByHash
		// // TODO no StorageMap by ethereum::transaction::Transaction Hash.
		// fn transaction_by_hash(_: H256) -> Transaction;

		// // eth_getTransactionByBlockHashAndIndex
		// /// ethereum::Block.transactions[Index] in <pallet_evm::Module<T>>::BlocksAndReceipts
		// /// Requires conversion from Block::Hash to BlockNumber.
		// fn transaction_by_block_hash(_: H256, _: Index) -> Transaction;

		// // eth_getTransactionByBlockNumberAndIndex
		// /// ethereum::Block.transactions[Index] in <pallet_evm::Module<T>>::BlocksAndReceipts
		// fn transaction_by_block_number(_: H256, _: Index) -> Transaction;

		// // eth_getTransactionReceipt
		// // TODO no StorageMap by ethereum::transaction::Transaction Hash.
		// fn transaction_receipt(_: H256) -> Receipt;
	}
}