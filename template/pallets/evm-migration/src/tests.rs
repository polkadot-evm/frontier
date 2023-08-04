

use crate::mock::*;
use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use sp_core::{H160, H256};
use sp_std::str::FromStr;

#[test]
fn migrate_account_codes_works() {
	new_test_ext().execute_with(|| {
		let addr_a = H160::from_str("0000000000000000000000000000000000000001").unwrap();
		let addr_b = H160::from_str("0000000000000000000000000000000000000002").unwrap();
		let addr_c = H160::from_str("0000000000000000000000000000000000000003").unwrap();

		let empty_code: Vec<u8> = vec![];
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_a), empty_code);
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_b), empty_code);
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_c), empty_code);

		let code_a: Vec<u8> = vec![0x01, 0x02, 0x03];
		let code_b: Vec<u8> = vec![0x04, 0x05, 0x06];
		let code_c: Vec<u8> = vec![0x07, 0x08, 0x09];

		let migration =
			vec![(addr_a, code_a.clone()), (addr_b, code_b.clone()), (addr_c, code_c.clone())];

		assert_ok!(EVMMigration::migrate_account_codes(RuntimeOrigin::root(), migration));

		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_a), code_a);
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_b), code_b);
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_c), code_c);
	});
}

#[test]
fn migrate_account_codes_non_root_fails() {
	new_test_ext().execute_with(|| {
		let addr_a = H160::from_str("0000000000000000000000000000000000000001").unwrap();
		let addr_b = H160::from_str("0000000000000000000000000000000000000002").unwrap();
		let addr_c = H160::from_str("0000000000000000000000000000000000000003").unwrap();

		let empty_code: Vec<u8> = vec![];
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_a), empty_code);
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_b), empty_code);
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_c), empty_code);

		let code_a: Vec<u8> = vec![0x01, 0x02, 0x03];
		let code_b: Vec<u8> = vec![0x04, 0x05, 0x06];
		let code_c: Vec<u8> = vec![0x07, 0x08, 0x09];

		let migration =
			vec![(addr_a, code_a.clone()), (addr_b, code_b.clone()), (addr_c, code_c.clone())];

		assert_noop!(
			EVMMigration::migrate_account_codes(RuntimeOrigin::signed(addr_a), migration),
			BadOrigin
		);

		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_a), empty_code);
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_b), empty_code);
		assert_eq!(pallet_evm::AccountCodes::<Test>::get(addr_c), empty_code);
	});
}

#[test]
fn migrate_account_storages_works() {
	new_test_ext().execute_with(|| {
		let addr_a = H160::from_str("0000000000000000000000000000000000000001").unwrap();
		let addr_b = H160::from_str("0000000000000000000000000000000000000002").unwrap();
		let addr_c = H160::from_str("0000000000000000000000000000000000000003").unwrap();

		let slot_a_1: H256 = H256([0xa1; 32]);
		let slot_a_2: H256 = H256([0xa2; 32]);
		let slot_a_3: H256 = H256([0xa3; 32]);

		let slot_b_1: H256 = H256([0xb1; 32]);
		let slot_b_2: H256 = H256([0xb2; 32]);
		let slot_b_3: H256 = H256([0xb3; 32]);

		let slot_c_1: H256 = H256([0xc1; 32]);
		let slot_c_2: H256 = H256([0xc2; 32]);
		let slot_c_3: H256 = H256([0xc3; 32]);

		let empty_value = H256([0u8; 32]);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_1), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_2), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_3), empty_value);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_1), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_2), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_3), empty_value);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_1), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_2), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_3), empty_value);

		let value_a_1: H256 = H256([0x01; 32]);
		let value_a_2: H256 = H256([0x02; 32]);
		let value_a_3: H256 = H256([0x03; 32]);
		let storage_a: Vec<(H256, H256)> =
			vec![(slot_a_1, value_a_1), (slot_a_2, value_a_2), (slot_a_3, value_a_3)];

		let value_b_1: H256 = H256([0x04; 32]);
		let value_b_2: H256 = H256([0x05; 32]);
		let value_b_3: H256 = H256([0x06; 32]);
		let storage_b: Vec<(H256, H256)> =
			vec![(slot_b_1, value_b_1), (slot_b_2, value_b_2), (slot_b_3, value_b_3)];

		let value_c_1: H256 = H256([0x07; 32]);
		let value_c_2: H256 = H256([0x08; 32]);
		let value_c_3: H256 = H256([0x09; 32]);
		let storage_c: Vec<(H256, H256)> =
			vec![(slot_c_1, value_c_1), (slot_c_2, value_c_2), (slot_c_3, value_c_3)];

		assert_eq!(
			EVMMigration::migrate_account_storage(RuntimeOrigin::root(), addr_a, storage_a),
			Ok(())
		);
		assert_eq!(
			EVMMigration::migrate_account_storage(RuntimeOrigin::root(), addr_b, storage_b),
			Ok(())
		);
		assert_eq!(
			EVMMigration::migrate_account_storage(RuntimeOrigin::root(), addr_c, storage_c),
			Ok(())
		);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_1), value_a_1);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_2), value_a_2);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_3), value_a_3);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_1), value_b_1);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_2), value_b_2);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_3), value_b_3);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_1), value_c_1);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_2), value_c_2);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_3), value_c_3);
	});
}

