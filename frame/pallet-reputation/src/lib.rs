#![cfg_attr(not(feature = "std"), no_std)]

//pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        dispatch::DispatchResult,
        pallet_prelude::*,
        traits::{Currency, Get, BuildGenesisConfig},
    };

    use frame_system::pallet_prelude::*;
    use sp_runtime::{
        traits::{CheckedAdd, CheckedSub, Saturating, Zero},
        DispatchError,
    };  
    use sp_std::vec::Vec;
    use super::*;

    /// El tipo para representar balances.
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;


    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // --- Config Trait ---
    // Define las dependencias y tipos genéricos que el pallet necesita del runtime.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// El evento del runtime, necesario para emitir eventos.
        //  type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        //  type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
 
        /// El tipo de moneda para manejar balances.
        type Currency: Currency<Self::AccountId>;

        /// El tipo de balance para el token de reputación.
        type Balance: Parameter
            + Member
            + sp_runtime::traits::AtLeast32BitUnsigned
            + Default
            + Copy
            + MaybeSerializeDeserialize
            + MaxEncodedLen
            + CheckedAdd
            + CheckedSub
            + Saturating
            + Zero;
    }



    // --- Storage ---
    // Define el almacenamiento del pallet.
    #[pallet::storage]
    #[pallet::getter(fn balances)]
    pub type Balances<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, T::Balance, ValueQuery>;

    // --- Events ---
    // Define los eventos que el pallet puede emitir.
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Se ha acuñado una nueva cantidad de tokens para una cuenta. [cuenta, cantidad]
        Minted(T::AccountId, T::Balance),
        /// Se ha quemado una cantidad de tokens de una cuenta. [cuenta, cantidad]
        Burned(T::AccountId, T::Balance),
        /// Un valor genérico almacenado en el almacenamiento. [clave, cuenta
        SomethingStored(u32, T::AccountId),
    }

    #[pallet::storage]
    #[pallet::getter(fn reputation)]
    pub type Reputation<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    // --- Errors ---
    // Define los errores que el pallet puede devolver.
    #[pallet::error]
    pub enum Error<T> {
        /// El balance es demasiado bajo para la operación de quemado.
        BalanceTooLow,
    }

/*
    // --- Genesis Configuration ---
    // Permite inicializar el estado del pallet en el bloque génesis.
    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        /// Lista de balances iniciales para las cuentas.
        pub balances: Vec<(T::AccountId, T::Balance)>,
    }
*/
 //   use frame_support::traits::BuildGenesisConfig;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub reputations: Vec<(T::AccountId, u32)>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for (acc, rep) in &self.reputations {
                Reputation::<T>::insert(acc, rep);
            }
        }
    }
/*
    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { balances: Vec::new() }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            for (acc, balance) in &self.balances {
                <Balances<T>>::insert(acc, balance);
            }
        }
    }
*/


    // --- Dispatchable Calls ---
    // Define las funciones que pueden ser llamadas desde fuera del runtime (extrinsics).
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Acuña una `cantidad` de tokens a la cuenta `dest`.
        ///
        /// Solo puede ser llamado por el origen `Root`.
        #[pallet::call_index(0)]
        /// #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        /// #[pallet::weight(T::DbWeight::get().writes(1).saturating_add(10_000u64.into()))]
        /// #[pallet::weight(T::DbWeight::get().writes(1).saturating_add(frame_support::weights::Weight::from_parts(10_000, 0)))]
        #[pallet::weight(
            T::DbWeight::get()
            .writes(1)
            .saturating_add(frame_support::weights::Weight::from_parts(10_000, 0))
        )]


        pub fn mint(origin: OriginFor<T>, dest: T::AccountId, amount: T::Balance) -> DispatchResult {
            ensure_root(origin)?;
            Self::mint_into(&dest, amount)
        }

        /// Quema una `cantidad` de tokens de la cuenta `source`.
        
        #[pallet::call_index(1)]
        // #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        #[pallet::weight(
            T::DbWeight::get()
            .writes(1)
            .saturating_add(frame_support::weights::Weight::from_parts(10_000, 0))
        )]

        pub fn burn(origin: OriginFor<T>, source: T::AccountId, amount: T::Balance) -> DispatchResult {
            ensure_root(origin)?;
            Self::burn_from(&source, amount)
        }
    }

    // --- Internal Functions ---
    // Lógica interna del pallet, no directamente expuesta como extrinsics.
    impl<T: Config> Pallet<T> {
        /// Función interna para acuñar tokens.
        pub fn mint_into(acc: &T::AccountId, amount: T::Balance) -> DispatchResult {
            if amount.is_zero() {
                return Ok(());
            }
            <Balances<T>>::try_mutate(acc, |balance| -> DispatchResult {
                *balance = balance.checked_add(&amount).ok_or(DispatchError::Arithmetic(ArithmeticError::Overflow))?;

                use sp_runtime::ArithmeticError;

                Ok(())
            })?;
            Self::deposit_event(Event::Minted(acc.clone(), amount));
            Ok(())
        }

        /// Función interna para quemar tokens.
        pub fn burn_from(acc: &T::AccountId, amount: T::Balance) -> DispatchResult {
            if amount.is_zero() {
                return Ok(());
            }
            <Balances<T>>::try_mutate(acc, |balance| -> DispatchResult {
                *balance = balance.checked_sub(&amount).ok_or(Error::<T>::BalanceTooLow)?;
                Ok(())
            })?;
            Self::deposit_event(Event::Burned(acc.clone(), amount));
            Ok(())
        }
    }
}

