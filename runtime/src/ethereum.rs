use system;
use runtime_primitives;
use node_primitives::{U256, H256};

/// Basic account type.
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
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

pub trait Trait: system::Trait { }

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {

	}
}

decl_storage! {
	trait Store for Module<T: Trait> as Ethereum { }

	add_extra_genesis {
		config(_phantom): ::std::marker::PhantomData<T>;
		build(|storage: &mut runtime_primitives::StorageMap, children_storage: &mut runtime_primitives::ChildrenStorageMap, config: &GenesisConfig<T>| {
			let mut accounts = runtime_primitives::StorageMap::default();
			accounts.insert(b"do".to_vec(), b"verb".to_vec());
			accounts.insert(b"ether".to_vec(), b"wookiedoo".to_vec());
			accounts.insert(b"horse".to_vec(), b"stallion".to_vec());
			accounts.insert(b"shaman".to_vec(), b"horse".to_vec());
			accounts.insert(b"doge".to_vec(), b"coin".to_vec());
			accounts.insert(b"ether".to_vec(), vec![]);
			accounts.insert(b"dog".to_vec(), b"puppy".to_vec());
			accounts.insert(b"shaman".to_vec(), vec![]);

			children_storage.insert(b":child_storage:eth:accounts".to_vec(), accounts);
			storage.insert(vec![0x00, 0x00], vec![0x01, 0x01]);
		});
	}
}
