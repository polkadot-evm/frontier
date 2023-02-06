#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use {
	alloc::{format, vec::Vec},
	czk_precompiles::{AnemioJave381Input4, AnonymousTransactionVerifier},
	fp_evm::{ExitError, ExitSucceed, LinearCostPrecompile, PrecompileFailure},
};

pub struct AnemoiJive;

impl LinearCostPrecompile for AnemoiJive {
	const BASE: u64 = 3000;
	const WORD: u64 = 0;
	fn execute(input: &[u8], _: u64) -> Result<(ExitSucceed, Vec<u8>), PrecompileFailure> {
		let jave = AnemioJave381Input4::new().map_err(|e| PrecompileFailure::Error {
			exit_status: ExitError::Other(format!("{:?}", e).into()),
		})?;
		jave.call(input)
			.map(|output| (ExitSucceed::Stopped, output))
			.map_err(|e| PrecompileFailure::Error {
				exit_status: ExitError::Other(
					alloc::format!("AnemioJave381Input4 call error:{:?}", e).into(),
				),
			})
	}
}

pub struct AnonymousVerifier;

impl LinearCostPrecompile for AnonymousVerifier {
	const BASE: u64 = 3000;
	const WORD: u64 = 0;
	fn execute(input: &[u8], _: u64) -> Result<(ExitSucceed, Vec<u8>), PrecompileFailure> {
		let jave = AnonymousTransactionVerifier::new().map_err(|e| PrecompileFailure::Error {
			exit_status: ExitError::Other(format!("{:?}", e).into()),
		})?;
		jave.call(input)
			.map(|output| (ExitSucceed::Stopped, output))
			.map_err(|e| PrecompileFailure::Error {
				exit_status: ExitError::Other(
					format!("AnonymousTransactionVerifier call error:{:?}", e).into(),
				),
			})
	}
}
