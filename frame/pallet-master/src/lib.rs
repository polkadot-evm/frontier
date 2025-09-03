#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*};
use frame_system::pallet_prelude::*;
//use sp_std::vec::Vec;

//pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use sp_std::vec::Vec;
    
    use frame_support::traits::ConstU32;
    

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
       // type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    /// Ejemplo: almacenamos un ID de génesis en formato `BoundedVec`
    #[pallet::storage]
    #[pallet::getter(fn genesis_id)]
    pub type GenesisId<T: Config> =
        StorageValue<_, BoundedVec<u8, ConstU32<64>>, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        GenesisIdSet(Vec<u8>),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// El `Vec<u8>` que intentamos guardar es demasiado largo para el límite definido
        BoundedVecTooLong,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Guarda el `genesis_id`
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn set_genesis_id(origin: OriginFor<T>, genesis: Vec<u8>) -> DispatchResult {
            ensure_signed(origin)?;

            let bounded: BoundedVec<u8, ConstU32<64>> =
                genesis.clone().try_into().map_err(|_| Error::<T>::BoundedVecTooLong)?;

            GenesisId::<T>::put(bounded);

            Self::deposit_event(Event::GenesisIdSet(genesis));
            Ok(())
        }
    }
}
