use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "larder",
    version,
    about = "Local cache of LLM CLI transcripts for offline retrieval and querying"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Ingest(IngestArgs),
    Watch(WatchArgs),
    Ask(AskArgs),
    Asked(AskedArgs),
    Find(FindArgs),
    Grep(GrepArgs),
    Open(OpenArgs),
    Proxy(ProxyArgs),
    Digest(DigestArgs),
    Stats,
    Path,
    Reindex,
    Serve(ServeArgs),
}

#[derive(Args, Debug)]
pub struct OpenArgs {
    #[arg(default_value_t = 1)]
    pub rank: usize,
    #[arg(long)]
    pub session: bool,
    #[arg(long)]
    pub raw: bool,
    #[arg(long)]
    pub no_color: bool,
}

#[derive(Args, Debug)]
pub struct IngestArgs {
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct WatchArgs {
    #[arg(long)]
    pub path: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct AskArgs {
    pub query: Vec<String>,
    #[arg(long, default_value_t = 5)]
    pub limit: usize,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
    #[arg(long)]
    pub cmd_only: bool,
    #[arg(long, conflicts_with = "raw")]
    pub full: bool,
    #[arg(long)]
    pub raw: bool,
    #[arg(long)]
    pub no_color: bool,
    #[arg(long)]
    pub no_subagents: bool,
}

#[derive(Args, Debug)]
pub struct AskedArgs {
    pub query: Vec<String>,
    #[arg(short = 'l', long, default_value_t = 10)]
    pub limit: usize,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
    #[arg(long, conflicts_with = "raw")]
    pub full: bool,
    #[arg(long)]
    pub raw: bool,
    #[arg(long)]
    pub no_color: bool,
}

#[derive(Args, Debug)]
pub struct FindArgs {
    pub query: Vec<String>,
    #[arg(short = 'l', long, default_value_t = 5)]
    pub limit: usize,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub no_color: bool,
    #[arg(long)]
    pub no_files: bool,
    #[arg(long)]
    pub no_grep: bool,
    #[arg(long)]
    pub no_prompts: bool,
    #[arg(long)]
    pub no_subagents: bool,
}

#[derive(Args, Debug)]
pub struct GrepArgs {
    pub pattern: String,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub path: Option<PathBuf>,
    #[arg(short = 'F', long)]
    pub literal: bool,
    #[arg(long)]
    pub no_color: bool,
    #[arg(long)]
    pub raw: bool,
    #[arg(short = 'l', long, default_value_t = 10)]
    pub limit: usize,
    #[arg(long)]
    pub by_hits: bool,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rg_args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct DigestArgs {
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long, default_value_t = 10)]
    pub top: usize,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    #[arg(long)]
    pub stdio: bool,
}

#[derive(Args, Debug)]
pub struct ProxyArgs {
    #[arg(long, default_value = "http://localhost:11434")]
    pub to: String,
    #[arg(long, default_value_t = 11435)]
    pub port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    pub bind: String,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum OutputFormat {
    Text,
    Json,
    Md,
}
