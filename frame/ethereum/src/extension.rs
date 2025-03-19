//! Extensions for unsigned general extrinsic for ethereum pallet

use crate::pallet::Call as EthereumCall;
use crate::{Config, Origin, Pallet as Ethereum, RawOrigin};
use ethereum_types::H160;
use fp_evm::TransactionValidationError;
use frame_support::pallet_prelude::{PhantomData, TypeInfo};
use frame_system::pallet_prelude::{OriginFor, RuntimeCallFor};
use scale_codec::{Decode, Encode};
use scale_info::prelude::fmt;
use sp_runtime::impl_tx_ext_default;
use sp_runtime::traits::{
	AsSystemOriginSigner, DispatchInfoOf, DispatchOriginOf, Dispatchable, Implication,
	TransactionExtension, ValidateResult,
};
use sp_runtime::transaction_validity::{
	InvalidTransaction, TransactionSource, TransactionValidityError, ValidTransaction,
};

/// Trait to be implemented by the Runtime call for Ethereum call conversion and additional validations.
pub trait EthereumTransactionHook<Runtime>
where
	Runtime: Config,
	OriginFor<Runtime>: Into<Result<RawOrigin, OriginFor<Runtime>>>,
{
	fn maybe_ethereum_call(&self) -> Option<&EthereumCall<Runtime>>;

	fn additional_validation(
		&self,
		_signer: H160,
		_source: TransactionSource,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}
}

/// Extensions for pallet-ethereum unsigned extrinsics.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct EthereumExtension<Runtime>(PhantomData<Runtime>);

impl<Runtime> EthereumExtension<Runtime> {
	pub fn new() -> Self {
		Self(PhantomData)
	}
}

impl<Runtime> Default for EthereumExtension<Runtime> {
	fn default() -> Self {
		Self::new()
	}
}

impl<T: Config> fmt::Debug for EthereumExtension<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "EthereumExtension",)
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
		Ok(())
	}
}

impl<Runtime> EthereumExtension<Runtime> where
	Runtime: Config + scale_info::TypeInfo + fmt::Debug + Send + Sync
{
}

impl<Runtime> TransactionExtension<RuntimeCallFor<Runtime>> for EthereumExtension<Runtime>
where
	Runtime: Config + scale_info::TypeInfo + fmt::Debug + Send + Sync,
	<RuntimeCallFor<Runtime> as Dispatchable>::RuntimeOrigin:
		AsSystemOriginSigner<<Runtime as frame_system::Config>::AccountId> + From<Origin> + Clone,
	OriginFor<Runtime>: Into<Result<RawOrigin, OriginFor<Runtime>>>,
	RuntimeCallFor<Runtime>: EthereumTransactionHook<Runtime>,
{
	const IDENTIFIER: &'static str = "EthereumExtension";
	type Implicit = ();
	type Val = ();
	type Pre = ();

	fn validate(
		&self,
		origin: DispatchOriginOf<RuntimeCallFor<Runtime>>,
		call: &RuntimeCallFor<Runtime>,
		_info: &DispatchInfoOf<RuntimeCallFor<Runtime>>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Implication,
		source: TransactionSource,
	) -> ValidateResult<Self::Val, RuntimeCallFor<Runtime>> {
		// we only care about unsigned calls
		if origin.as_system_origin_signer().is_some() {
			return Ok((ValidTransaction::default(), (), origin));
		};

		let transaction = match call.maybe_ethereum_call() {
			Some(EthereumCall::transact { transaction }) => transaction,
			_ => return Ok((ValidTransaction::default(), (), origin)),
		};

		// check signer
		let signer = Ethereum::<Runtime>::recover_signer(transaction).ok_or(
			InvalidTransaction::Custom(TransactionValidationError::InvalidSignature as u8),
		)?;

		// validation on transactions based on pre_dispatch or mempool validation.
		let pre_dispatch = source == TransactionSource::InBlock;
		let validity = if pre_dispatch {
			Ethereum::<Runtime>::validate_transaction_in_block(signer, transaction)
				.map(|_| ValidTransaction::default())
		} else {
			Ethereum::<Runtime>::validate_transaction_in_pool(signer, transaction)
		}?;

		// do any additional validations
		call.additional_validation(signer, source)?;

		Ok((validity, (), Origin::EthereumTransaction(signer).into()))
	}

	impl_tx_ext_default!(RuntimeCallFor<Runtime>; prepare weight);
}
