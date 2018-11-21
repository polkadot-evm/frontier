// Copyright 2018 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! The Substrate runtime. This can be compiled with ``#[no_std]`, ready for Wasm.

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit="256"]

#[macro_use]
extern crate sr_api as runtime_api;

#[macro_use]
extern crate srml_support;

#[macro_use]
extern crate sr_primitives as runtime_primitives;

#[cfg(feature = "std")]
#[macro_use]
extern crate serde_derive;

extern crate substrate_primitives;

#[macro_use]
extern crate parity_codec_derive;

extern crate sr_std as rstd;
extern crate srml_consensus as consensus;
extern crate srml_session as session;
extern crate srml_system as system;
extern crate srml_timestamp as timestamp;
extern crate srml_upgrade_key as upgrade_key;
#[macro_use]
extern crate sr_version as version;
extern crate node_primitives;
extern crate rlp;
#[macro_use]
extern crate rlp_derive;
extern crate keccak_hasher;

pub mod ethereum;

use rstd::prelude::*;
use node_primitives::{AccountId, BlockNumber, Hash, Index, SessionKey, Signature};
use runtime_api::{runtime::*, id::*};
use runtime_primitives::ApplyResult;
use runtime_primitives::transaction_validity::TransactionValidity;
use runtime_primitives::generic;
use runtime_primitives::traits::{Convert, BlakeTwo256, Block as BlockT};
use version::RuntimeVersion;
#[cfg(any(feature = "std", test))]
use version::NativeVersion;

#[cfg(any(feature = "std", test))]
pub use runtime_primitives::BuildStorage;
pub use consensus::Call as ConsensusCall;
pub use timestamp::Call as TimestampCall;
pub use runtime_primitives::{Permill, Perbill};
pub use timestamp::BlockPeriod;
pub use srml_support::{StorageValue, RuntimeMetadata};

const TIMESTAMP_SET_POSITION: u32 = 0;
const NOTE_OFFLINE_POSITION: u32 = 1;

/// Runtime version.
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: ver_str!("node"),
	impl_name: ver_str!("substrate-node"),
	authoring_version: 1,
	spec_version: 1,
	impl_version: 0,
	apis: apis_vec!([
		(BLOCK_BUILDER, 1),
		(TAGGED_TRANSACTION_QUEUE, 1),
		(METADATA, 1)
	]),
};

/// Native version.
#[cfg(any(feature = "std", test))]
pub fn native_version() -> NativeVersion {
	NativeVersion {
		runtime_version: VERSION,
		can_author_with: Default::default(),
	}
}

impl system::Trait for Runtime {
	type Origin = Origin;
	type Index = Index;
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hashing = BlakeTwo256;
	type Digest = generic::Digest<Log>;
	type AccountId = AccountId;
	type Header = generic::Header<BlockNumber, BlakeTwo256, Log>;
	type Event = Event;
	type Log = Log;
}

impl consensus::Trait for Runtime {
	const NOTE_OFFLINE_POSITION: u32 = NOTE_OFFLINE_POSITION;
	type Log = Log;
	type SessionKey = SessionKey;
	type OnOfflineValidator = ();
}

impl timestamp::Trait for Runtime {
	const TIMESTAMP_SET_POSITION: u32 = TIMESTAMP_SET_POSITION;
	type Moment = u64;
}

/// Session key conversion.
pub struct SessionKeyConversion;
impl Convert<AccountId, SessionKey> for SessionKeyConversion {
	fn convert(a: AccountId) -> SessionKey {
		a.to_fixed_bytes().into()
	}
}

impl session::Trait for Runtime {
	type ConvertAccountIdToSessionKey = SessionKeyConversion;
	type OnSessionChange = ();
	type Event = Event;
}

impl upgrade_key::Trait for Runtime {
	type Event = Event;
}

impl ethereum::Trait for Runtime { }

construct_runtime!(
	pub enum Runtime with Log(InternalLog: DigestItem<Hash, SessionKey>) where
		Block = Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: system::{default, Log(ChangesTrieRoot)},
		Ethereum: ethereum::{Module, Call, Storage, Config<T>},
		Timestamp: timestamp::{Module, Call, Storage, Config<T>, Inherent},
		Consensus: consensus::{Module, Call, Storage, Config<T>, Log(AuthoritiesChange), Inherent},
		Session: session,
		UpgradeKey: upgrade_key,
	}
);

/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256, Log>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Header, UncheckedExtrinsic>;
/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic = generic::UncheckedMortalExtrinsic<AccountId, Index, Call, Signature>;
/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, Index, Call>;

impl_apis! {
	impl Core<Block, SessionKey> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn authorities() -> Vec<SessionKey> {
			Consensus::authorities()
		}

		fn execute_block(block: Block) {
			unimplemented!()
		}
	}

	impl Metadata<RuntimeMetadata> for Runtime {
		fn metadata() -> RuntimeMetadata {
			Runtime::metadata()
		}
	}

	impl BlockBuilder<Block, InherentData, UncheckedExtrinsic, InherentData, InherentError> for Runtime {
		fn initialise_block(header: <Block as BlockT>::Header) {
			unimplemented!()
		}

		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyResult {
			unimplemented!()
		}

		fn finalise_block() -> <Block as BlockT>::Header {
			unimplemented!()
		}

		fn inherent_extrinsics(data: InherentData) -> Vec<UncheckedExtrinsic> {
			data.create_inherent_extrinsics()
		}

		fn check_inherents(block: Block, data: InherentData) -> Result<(), InherentError> {
			data.check_inherents(block)
		}

		fn random_seed() -> <Block as BlockT>::Hash {
			System::random_seed()
		}
	}

	impl TaggedTransactionQueue<Block, TransactionValidity> for Runtime {
		fn validate_transaction(tx: <Block as BlockT>::Extrinsic) -> TransactionValidity {
			unimplemented!()
		}
	}
}
