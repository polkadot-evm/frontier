//! The runtime API.

#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::prelude::*;

sp_api::decl_runtime_apis! {
	/// Runtime API for the transaction converter.
	pub trait TransactionConverterApi {
		/// Convert an ethereum transaction to an extrinsic.
		fn convert_transaction(transaction: ethereum::TransactionV2) -> Block::Extrinsic;
	}
}
