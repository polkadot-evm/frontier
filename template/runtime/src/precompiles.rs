use sp_core::H160;
use sp_std::marker::PhantomData;

use pallet_evm::{
	IsPrecompileResult, Precompile, PrecompileHandle, PrecompileResult, PrecompileSet,
};
use pallet_evm_precompile_blake2::Blake2F;
use pallet_evm_precompile_bn128::{Bn128Add, Bn128Mul, Bn128Pairing};
use pallet_evm_precompile_modexp::Modexp;
use pallet_evm_precompile_sha3fips::Sha3FIPS256;
use pallet_evm_precompile_simple::{ECRecover, ECRecoverPublicKey, Identity, Ripemd160, Sha256};
use precompile_utils::prelude::revert;

pub struct FrontierPrecompiles<R>(PhantomData<R>);

impl<R> FrontierPrecompiles<R>
where
	R: pallet_evm::Config,
{
	pub fn new() -> Self {
		Self(Default::default())
	}
	pub fn used_addresses() -> [H160; 11] {
		[
			hash(1),
			hash(2),
			hash(3),
			hash(4),
			hash(5),
			hash(6),
			hash(7),
			hash(8),
			hash(9),
			hash(1024),
			hash(1025),
		]
	}
}
impl<R> PrecompileSet for FrontierPrecompiles<R>
where
	R: pallet_evm::Config,
{
	fn execute(&self, handle: &mut impl PrecompileHandle) -> Option<PrecompileResult> {
		let (code_addr, context_addr) = (handle.code_address(), handle.context().address);
		// For chains that possess their own stateful precompiles, it is advisable to activate this verification measure.
		// This measure prohibits the use of DELEGATECALL or CALLCODE for any precompiles other than the official Ethereum precompiles, which are inherently stateless.
		if Self::used_addresses().contains(&code_addr)
			&& code_addr > hash(9)
			&& code_addr != context_addr
		{
			return Some(Err(revert(
				"cannot be called with DELEGATECALL or CALLCODE",
			)));
		};

		match handle.code_address() {
			// Ethereum precompiles :
			a if a == hash(1) => Some(ECRecover::execute(handle)),
			a if a == hash(2) => Some(Sha256::execute(handle)),
			a if a == hash(3) => Some(Ripemd160::execute(handle)),
			a if a == hash(4) => Some(Identity::execute(handle)),
			a if a == hash(5) => Some(Modexp::execute(handle)),
			a if a == hash(6) => Some(Bn128Add::execute(handle)),
			a if a == hash(7) => Some(Bn128Mul::execute(handle)),
			a if a == hash(8) => Some(Bn128Pairing::execute(handle)),
			a if a == hash(9) => Some(Blake2F::execute(handle)),
			// Non-Frontier specific nor Ethereum precompiles :
			a if a == hash(1024) => Some(Sha3FIPS256::execute(handle)),
			a if a == hash(1025) => Some(ECRecoverPublicKey::execute(handle)),
			_ => None,
		}
	}

	fn is_precompile(&self, address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: Self::used_addresses().contains(&address),
			extra_cost: 0,
		}
	}
}

fn hash(a: u64) -> H160 {
	H160::from_low_u64_be(a)
}
