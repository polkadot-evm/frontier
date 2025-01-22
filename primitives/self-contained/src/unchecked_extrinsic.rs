// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2017-2020 Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::{ExtrinsicCall, InherentBuilder, SignedTransactionBuilder},
};
use scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::{
	generic::{self, Preamble},
	traits::{
		self, Checkable, Dispatchable, ExtrinsicLike, ExtrinsicMetadata, IdentifyAccount,
		MaybeDisplay, Member, TransactionExtension,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	OpaqueExtrinsic, RuntimeDebug,
};

use crate::{CheckedExtrinsic, CheckedSignature, SelfContainedCall};

/// A extrinsic right from the external world. This is unchecked and so
/// can contain a signature.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct UncheckedExtrinsic<
	Address,
	Call: Dispatchable,
	Signature,
	Extension: TransactionExtension<Call>,
>(pub generic::UncheckedExtrinsic<Address, Call, Signature, Extension>);

impl<Address, Call: Dispatchable, Signature, Extra: TransactionExtension<Call>>
	UncheckedExtrinsic<Address, Call, Signature, Extra>
{
	/// New instance of a signed extrinsic aka "transaction".
	pub fn new_signed(function: Call, signed: Address, signature: Signature, extra: Extra) -> Self {
		Self(generic::UncheckedExtrinsic::new_signed(
			function, signed, signature, extra,
		))
	}

	/// New instance of an unsigned extrinsic aka "inherent".
	pub fn new_bare(function: Call) -> Self {
		Self(generic::UncheckedExtrinsic::new_bare(function))
	}
}

impl<Address: TypeInfo, Call, Signature: TypeInfo, Extension> ExtrinsicLike
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Call: TypeInfo + Dispatchable,
	Extension: TypeInfo + TransactionExtension<Call>,
{
	fn is_bare(&self) -> bool {
		ExtrinsicLike::is_bare(&self.0)
	}
}

impl<Address, AccountId, Call, Signature, Extension, Lookup> Checkable<Lookup>
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Address: Member + MaybeDisplay,
	Call: Encode + Member + SelfContainedCall,
	Signature: Member + traits::Verify,
	<Signature as traits::Verify>::Signer: IdentifyAccount<AccountId = AccountId>,
	Extension: Encode + TransactionExtension<Call>,
	AccountId: Member + MaybeDisplay,
	Lookup: traits::Lookup<Source = Address, Target = AccountId>,
{
	type Checked =
		CheckedExtrinsic<AccountId, Call, Extension, <Call as SelfContainedCall>::SignedInfo>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		if self.0.function.is_self_contained() {
			if matches!(self.0.preamble, Preamble::Signed(_, _, _)) {
				return Err(TransactionValidityError::Invalid(
					InvalidTransaction::BadProof,
				));
			}

			let signed_info = self.0.function.check_self_contained().ok_or(
				TransactionValidityError::Invalid(InvalidTransaction::BadProof),
			)??;
			Ok(CheckedExtrinsic {
				signed: CheckedSignature::SelfContained(signed_info),
				function: self.0.function,
			})
		} else {
			let checked = Checkable::<Lookup>::check(self.0, lookup)?;
			Ok(CheckedExtrinsic {
				signed: CheckedSignature::GenericDelegated(checked.format),
				function: checked.function,
			})
		}
	}

	#[cfg(feature = "try-runtime")]
	fn unchecked_into_checked_i_know_what_i_am_doing(
		self,
		lookup: &Lookup,
	) -> Result<Self::Checked, TransactionValidityError> {
		use generic::ExtrinsicFormat;
		if self.0.function.is_self_contained() {
			match self.0.function.check_self_contained() {
				Some(signed_info) => Ok(CheckedExtrinsic {
					signed: match signed_info {
						Ok(info) => CheckedSignature::SelfContained(info),
						_ => CheckedSignature::GenericDelegated(ExtrinsicFormat::Bare),
					},
					function: self.0.function,
				}),
				None => Ok(CheckedExtrinsic {
					signed: CheckedSignature::GenericDelegated(ExtrinsicFormat::Bare),
					function: self.0.function,
				}),
			}
		} else {
			let checked =
				Checkable::<Lookup>::unchecked_into_checked_i_know_what_i_am_doing(self.0, lookup)?;
			Ok(CheckedExtrinsic {
				signed: CheckedSignature::GenericDelegated(checked.format),
				function: checked.function,
			})
		}
	}
}

