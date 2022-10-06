//! Substrate Node Template CLI library.

#![warn(missing_docs)]
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

mod chain_spec;
mod cli;
mod command;
mod command_helper;
mod rpc;
mod service;

fn main() -> sc_cli::Result<()> {
	command::run()
}
