use std::{marker::PhantomData, sync::Arc};

// Substrate
use sc_client_api::{AuxStore, UsageProvider};
use sp_api::ProvideRuntimeApi;
use sp_consensus_aura::{
	digests::CompatibleDigestItem,
	sr25519::{AuthorityId, AuthoritySignature},
	AuraApi, Slot, SlotDuration,
};
use sp_inherents::InherentData;
use sp_runtime::{traits::Block as BlockT, Digest, DigestItem};
use sp_timestamp::TimestampInherentData;
// Frontier
use fc_rpc::pending::ConsensusDataProvider;

/// Consensus data provider for Aura.
pub struct AuraConsensusDataProvider<B, C> {
	// slot duration
	slot_duration: SlotDuration,
	// phantom data for required generics
	_phantom: PhantomData<(B, C)>,
}

impl<B, C> AuraConsensusDataProvider<B, C>
where
	B: BlockT,
	C: AuxStore + ProvideRuntimeApi<B> + UsageProvider<B>,
	C::Api: AuraApi<B, AuthorityId>,
{
	/// Creates a new instance of the [`AuraConsensusDataProvider`], requires that `client`
	/// implements [`sp_consensus_aura::AuraApi`]
	pub fn new(client: Arc<C>) -> Self {
		let slot_duration = sc_consensus_aura::slot_duration(&*client)
			.expect("slot_duration is always present; qed.");
		Self {
			slot_duration,
			_phantom: PhantomData,
		}
	}
}

impl<B: BlockT, C: Send + Sync> ConsensusDataProvider<B> for AuraConsensusDataProvider<B, C> {
	fn create_digest(
		&self,
		_parent: &B::Header,
		data: &InherentData,
	) -> Result<Digest, sp_inherents::Error> {
		let timestamp = data
			.timestamp_inherent_data()?
			.expect("Timestamp is always present; qed");

		let digest_item = <DigestItem as CompatibleDigestItem<AuthoritySignature>>::aura_pre_digest(
			Slot::from_timestamp(timestamp, self.slot_duration),
		);

		Ok(Digest {
			logs: vec![digest_item],
		})
	}
}
