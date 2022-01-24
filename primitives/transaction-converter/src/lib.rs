use fp_transaction_converter_api::TransactionConverterApi;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

/// Ethereum transaction converter to an extrinsic.
pub struct RuntimeTransactionConverter<Block: BlockT, Client> {
	/// The client provides access to the runtime.
	client: Arc<Client>,
	/// The type of the block used in the chain.
	_phantom_block: PhantomData<Block>,
}

impl<Block: BlockT, Client> RuntimeTransactionConverter<Block, Client> {
	/// Create a new [`RuntimeTransactionConverter`].
	pub fn new(client: Arc<Client>) -> Self {
		Self {
			client,
			_phantom_block: PhantomData,
		}
	}
}

impl<Block: BlockT, Client> fp_rpc::ConvertTransaction<Block::Extrinsic>
	for RuntimeTransactionConverter<Block, Client>
where
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block>,
	Client::Api: TransactionConverterApi<Block>,
{
	fn convert_transaction(&self, transaction: ethereum::TransactionV2) -> Block::Extrinsic {
		let at = sp_api::BlockId::Hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.convert_transaction(&at, transaction)
			.unwrap()
	}
}
