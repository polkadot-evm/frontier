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
			a if &Precompiles::<R>::get(a)[..] == b"ECRecover" => Some(ECRecover::execute(handle)),
			a if &Precompiles::<R>::get(a)[..] == b"Sha256" => Some(Sha256::execute(handle)),
			a if &Precompiles::<R>::get(a)[..] == b"Ripemd160" => Some(Ripemd160::execute(handle)),
			a if &Precompiles::<R>::get(a)[..] == b"Identity" => Some(Identity::execute(handle)),
			a if &Precompiles::<R>::get(a)[..] == b"Modexp" => Some(Modexp::execute(handle)),
			// Non-Frontier specific nor Ethereum precompiles :
			a if &Precompiles::<R>::get(a)[..] == b"Sha3FIPS256" => {
				Some(Sha3FIPS256::execute(handle))
			}
			a if &Precompiles::<R>::get(a)[..] == b"ECRecoverPublicKey" => {
				Some(ECRecoverPublicKey::execute(handle))
			}
			_ => None,
		}
	}

	fn is_precompile(&self, address: H160) -> bool {
		match address {
			a if &Precompiles::<R>::get(a)[..] == b"ECRecover" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Sha256" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Ripemd160" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Identity" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Modexp" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Sha3FIPS256" => true,
			a if &Precompiles::<R>::get(a)[..] == b"ECRecoverPublicKey" => true,
			_ => false,
		}
	}
}
