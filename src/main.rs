use anyhow::Result;
use clap::Parser;
use larder::cli::Cli;

fn main() -> Result<()> {
    reset_sigpipe();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    larder::run(Cli::parse())
}

#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}
