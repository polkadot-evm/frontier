#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    dispatch::DispatchResult,
    pallet_prelude::*,
};
use frame_system::pallet_prelude::*;
use sp_std::vec::Vec;

/// Pallet cAuth: registro de nodos validadores autorizados
#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_system::pallet_prelude::BlockNumberFor;
   
    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// Configuración del pallet
    #[pallet::config]
    pub trait Config: frame_system::Config {
        // /// The overarching runtime event type.
        //type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    /// Nodos activos de cAuth: (AccountId => BlockNumber de registro)
    #[pallet::storage]
    #[pallet::getter(fn active_nodes)]
    pub type ActiveNodes<T: Config> = StorageMap<
        _, 
        Blake2_128Concat, 
        T::AccountId, 
        BlockNumberFor<T>,
        OptionQuery
    >;

    /// Eventos
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Un nodo se ha registrado como cAuth [account_id]
        NodeRegistered(T::AccountId),
        /// Un nodo se ha dado de baja como cAuth [account_id]
        NodeUnregistered(T::AccountId),
    }

    /// Errores
    #[pallet::error]
    pub enum Error<T> {
        /// El nodo ya está registrado
        AlreadyRegistered,
        /// El nodo no está registrado
        NotRegistered,
    }

    /// Hooks del pallet
    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    /// Genesis configuration
    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub initial_nodes: Vec<T::AccountId>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for node in &self.initial_nodes {
                let current_block = BlockNumberFor::<T>::default();
                ActiveNodes::<T>::insert(node, current_block);
            }
        }
    }

    /// Extrinsics
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Registrar un nodo como cAuth
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn register_node(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(
                !ActiveNodes::<T>::contains_key(&who),
                Error::<T>::AlreadyRegistered
            );

            let current_block = <frame_system::Pallet<T>>::block_number();
            ActiveNodes::<T>::insert(&who, current_block);

            Self::deposit_event(Event::NodeRegistered(who));
            Ok(())
        }

        /// Eliminar un nodo de cAuth
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn unregister_node(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(
                ActiveNodes::<T>::contains_key(&who),
                Error::<T>::NotRegistered
            );

            ActiveNodes::<T>::remove(&who);

            Self::deposit_event(Event::NodeUnregistered(who));
            Ok(())
        }
    }

    /// Implementaciones adicionales del pallet
    impl<T: Config> Pallet<T> {
        /// Verifica si un nodo está registrado
        pub fn is_node_registered(account: &T::AccountId) -> bool {
            ActiveNodes::<T>::contains_key(account)
        }

        /// Obtiene el bloque de registro de un nodo
        pub fn get_registration_block(account: &T::AccountId) -> Option<BlockNumberFor<T>> {
            ActiveNodes::<T>::get(account)
        }

        /// Obtiene todos los nodos registrados
        pub fn get_all_registered_nodes() -> Vec<T::AccountId> {
            ActiveNodes::<T>::iter_keys().collect()
        }
    }
}