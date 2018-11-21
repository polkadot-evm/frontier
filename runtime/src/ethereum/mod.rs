mod state;

use system;
use substrate_primitives::Hasher;
use runtime_primitives;
use node_primitives::{H160, U256, H256};
use rstd::collections::btree_map::BTreeMap;
use rlp;

pub use self::state::BasicAccount;

use keccak_hasher::KeccakHasher;

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
