//! Substrate Node Template CLI library.
#![warn(missing_docs)]
#![allow(clippy::type_complexity)]

mod chain_spec;
#[macro_use]
mod service;
mod cli;
mod command;
mod command_helper;
mod rpc;

fn main() -> sc_cli::Result<()> {
	command::run()
}
