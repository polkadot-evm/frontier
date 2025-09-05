//! Substrate Node Template CLI library.

#![warn(missing_docs)]
#![allow(
	clippy::type_complexity,
	clippy::too_many_arguments,
	clippy::large_enum_variant,
	clippy::result_large_err
)]
#![cfg_attr(feature = "runtime-benchmarks", warn(unused_crate_dependencies))]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod chain_spec;
mod cli;
mod client;
mod command;
mod eth;
mod rpc;
mod service;

fn main() -> sc_cli::Result<()> {
	command::run()
}
