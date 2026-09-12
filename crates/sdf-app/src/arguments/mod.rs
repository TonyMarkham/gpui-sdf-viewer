pub(crate) mod command;
pub(crate) mod extract;
pub(crate) mod query_config;

// ---------------------------------------------------------------------------------------------- //

use crate::Command;

use clap::Parser;

// ---------------------------------------------------------------------------------------------- //

/// cd-map-offline: the Crimson Desert map fields, offline.
#[derive(Parser)]
pub struct Arguments {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}
