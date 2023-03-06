use pallet_evmless::{Precompile, PrecompileHandle, PrecompileResult, PrecompileSet};
use sp_core::{H160, U256};
use sp_std::marker::PhantomData;

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
	pub fn used_addresses() -> [H160; 1] {
		[
			hash(1337),
			// ...
		]
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
			a if a == hash(1337) => Some(Fungibles::<R>::execute(handle)),
			_ => None,
		}
	}

	fn is_precompile(&self, address: H160) -> bool {
		Self::used_addresses().contains(&address)
	}
}

fn hash(a: u64) -> H160 {
	H160::from_low_u64_be(a)
}