// --- Tests ---
// Pruebas unitarias para asegurar que la lógica del pallet es correcta.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{new_test_ext, Reputation, RuntimeEvent, Test};
    use frame_support::{assert_noop, assert_ok};
    use frame_system::ensure_root;

    #[test]
    fn genesis_config_works() {
        new_test_ext(vec![(1, 100), (2, 200)]).execute_with(|| {
            assert_eq!(Reputation::balances(1), 100);
            assert_eq!(Reputation::balances(2), 200);
            assert_eq!(Reputation::balances(3), 0);
        });
    }

    #[test]
    fn minting_works() {
        new_test_ext(vec![]).execute_with(|| {
            // Acuñar 100 tokens a la cuenta 1
            assert_ok!(Reputation::mint(ensure_root(None).unwrap(), 1, 100));
            assert_eq!(Reputation::balances(1), 100);

            // Verificar que el evento fue emitido
            System::assert_last_event(RuntimeEvent::Reputation(Event::Minted(1, 100)));

            // Acuñar más tokens a la misma cuenta
            assert_ok!(Reputation::mint(ensure_root(None).unwrap(), 1, 50));
            assert_eq!(Reputation::balances(1), 150);
            System::assert_last_event(RuntimeEvent::Reputation(Event::Minted(1, 50)));
        });
    }

    #[test]
    fn minting_zero_does_nothing() {
        new_test_ext(vec![]).execute_with(|| {
            assert_ok!(Reputation::mint(ensure_root(None).unwrap(), 1, 0));
            assert_eq!(Reputation::balances(1), 0);
            // No se emite evento para acuñación de cero
            assert_ne!(
                System::last_event(),
                Some(RuntimeEvent::Reputation(Event::Minted(1, 0)))
            );
        });
    }

    #[test]
    fn minting_fails_for_non_root() {
        new_test_ext(vec![]).execute_with(|| {
            assert_noop!(
                Reputation::mint(RuntimeOrigin::signed(1), 1, 100),
                DispatchError::BadOrigin
            );
        });
    }

    #[test]
    fn burning_works() {
        new_test_ext(vec![(1, 100)]).execute_with(|| {
            // Quemar 30 tokens de la cuenta 1
            assert_ok!(Reputation::burn(ensure_root(None).unwrap(), 1, 30));
            assert_eq!(Reputation::balances(1), 70);

            // Verificar que el evento fue emitido
            System::assert_last_event(RuntimeEvent::Reputation(Event::Burned(1, 30)));

            // Quemar el resto
            assert_ok!(Reputation::burn(ensure_root(None).unwrap(), 1, 70));
            assert_eq!(Reputation::balances(1), 0);
            System::assert_last_event(RuntimeEvent::Reputation(Event::Burned(1, 70)));
        });
    }

    #[test]
    fn burning_fails_if_balance_too_low() {
        new_test_ext(vec![(1, 50)]).execute_with(|| {
            assert_noop!(
                Reputation::burn(ensure_root(None).unwrap(), 1, 100),
                Error::<Test>::BalanceTooLow
            );
        });
    }

    #[test]
    fn burning_fails_for_non_root() {
        new_test_ext(vec![(1, 100)]).execute_with(|| {
            assert_noop!(
                Reputation::burn(RuntimeOrigin::signed(1), 1, 50),
                DispatchError::BadOrigin
            );
        });
    }
}

// --- Mock Runtime for Tests ---
// Un entorno simulado para ejecutar las pruebas unitarias.
#[cfg(test)]
mod mock {
    use super::*;
    use frame_support::{
        parameter_types,
        traits::{ConstU32, ConstU64},
    };
    use sp_core::H256;
    use sp_runtime::{
        testing::Header,
        traits::{BlakeTwo256, IdentityLookup},
    };

    type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
    type Block = frame_system::mocking::MockBlock<Test>;

    frame_support::construct_runtime!(
        pub enum Test where
            Block = Block,
            NodeBlock = Block,
            UncheckedExtrinsic = UncheckedExtrinsic,
        {
            System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
            Reputation: pallet_reputation::{Pallet, Call, Storage, Event<T>, GenesisConfig},
        }
    );

    impl frame_system::Config for Test {
        type BaseCallFilter = frame_support::traits::Everything;
        type BlockWeights = ();
        type BlockLength = ();
        type DbWeight = ();
        type RuntimeOrigin = RuntimeOrigin;
        type RuntimeCall = RuntimeCall;
        type Index = u64;
        type BlockNumber = u64;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = u64;
        type Lookup = IdentityLookup<Self::AccountId>;
        type Header = Header;
        type RuntimeEvent = RuntimeEvent;
        type BlockHashCount = ConstU64<250>;
        type Version = ();
        type PalletInfo = PalletInfo;
        type AccountData = ();
        type OnNewAccount = ();
        type OnKilledAccount = ();
        type SystemWeightInfo = ();
        type SS58Prefix = ();
        type OnSetCode = ();
        type MaxConsumers = ConstU32<16>;
    }

    parameter_types! {
        pub const ExistentialDeposit: u64 = 1;
    }

    impl pallet_reputation::Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type Currency = (); // No se usa directamente, pero es requerido por el trait.
        type Balance = u128;
    }

    /// Construye un test externalities con una configuración de génesis.
    pub fn new_test_ext(balances: Vec<(u64, u128)>) -> sp_io::TestExternalities {
        let mut storage = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
        pallet_reputation::GenesisConfig::<Test> { balances }
            .assimilate_storage(&mut storage)
            .unwrap();
        storage.into()
    }
}

