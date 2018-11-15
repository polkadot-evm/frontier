use system;
use runtime_primitives::{StorageMap, ChildrenStorageMap};

pub trait Trait: system::Trait { }

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {

	}
}

decl_storage! {
	trait Store for Module<T: Trait> as Ethereum { }

	add_extra_genesis {
		config(_marker) : ::std::marker::PhantomData<T>;
		build(|storage: &mut StorageMap, children_storage: &mut ChildrenStorageMap, config: &GenesisConfig<T>| {

		});
	}
}
