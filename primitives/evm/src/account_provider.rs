//! Custom account provider logic.

use sp_runtime::traits::AtLeast32Bit;

/// The account provider interface abstraction layer.
///
/// Expose account related logic that `pallet_evm` required to control accounts existence
/// in the network and their transactions uniqueness. By default, the pallet operates native
/// system accounts records that `frame_system` provides.
///
/// The interface allow any custom account provider logic to be used instead of
/// just using `frame_system` account provider. The accounts records should store nonce value
/// for each account at least.
pub trait AccountProvider {
	/// The account identifier type.
	///
	/// Represent the account itself in accounts records.
	type AccountId;
	/// Account nonce type.
	///
	/// The number that helps to ensure that each transaction in the network is unique
	/// for particular account.
	type Nonce: AtLeast32Bit;

	/// Creates a new account in accounts records.
	///
	/// The account associated with new created address EVM.
	fn create_account(who: &Self::AccountId);
	/// Removes an account from accounts records.
	///
	/// The account associated with removed address from EVM.
	fn remove_account(who: &Self::AccountId);
	/// Return current account nonce value.
	///
	/// Used to represent account basic information in EVM format.
	fn account_nonce(who: &Self::AccountId) -> Self::Nonce;
	/// Increment a particular account's nonce value.
	///
	/// Incremented with each new transaction submitted by the account.
	fn inc_account_nonce(who: &Self::AccountId);
}
