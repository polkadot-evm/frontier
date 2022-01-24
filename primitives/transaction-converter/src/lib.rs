//! Ethereum transaction converter logic.

use fp_transaction_converter_api::TransactionConverterApi;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

/// Ethereum transaction converter to an extrinsic via runtime.
pub struct RuntimeTransactionConverter<Block: BlockT, Client> {
	/// The client provides access to the runtime.
	client: Arc<Client>,
	/// The type of the block used in the chain.
	_phantom_block: PhantomData<Block>,
}

/// An error that can occur during transaction convertation.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeTransactionConverterError {
	/// Something went wrong while converting transaction via the runtime.
	#[error("unable to convert transaction: {0}")]
	UnableToConvertTransaction(sp_api::ApiError),
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
	type Error = RuntimeTransactionConverterError;

	fn convert_transaction(
		&self,
		transaction: ethereum::TransactionV2,
	) -> Result<Block::Extrinsic, RuntimeTransactionConverterError> {
		let at = sp_api::BlockId::Hash(self.client.info().best_hash);
		let converted_transaction = self
			.client
			.runtime_api()
			.convert_transaction(&at, transaction)
			.map_err(RuntimeTransactionConverterError::UnableToConvertTransaction)?;
		Ok(converted_transaction)
	}
}