#[test]
fn migrate_account_storages_non_root_fails() {
	new_test_ext().execute_with(|| {
		let addr_a = H160::from_str("0000000000000000000000000000000000000001").unwrap();
		let addr_b = H160::from_str("0000000000000000000000000000000000000002").unwrap();
		let addr_c = H160::from_str("0000000000000000000000000000000000000003").unwrap();

		let slot_a_1: H256 = H256([0xa1; 32]);
		let slot_a_2: H256 = H256([0xa2; 32]);
		let slot_a_3: H256 = H256([0xa3; 32]);

		let slot_b_1: H256 = H256([0xb1; 32]);
		let slot_b_2: H256 = H256([0xb2; 32]);
		let slot_b_3: H256 = H256([0xb3; 32]);

		let slot_c_1: H256 = H256([0xc1; 32]);
		let slot_c_2: H256 = H256([0xc2; 32]);
		let slot_c_3: H256 = H256([0xc3; 32]);

		let empty_value = H256([0u8; 32]);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_1), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_2), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_3), empty_value);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_1), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_2), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_3), empty_value);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_1), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_2), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_3), empty_value);

		let value_a_1: H256 = H256([0x01; 32]);
		let value_a_2: H256 = H256([0x02; 32]);
		let value_a_3: H256 = H256([0x03; 32]);
		let storage_a: Vec<(H256, H256)> =
			vec![(slot_a_1, value_a_1), (slot_a_2, value_a_2), (slot_a_3, value_a_3)];

		let value_b_1: H256 = H256([0x04; 32]);
		let value_b_2: H256 = H256([0x05; 32]);
		let value_b_3: H256 = H256([0x06; 32]);
		let storage_b: Vec<(H256, H256)> =
			vec![(slot_b_1, value_b_1), (slot_b_2, value_b_2), (slot_b_3, value_b_3)];

		let value_c_1: H256 = H256([0x07; 32]);
		let value_c_2: H256 = H256([0x08; 32]);
		let value_c_3: H256 = H256([0x09; 32]);
		let storage_c: Vec<(H256, H256)> =
			vec![(slot_c_1, value_c_1), (slot_c_2, value_c_2), (slot_c_3, value_c_3)];

		assert_noop!(
			EVMMigration::migrate_account_storage(RuntimeOrigin::signed(addr_a), addr_a, storage_a),
			BadOrigin
		);
		assert_noop!(
			EVMMigration::migrate_account_storage(RuntimeOrigin::signed(addr_a), addr_b, storage_b),
			BadOrigin
		);
		assert_noop!(
			EVMMigration::migrate_account_storage(RuntimeOrigin::signed(addr_a), addr_c, storage_c),
			BadOrigin
		);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_1), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_2), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_a, slot_a_3), empty_value);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_1), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_2), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_b, slot_b_3), empty_value);

		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_1), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_2), empty_value);
		assert_eq!(pallet_evm::AccountStorages::<Test>::get(addr_c, slot_c_3), empty_value);
	});
}

