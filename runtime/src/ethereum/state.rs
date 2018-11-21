use keccak_hasher::{KECCAK_EMPTY, KECCAK_NULL_RLP};
use node_primitives::{H160, U256, H256};

pub const ACCOUNT_KEY: &str = ":child_storage:eth:accounts";
pub const ACCOUNT_STORAGE_KEY_PREFIX: &str = ":child_storage:eth:storage:";

/// Basic account type.
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BasicAccount {
	/// Nonce of the account.
	pub nonce: U256,
	/// Balance of the account.
	pub balance: U256,
	/// Storage root of the account.
	pub storage_root: H256,
	/// Code hash of the account.
	pub code_hash: H256,
}

impl Default for BasicAccount {
	fn default() -> Self {
		Self {
			nonce: U256::zero(),
			balance: U256::zero(),
			storage_root: H256::from(&KECCAK_NULL_RLP),
			code_hash: H256::from(&KECCAK_EMPTY),
		}
	}
}
