use ethereum_types::{H256, H64, U256};
use jsonrpc_core::Result;

use fc_rpc_core::{types::*, EthMiningApi as EthMiningApiT};

pub struct EthMiningApi {
	is_authority: bool,
}

impl EthMiningApi {
	pub fn new(is_authority: bool) -> Self {
		Self { is_authority }
	}
}

impl EthMiningApiT for EthMiningApi {
	fn is_mining(&self) -> Result<bool> {
		Ok(self.is_authority)
	}

	fn hashrate(&self) -> Result<U256> {
		Ok(U256::zero())
	}

	fn work(&self) -> Result<Work> {
		Ok(Work::default())
	}

	fn submit_hashrate(&self, _: U256, _: H256) -> Result<bool> {
		Ok(false)
	}

	fn submit_work(&self, _: H64, _: H256, _: H256) -> Result<bool> {
		Ok(false)
	}
}
