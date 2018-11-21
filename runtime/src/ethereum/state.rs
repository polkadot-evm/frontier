use keccak_hasher::{KeccakHasher, KECCAK_EMPTY, KECCAK_NULL_RLP};
use substrate_primitives::Hasher;
use node_primitives::{H160, U256, H256};
use runtime_io;

pub const ACCOUNT_KEY: &[u8] = b":child_storage:eth:accounts";
pub const ACCOUNT_CODE_KEY: &[u8] = b":child_storage:eth:codes";
pub const ACCOUNT_STORAGE_KEY_PREFIX: &[u8] = b":child_storage:eth:storage:";

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

fn read_account(address: H160) -> Option<BasicAccount> {
	runtime_io::child_storage(ACCOUNT_KEY, &KeccakHasher::hash(address.as_ref()).as_ref())
		.map(|val| rlp::decode(&val).expect("Non-corrupt database always have valid BasicAccount encodings; qed"))
}

fn write_account(address: H160, account: Option<BasicAccount>) {
	let key = KeccakHasher::hash(address.as_ref());

	match account {
		Some(account) => runtime_io::set_child_storage(
			ACCOUNT_KEY, &key.as_ref(), &rlp::encode(&account)[..]
		),
		None => runtime_io::clear_child_storage(
			ACCOUNT_KEY, &key.as_ref()
		),
	}
}

fn read_account_storage(address: H160, storage: H256) -> Option<H256> {
	let key = ACCOUNT_STORAGE_KEY_PREFIX
		.iter()
		.cloned()
		.chain(address.as_ref().iter().cloned())
		.collect::<Vec<_>>();

	runtime_io::child_storage(&key, &KeccakHasher::hash(storage.as_ref()).as_ref())
		.map(|val| {
			assert!(val.len() == 32, "Non-corrupt database always have storage with 32 byte value; qed");
			H256::from_slice(&val[..])
		})
}

fn write_account_storage(address: H160, storage: H256, value: Option<H256>) {
	let key = ACCOUNT_STORAGE_KEY_PREFIX
		.iter()
		.cloned()
		.chain(address.as_ref().iter().cloned())
		.collect::<Vec<_>>();
	let storage_key = KeccakHasher::hash(storage.as_ref());

	match value {
		Some(value) => runtime_io::set_child_storage(
			&key, &storage_key.as_ref(), &value.as_ref()
		),
		None => runtime_io::clear_child_storage(
			&key, &storage_key.as_ref()
		),
	}
}

fn kill_account_storage(address: H160) {
	let key = ACCOUNT_STORAGE_KEY_PREFIX
		.iter()
		.cloned()
		.chain(address.as_ref().iter().cloned())
		.collect::<Vec<_>>();

	runtime_io::kill_child_storage(&key)
}

fn account_storage_root(address: H160) -> H256 {
	let key = ACCOUNT_STORAGE_KEY_PREFIX
		.iter()
		.cloned()
		.chain(address.as_ref().iter().cloned())
		.collect::<Vec<_>>();

	let root_raw = runtime_io::child_storage_root(&key).expect("Child storage always exists by current trie rule; qed");
	assert!(root_raw.len() == 32, "Account storage is under child storage by keccak256; hash is always 32 bytes; qed");
	H256::from_slice(&root_raw[..])
}

fn read_account_code(hash: H256) -> Option<Vec<u8>> {
	if hash == H256::from(&KECCAK_EMPTY) {
		Some(Vec::new())
	} else {
		runtime_io::child_storage(ACCOUNT_CODE_KEY, &hash.as_ref())
	}
}

fn note_account_code(code: Vec<u8>) {
	runtime_io::set_child_storage(ACCOUNT_CODE_KEY, &KeccakHasher::hash(&code).as_ref(), &code)
}
