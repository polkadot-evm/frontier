use system;
use runtime_primitives;

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
			storage.insert(vec![0x00, 0x00], vec![0x01, 0x01]);
		});
	}
}
