use pallet_evm::{Precompile, PrecompileHandle, PrecompileResult, PrecompileSet};
use sp_core::H160;
use sp_std::marker::PhantomData;

use pallet_evm_precompile_modexp::Modexp;
use pallet_evm_precompile_sha3fips::Sha3FIPS256;
use pallet_evm_precompile_simple::{ECRecover, ECRecoverPublicKey, Identity, Ripemd160, Sha256};

pub struct FrontierPrecompiles<R>(PhantomData<R>);

impl<R> FrontierPrecompiles<R>
where
	R: pallet_evm::Config,
{
	pub fn new() -> Self {
		Self(Default::default())
	}
	pub fn used_addresses() -> sp_std::vec::Vec<H160> {
		sp_std::vec![1, 2, 3, 4, 5, 1024, 1025]
			.into_iter()
			.map(hash)
			.collect()
	}
}
impl<R> PrecompileSet for FrontierPrecompiles<R>
where
	R: pallet_evm::Config,
{
	fn execute(&self, handle: &mut impl PrecompileHandle) -> Option<PrecompileResult> {
		let address = handle.code_address();
		let input = &handle.input().to_vec();
		let target_gas = handle.gas_limit();
		let context = &handle.context().clone();
		let is_static = handle.is_static();

		match address {
			// Ethereum precompiles :
			a if a == hash(1) => Some(ECRecover::execute(
				handle, input, target_gas, context, is_static,
			)),
			a if a == hash(2) => Some(Sha256::execute(
				handle, input, target_gas, context, is_static,
			)),
			a if a == hash(3) => Some(Ripemd160::execute(
				handle, input, target_gas, context, is_static,
			)),
			a if a == hash(4) => Some(Identity::execute(
				handle, input, target_gas, context, is_static,
			)),
			a if a == hash(5) => Some(Modexp::execute(
				handle, input, target_gas, context, is_static,
			)),
			// Non-Frontier specific nor Ethereum precompiles :
			a if a == hash(1024) => Some(Sha3FIPS256::execute(
				handle, input, target_gas, context, is_static,
			)),
			a if a == hash(1025) => Some(ECRecoverPublicKey::execute(
				handle, input, target_gas, context, is_static,
			)),
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
