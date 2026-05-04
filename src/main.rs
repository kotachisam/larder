use anyhow::Result;
use clap::Parser;
use larder::cli::Cli;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    larder::run(Cli::parse())
}