impl<Address, Call, Signature, Extra> ExtrinsicMetadata
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Call: Dispatchable,
	Extra: TransactionExtension<Call>,
{
	const VERSIONS: &'static [u8] =
		generic::UncheckedExtrinsic::<Address, Call, Signature, Extra>::VERSIONS;
	type TransactionExtensions = Extra;
}

impl<Address, Call, Signature, Extension> ExtrinsicCall
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Address: TypeInfo,
	Call: SelfContainedCall + TypeInfo,
	Signature: TypeInfo,
	Extension: TransactionExtension<Call>,
{
	type Call = Call;

	fn call(&self) -> &Self::Call {
		&self.0.function
	}
}

impl<Address, Call, Signature, Extension> GetDispatchInfo
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Call: GetDispatchInfo + Dispatchable,
	Extension: TransactionExtension<Call>,
{
	fn get_dispatch_info(&self) -> DispatchInfo {
		self.0.function.get_dispatch_info()
	}
}

#[cfg(feature = "serde")]
impl<Address: Encode, Signature: Encode, Call, Extension> serde::Serialize
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Call: Encode + Dispatchable,
	Extension: Encode + TransactionExtension<Call>,
{
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error>
	where
		S: ::serde::Serializer,
	{
		self.0.serialize(seq)
	}
}

#[cfg(feature = "serde")]
impl<'a, Address: Decode, Signature: Decode, Call, Extension> serde::Deserialize<'a>
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Call: Decode + Dispatchable,
	Extension: Decode + TransactionExtension<Call>,
{
	fn deserialize<D>(de: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'a>,
	{
		<generic::UncheckedExtrinsic<Address, Call, Signature, Extension>>::deserialize(de)
			.map(Self)
	}
}

impl<Address, Signature, Call, Extension> SignedTransactionBuilder
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Address: TypeInfo,
	Signature: TypeInfo,
	Call: TypeInfo + SelfContainedCall,
	Extension: TypeInfo + TransactionExtension<Call>,
{
	type Address = Address;
	type Signature = Signature;
	type Extension = Extension;

	fn new_signed_transaction(
		call: Self::Call,
		signed: Address,
		signature: Signature,
		tx_ext: Extension,
	) -> Self {
		generic::UncheckedExtrinsic::new_signed(call, signed, signature, tx_ext).into()
	}
}

impl<Address, Signature, Call, Extension> InherentBuilder
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Address: TypeInfo,
	Signature: TypeInfo,
	Call: TypeInfo + SelfContainedCall,
	Extension: TypeInfo + TransactionExtension<Call>,
{
	fn new_inherent(call: Self::Call) -> Self {
		generic::UncheckedExtrinsic::new_bare(call).into()
	}
}

impl<Address, Call, Signature, Extension>
	From<UncheckedExtrinsic<Address, Call, Signature, Extension>> for OpaqueExtrinsic
where
	Address: Encode,
	Signature: Encode,
	Call: Encode + SelfContainedCall,
	Extension: TransactionExtension<Call>,
{
	fn from(extrinsic: UncheckedExtrinsic<Address, Call, Signature, Extension>) -> Self {
		extrinsic.0.into()
	}
}

impl<Address, Call, Signature, Extension>
	From<generic::UncheckedExtrinsic<Address, Call, Signature, Extension>>
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Call: Dispatchable,
	Extension: TransactionExtension<Call>,
{
	fn from(utx: generic::UncheckedExtrinsic<Address, Call, Signature, Extension>) -> Self {
		Self(utx)
	}
}
