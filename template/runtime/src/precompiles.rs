use pallet_evm::{Precompile, PrecompileHandle, PrecompileResult, PrecompileSet};
use sp_core::H160;
use sp_std::marker::PhantomData;

use pallet_evm::Precompiles;
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
}
impl<R> PrecompileSet for FrontierPrecompiles<R>
where
	R: pallet_evm::Config,
{
	fn execute(&self, handle: &mut impl PrecompileHandle) -> Option<PrecompileResult> {
		match handle.code_address() {
			// Ethereum precompiles :
			a if *Precompiles::<R>::get(a) == b"ECRecover".to_vec() => {
				Some(ECRecover::execute(handle))
			}
			a if *Precompiles::<R>::get(a) == b"Sha256".to_vec() => Some(Sha256::execute(handle)),
			a if *Precompiles::<R>::get(a) == b"Ripemd160".to_vec() => {
				Some(Ripemd160::execute(handle))
			}
			a if *Precompiles::<R>::get(a) == b"Identity".to_vec() => {
				Some(Identity::execute(handle))
			}
			a if *Precompiles::<R>::get(a) == b"Modexp".to_vec() => Some(Modexp::execute(handle)),
			// Non-Frontier specific nor Ethereum precompiles :
			a if *Precompiles::<R>::get(a) == b"Sha3FIPS256".to_vec() => {
				Some(Sha3FIPS256::execute(handle))
			}
			a if *Precompiles::<R>::get(a) == b"ECRecoverPublicKey".to_vec() => {
				Some(ECRecoverPublicKey::execute(handle))
			}
			_ => None,
		}
	}

	fn is_precompile(&self, address: H160) -> bool {
		match address {
			a if *Precompiles::<R>::get(a) == b"ECRecover".to_vec() => true,
			a if *Precompiles::<R>::get(a) == b"Sha256".to_vec() => true,
			a if *Precompiles::<R>::get(a) == b"Ripemd160".to_vec() => true,
			a if *Precompiles::<R>::get(a) == b"Identity".to_vec() => true,
			a if *Precompiles::<R>::get(a) == b"Modexp".to_vec() => true,
			a if *Precompiles::<R>::get(a) == b"Sha3FIPS256".to_vec() => true,
			a if *Precompiles::<R>::get(a) == b"ECRecoverPublicKey".to_vec() => true,
			_ => false,
		}
	}
}
