
use std::sync::Arc;
use frontier_template_runtime::{
    AccountId, Balance, Call, Runtime, System, TransactionConverter, UncheckedExtrinsic, Ethereum,
    VERSION, Address, Signature, GasPricePrioritizer,
};
use sp_runtime::{
    transaction_validity::TransactionValidity,
    traits::{BlakeTwo256, ValidateUnsigned, Verify, IdentifyAccount},
    generic::{SignedPayload, Era},
};
use sp_core::{H160, Encode, Pair};
use std::str::FromStr;
use pallet_ethereum::Transaction;
use pallet_evm::{AddressMapping, HashedAddressMapping};
use fp_rpc::ConvertTransaction;
use sp_keyring::AccountKeyring;

pub const DEV: Balance = 1_000_000_000_000_000_000;

type AccountPublic = <Signature as Verify>::Signer;

struct ExtBuilder {
	balances: Vec<(AccountId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> ExtBuilder {
		ExtBuilder {
			balances: vec![]
		}
	}
}

impl ExtBuilder {
	fn with_balances(mut self, balances: Vec<(AccountId, Balance)>) -> Self {
		self.balances = balances;
		self
	}
	fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::default()
			.build_storage::<Runtime>()
			.unwrap();

		pallet_balances::GenesisConfig::<Runtime> {
			balances: self.balances,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}

#[test]
fn fair_priority() {
    
    let alice = Arc::new(AccountKeyring::Alice.pair());
    let bob = Arc::new(AccountKeyring::Bob.pair());

    ExtBuilder::default()
    .with_balances({
        let alith = H160::from_str("6be02d1d3665660d22ff9624b7be0551ee1ac91b")
            .expect("internal H160 is valid; qed");
        let alith32 = <HashedAddressMapping<BlakeTwo256>>::into_account_id(alith);
        vec![
            (alith32, 2_000_000 * DEV),
            (AccountId::from(alice.public()), 2_000 * DEV),
            (AccountId::from(bob.public()), 2_000 * DEV),
        ]
    })
    .build()
    .execute_with(|| {
        // Ethereum transaction
        //
        // Simple transfer from prefunded account with gas_price 0x01.
        // {from: 0x6be02d1d3665660d22ff9624b7be0551ee1ac91b, .., gasPrice: "0x01"}
        let bytes = hex_literal::hex!("f8628001831000009411111111111111111111111111111111111111118202008077a01088d8a9b9eae76258111a8e143668d82d086da1a4cb7a8b5f41df17cc404f1ca0517ea14a803b40c2f731e6961cbba0aff6d70397d8868a8eedfb6e2912ff0c23");

        let transaction = rlp::decode::<Transaction>(&bytes[..]);
        assert!(transaction.is_ok());
        let converter = TransactionConverter;

        let transaction = transaction.unwrap();
        // Create extrinsic
        let uxt: UncheckedExtrinsic = converter.convert_transaction(transaction.clone());
        // Validate unsigned on pallet ethereum
        let validity: Option<TransactionValidity> = match &uxt.function {
            Call::Ethereum(inner_call) => {
                Some(Ethereum::validate_unsigned(
                    sp_runtime::transaction_validity::TransactionSource::External,
                    &inner_call
                ))
            },
            _ => None
        };
        assert!(validity.is_some());
        let valid_tx = validity.unwrap().expect("Ethereum transaction valid.");
        let priority_ethereum = valid_tx.priority;

        // Substrate transfer
        //
        // Transfer from Alice to Bob.
        let from: Address = AccountPublic::from(alice.public()).into_account().into();
        let to: Address = AccountPublic::from(bob.public()).into_account().into();

        // Sign payload
        let function = Call::Balances(pallet_balances::Call::<Runtime>::transfer(to.into(), 1_000 * DEV));

        let check_spec_version = frame_system::CheckSpecVersion::<Runtime>::new();
        let check_tx_version = frame_system::CheckTxVersion::<Runtime>::new();
        let check_genesis = frame_system::CheckGenesis::<Runtime>::new();
        let check_era = frame_system::CheckEra::<Runtime>::from(Era::Immortal);
        let check_nonce = frame_system::CheckNonce::<Runtime>::from(0);
        let check_weight = frame_system::CheckWeight::<Runtime>::new();
        let payment = pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0);
        let extra = (
            check_spec_version,
            check_tx_version,
            check_genesis,
            check_era,
            check_nonce,
            check_weight,
            payment,
        );
        let genesis_hash = System::block_hash(0);
        let raw_payload = SignedPayload::from_raw(
            function,
            extra,
            (VERSION.spec_version, VERSION.transaction_version, genesis_hash, genesis_hash, (), (), ())
        );
        let signer = alice.clone();
        let signature = raw_payload.using_encoded(|payload|	{
            signer.sign(payload)
        });

        let (function, extra, _) = raw_payload.deconstruct();
        // Create extrinsic
        let uxtb: UncheckedExtrinsic = UncheckedExtrinsic::new_signed(
            function,
            from.into(),
            signature.into(),
            extra,
        ).into();
        // Reprioritize
        let reprioritized = GasPricePrioritizer::validate_and_prioritize(
            sp_runtime::transaction_validity::TransactionSource::Local,
            uxtb.clone(),
        );
        let valid_transfer = reprioritized.expect("Substrate transfer valid.");
        let priority_substrate = valid_transfer.priority;

        // Both extrinsics represent a simple transfer. Both extrinsics are a ValidTransaction.
        // Expect both to be reprioritized in a fair way. 
        assert_eq!(
            priority_ethereum,
            priority_substrate,
        );
    });
}