#[test]
fn migrate_account_balances_and_nonces() {
	new_test_ext().execute_with(|| {
		let addr_a = H160::from_str("0000000000000000000000000000000000000001").unwrap();
		let addr_b = H160::from_str("0000000000000000000000000000000000000002").unwrap();
		let addr_c = H160::from_str("0000000000000000000000000000000000000003").unwrap();

		assert_eq!(Balances::free_balance(addr_a), 0);
		assert_eq!(Balances::free_balance(addr_b), 0);
		assert_eq!(Balances::free_balance(addr_c), 0);

		assert_eq!(System::account_nonce(addr_a), 0);
		assert_eq!(System::account_nonce(addr_b), 0);
		assert_eq!(System::account_nonce(addr_c), 0);

		let balance_a = 1;
		let balance_b = 2;
		let balance_c = 3;

		let nonce_a = 111;
		let nonce_b = 222;
		let nonce_c = 333;

		let migration = vec![
			(addr_a, balance_a, nonce_a),
			(addr_b, balance_b, nonce_b),
			(addr_c, balance_c, nonce_c),
		];

		assert_ok!(EVMMigration::migrate_account_balances_and_nonces(
			RuntimeOrigin::root(),
			migration
		));

		// todo

		assert_eq!(Balances::free_balance(addr_a), balance_a);
		assert_eq!(Balances::free_balance(addr_b), balance_b);
		assert_eq!(Balances::free_balance(addr_c), balance_c);

		assert_eq!(System::account_nonce(addr_a), nonce_a);
		assert_eq!(System::account_nonce(addr_b), nonce_b);
		assert_eq!(System::account_nonce(addr_c), nonce_c);
	});
}

#[test]
fn migrate_account_balances_and_nonces_non_root_fails() {
	new_test_ext().execute_with(|| {
		let addr_a = H160::from_str("0000000000000000000000000000000000000001").unwrap();
		let addr_b = H160::from_str("0000000000000000000000000000000000000002").unwrap();
		let addr_c = H160::from_str("0000000000000000000000000000000000000003").unwrap();

		assert_eq!(Balances::free_balance(addr_a), 0);
		assert_eq!(Balances::free_balance(addr_b), 0);
		assert_eq!(Balances::free_balance(addr_c), 0);

		assert_eq!(System::account_nonce(addr_a), 0);
		assert_eq!(System::account_nonce(addr_b), 0);
		assert_eq!(System::account_nonce(addr_c), 0);

		let balance_a = 1;
		let balance_b = 2;
		let balance_c = 3;

		let nonce_a = 111;
		let nonce_b = 222;
		let nonce_c = 333;

		let migration = vec![
			(addr_a, balance_a, nonce_a),
			(addr_b, balance_b, nonce_b),
			(addr_c, balance_c, nonce_c),
		];

		assert_noop!(
			EVMMigration::migrate_account_balances_and_nonces(
				RuntimeOrigin::signed(addr_a),
				migration
			),
			BadOrigin
		);

		assert_eq!(Balances::free_balance(addr_a), 0);
		assert_eq!(Balances::free_balance(addr_b), 0);
		assert_eq!(Balances::free_balance(addr_c), 0);

		assert_eq!(System::account_nonce(addr_a), 0);
		assert_eq!(System::account_nonce(addr_b), 0);
		assert_eq!(System::account_nonce(addr_c), 0);
	});
}
