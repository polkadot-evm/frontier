use pallet_evmless::{Precompile, PrecompileHandle, PrecompileResult, PrecompileSet};
use sp_core::{H160, U256};
use sp_std::marker::PhantomData;

use pallet_evmless::EvmlessFungiblesPrecompiles;
use pallet_evmless_precompile_fungibles::{AssetIdOf, BalanceOf, Fungibles};

use precompile_utils::EvmData;

pub struct FrontierPrecompiles<R>(PhantomData<R>);

impl<R> FrontierPrecompiles<R>
where
	R: pallet_evmless::Config,
{
	pub fn new() -> Self {
		Self(Default::default())
	}
}
impl<R> PrecompileSet for FrontierPrecompiles<R>
where
	R: pallet_evmless::Config,
	AssetIdOf<R>: From<u32>,
	BalanceOf<R>: EvmData + Into<U256>,
	<R as frame_system::Config>::AccountId: From<H160>,
{
	fn execute(&self, handle: &mut impl PrecompileHandle) -> Option<PrecompileResult> {
		match handle.code_address() {
			// EVMless
			address if EvmlessFungiblesPrecompiles::<R>::contains_key(address) => {
				Some(Fungibles::<R>::execute(handle))
			}
			_ => None,
		}
	}

	fn is_precompile(&self, address: H160) -> bool {
		EvmlessFungiblesPrecompiles::<R>::contains_key(address)
	}
}
