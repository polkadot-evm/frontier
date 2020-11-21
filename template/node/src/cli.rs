use structopt::{StructOpt, clap::arg_enum};

arg_enum! {
	/// Available Sealing methods.
	#[allow(missing_docs)]
	#[derive(Debug, Copy, Clone, StructOpt)]
	pub enum Sealing {
		// Seal using rpc method.
		Manual,
		// Seal when transaction is executed.
		Instant,
	}
}

#[allow(missing_docs)]
#[derive(Debug, StructOpt)]
pub struct RunCmd {
	#[allow(missing_docs)]
	#[structopt(flatten)]
	pub base: sc_cli::RunCmd,

	/// Choose sealing method.
	#[structopt(long = "sealing")]
	pub sealing: Option<Sealing>,

	#[structopt(long = "enable-dev-signer")]
	pub enable_dev_signer: bool,
}

#[derive(Debug, StructOpt)]
pub struct Cli {
	#[structopt(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[structopt(flatten)]
	pub run: RunCmd,
}

#[derive(Debug, StructOpt)]
pub enum Subcommand {
	/// Build a chain specification.
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),
}
