pub mod cli;
pub mod config;
pub mod digest;
pub mod extract;
pub mod format;
pub mod ingest;
pub mod mcp;
pub mod search;
pub mod store;
pub mod transcript;
pub mod watch;

use anyhow::Result;

use crate::cli::{Cli, Command};

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Ingest(args) => ingest::run(args),
        Command::Watch(args) => watch::run(args),
        Command::Ask(args) => search::run(args),
        Command::Digest(args) => digest::run(args),
        Command::Stats => stats(),
        Command::Path => path(),
        Command::Reindex => reindex(),
        Command::Serve(args) => mcp::run(args),
    }
}

fn stats() -> Result<()> {
    todo!("stats")
}

fn path() -> Result<()> {
    todo!("path")
}

fn reindex() -> Result<()> {
    todo!("reindex")
}
