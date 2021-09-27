use scale_info::TypeInfo;
use codec::Encode;
use sp_core::{H160, H256};
use sp_runtime::{
	traits::{
		self, Checkable, Extrinsic, ExtrinsicMetadata, IdentifyAccount, MaybeDisplay, Member,
		SignedExtension,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
};
use sha3::{Digest, Keccak256};
use crate::{CheckedExtrinsic, CheckedSignature, MultiSignature, EthereumAddress};

/// A extrinsic right from the external world. This is unchecked and so
/// can contain a signature.
#[derive(PartialEq, Eq, Clone, TypeInfo)]
pub struct UncheckedExtrinsic<Address, Call, Extra: SignedExtension>(
	sp_runtime::generic::UncheckedExtrinsic<Address, Call, MultiSignature, Extra>,
);

#[cfg(feature = "std")]
impl<Address, Call, Extra> parity_util_mem::MallocSizeOf
	for UncheckedExtrinsic<Address, Call, Extra>
where
	Extra: SignedExtension,
{
	fn size_of(&self, ops: &mut parity_util_mem::MallocSizeOfOps) -> usize {
		self.0.size_of(ops)
	}
}

impl<Address, Call, Extra: SignedExtension>
	UncheckedExtrinsic<Address, Call, Extra>
{
	/// New instance of a signed extrinsic aka "transaction".
	pub fn new_signed(function: Call, signed: Address, signature: MultiSignature, extra: Extra) -> Self {
		Self(sp_runtime::generic::UncheckedExtrinsic::new_signed(function, signed, signature, extra))
	}

	/// New instance of an unsigned extrinsic aka "inherent".
	pub fn new_unsigned(function: Call) -> Self {
		Self(sp_runtime::generic::UncheckedExtrinsic::new_unsigned(function))
	}
}

impl<Address, Call, Extra: SignedExtension> Extrinsic
	for UncheckedExtrinsic<Address, Call, Extra>
{
	type Call = Call;

	type SignaturePayload = (Address, MultiSignature, Extra);

	fn is_signed(&self) -> Option<bool> {
		self.0.is_signed()
	}

	fn new(function: Call, signed_data: Option<Self::SignaturePayload>) -> Option<Self> {
		sp_runtime::generic::UncheckedExtrinsic::new(function, signed_data).map(Self)
	}
}

impl<Address, AccountId, Call, Extra, Lookup> Checkable<Lookup>
	for UncheckedExtrinsic<Address, Call, Extra>
where
	Address: Member + MaybeDisplay + EthereumAddress,
	Call: Encode + Member,
	MultiSignature: Member + traits::Verify,
	<MultiSignature as traits::Verify>::Signer: IdentifyAccount<AccountId = AccountId>,
	Extra: SignedExtension<AccountId = AccountId>,
	AccountId: Member + MaybeDisplay,
	Lookup: traits::Lookup<Source = Address, Target = AccountId>,
{
	type Checked = CheckedExtrinsic<AccountId, Call, Extra>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		let ethereum_address = self.0.signature.as_ref().and_then(|(address, _, _)| address.ethereum_address());

		match (ethereum_address, self.0) {
			(Some(ethereum_address), sp_runtime::generic::UncheckedExtrinsic {
				signature: Some((_, MultiSignature::EthereumTransaction(preimage_hash, signature), _)),
				function,
			}) => {
				let recovered_pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&signature.0, &preimage_hash.0).map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::BadProof))?;
				let recovered_address = H160::from(H256::from_slice(
					Keccak256::digest(&recovered_pubkey).as_slice(),
				));
				if recovered_address == ethereum_address && Extra::identifier().is_empty() {
					Ok(CheckedExtrinsic {
						signed: CheckedSignature::EthereumTransaction(preimage_hash, ethereum_address),
						function,
					})
				} else {
					Err(TransactionValidityError::Invalid(InvalidTransaction::BadProof))
				}
			},
			(_, extrinsic) => {
				let checked = Checkable::<Lookup>::check(extrinsic, lookup)?;
				Ok(CheckedExtrinsic {
					signed: match checked.signed {
						Some((id, extra)) => CheckedSignature::Signed(id, extra),
						None => CheckedSignature::Unsigned,
					},
					function: checked.function,
				})
			},
		}
	}
}

impl<Address, Call, Extra> ExtrinsicMetadata
	for UncheckedExtrinsic<Address, Call, Extra>
where
	Extra: SignedExtension,
{
	const VERSION: u8 = <sp_runtime::generic::UncheckedExtrinsic<Address, Call, MultiSignature, Extra> as ExtrinsicMetadata>::VERSION;
	type SignedExtensions = Extra;
}
