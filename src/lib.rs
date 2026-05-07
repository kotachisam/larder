pub mod cli;
pub mod config;
pub mod digest;
pub mod extract;
pub mod find;
pub mod format_qa;
pub mod grep;
pub mod history;
pub mod ingest;
pub mod mcp;
pub mod open;
pub mod proxy;
pub mod results_cache;
pub mod search;
pub mod store;
pub mod transcript;
pub mod util;
pub mod watch;

use anyhow::Result;

use crate::cli::{Cli, Command};
use crate::config::Paths;
use crate::store::Store;

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Ingest(args) => ingest::run(args),
        Command::Watch(args) => watch::run(args),
        Command::Ask(args) => search::run(args),
        Command::Asked(args) => search::run_asked(args),
        Command::Find(args) => find::run(args),
        Command::Grep(args) => grep::run(args),
        Command::Open(args) => open::run(args),
        Command::Proxy(args) => proxy::run(args),
        Command::Digest(args) => digest::run(args),
        Command::Stats => stats(),
        Command::Path => path(),
        Command::Reindex => reindex(),
        Command::Serve(args) => mcp::run(args),
    }
}

fn stats() -> Result<()> {
    let paths = Paths::resolve()?;
    let store = Store::open(&paths.db_path)?;
    let sessions = store.session_count()?;
    let subagents = store.subagent_session_count()?;
    let entries = store.entry_count()?;
    let bash = store.entry_count_by_kind("bash")?;
    let qa = store.entry_count_by_kind("qa")?;
    let prompts = store.prompt_count()?;
    println!("db:       {}", paths.db_path.display());
    println!(
        "sessions: {} ({} top-level, {} subagent)",
        sessions,
        sessions - subagents,
        subagents
    );
    println!("entries:  {} ({} bash, {} qa)", entries, bash, qa);
    println!("prompts:  {} (history.jsonl)", prompts);
    Ok(())
}

fn path() -> Result<()> {
    let paths = Paths::resolve()?;
    println!("data_dir:        {}", paths.data_dir.display());
    println!("db_path:         {}", paths.db_path.display());
    println!("transcripts_dir: {}", paths.transcripts_dir.display());
    Ok(())
}

fn reindex() -> Result<()> {
    anyhow::bail!("reindex not yet implemented")
}
