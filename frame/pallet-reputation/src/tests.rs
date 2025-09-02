use super::*;
use crate::mock::{ExtBuilder, Reputation, System, Test};
use frame_support::{assert_noop, assert_ok};

const ALICE: u64 = 1;
const BOB: u64 = 2;

#[test]
fn genesis_config_works() {
    ExtBuilder::default()
        .with_balances(vec![(ALICE, 100), (BOB, 200)])
        .build()
        .execute_with(|| {
            assert_eq!(Reputation::balance(&ALICE), 100);
            assert_eq!(Reputation::balance(&BOB), 200);
        });
}

#[test]
fn mint_works() {
    ExtBuilder::default().build().execute_with(|| {
        // Mint 100 a ALICE
        assert_ok!(Reputation::mint(&ALICE, 100));
        assert_eq!(Reputation::balance(&ALICE), 100);

        // Verificar que el evento fue emitido
        System::assert_last_event(crate::Event::Minted { who: ALICE, amount: 100 }.into());

        // Mint 50 más a ALICE
        assert_ok!(Reputation::mint(&ALICE, 50));
        assert_eq!(Reputation::balance(&ALICE), 150);
        System::assert_last_event(crate::Event::Minted { who: ALICE, amount: 50 }.into());
    });
}

#[test]
fn burn_works() {
    ExtBuilder::default()
        .with_balances(vec![(ALICE, 100)])
        .build()
        .execute_with(|| {
            // Burn 30 de ALICE
            assert_ok!(Reputation::burn(&ALICE, 30));
            assert_eq!(Reputation::balance(&ALICE), 70);

            // Verificar que el evento fue emitido
            System::assert_last_event(crate::Event::Burned { who: ALICE, amount: 30 }.into());

            // Burn los 70 restantes
            assert_ok!(Reputation::burn(&ALICE, 70));
            assert_eq!(Reputation::balance(&ALICE), 0);
            System::assert_last_event(crate::Event::Burned { who: ALICE, amount: 70 }.into());
        });
}

#[test]
fn burn_fails_if_insufficient_balance() {
    ExtBuilder::default()
        .with_balances(vec![(ALICE, 50)])
        .build()
        .execute_with(|| {
            // Intentar quemar más de lo que tiene
            assert_noop!(
                Reputation::burn(&ALICE, 100),
                Error::<Test>::InsufficientBalance
            );

            // El balance no debe haber cambiado
            assert_eq!(Reputation::balance(&ALICE), 50);
        });
}

#[test]
fn zero_amount_operations_do_nothing() {
    ExtBuilder::default().build().execute_with(|| {
        assert_ok!(Reputation::mint(&ALICE, 0));
        assert_eq!(Reputation::balance(&ALICE), 0);

        assert_ok!(Reputation::burn(&ALICE, 0));
        assert_eq!(Reputation::balance(&ALICE), 0);

        // No se deben emitir eventos para operaciones de cero
        assert_eq!(System::events().len(), 0);
    });
}
