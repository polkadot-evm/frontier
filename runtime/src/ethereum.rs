use system;
use substrate_primitives::Hasher;
use runtime_primitives;
use node_primitives::{H160, U256, H256};
use rstd::collections::btree_map::BTreeMap;
use rlp;

#[cfg(feature = "std")]
use keccak_hasher::KeccakHasher;

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

pub trait Trait: system::Trait { }

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {

	}
}

decl_storage! {
	trait Store for Module<T: Trait> as Ethereum { }

	add_extra_genesis {
		config(accounts): BTreeMap<H160, BasicAccount>;
		config(_phantom): ::std::marker::PhantomData<T>;
		build(|storage: &mut runtime_primitives::StorageMap, children_storage: &mut runtime_primitives::ChildrenStorageMap, config: &GenesisConfig<T>| {
			let mut accounts = runtime_primitives::StorageMap::default();

			for (address, account) in &config.accounts {
				accounts.insert(KeccakHasher::hash(address.as_ref()).as_ref().to_vec(),
								rlp::encode(account));
			}

			children_storage.insert(b":child_storage:eth:accounts".to_vec(), accounts);
		});
	}
}
