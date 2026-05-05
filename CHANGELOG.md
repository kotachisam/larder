# Changelog

All notable changes to this project will be documented in this file. Format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); dates are
ISO-8601 (YYYY-MM-DD).

## [Unreleased]

### 2026-05-05

#### Added

- `store::open` — SQLite WAL + foreign keys + schema bootstrap.
- `store::queries` — `upsert_session`, `insert_entries` (transactional, idempotent
  via `INSERT OR IGNORE`), `session_fingerprint`, `last_source_line`,
  `session_count`, `entry_count`, `entry_count_by_kind`.
- `transcript::paths::walk` — discover `*.jsonl` sessions under
  `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl` (depth-bounded for
  speed).
- `transcript::paths::decode_project_path` — fallback decoder for the
  encoded-cwd directory name. Lossy for paths containing underscores; ingest
  prefers the canonical `cwd` field from the first event when present.
- `transcript::event::MessageContent` — untagged enum (Text | Blocks | None) so
  user prompts (string content) and tool results (block array content)
  deserialize from the same `MessageEnvelope`.
- `extract::Extractor` — pairs user prompts → assistant text → Bash tool_use →
  tool_result into `Entry` rows; flushes interrupted Bash on session end;
  emits QA entries when the next user prompt arrives.
- `ingest::run` — orchestrates walk → fingerprint check → parse → upsert,
  prints a summary line, supports `--path` and `--dry-run`.
- `search::search` + `search::run` — FTS5 BM25 query over question /
  answer_summary / command / command_stdout. Quoted-token query builder
  prevents FTS syntax injection from punctuation in user input.
- `format::render_hits` + `format::render_command_only` — text (color-aware,
  TTY-detected), JSON, and Markdown rendering. `--cmd-only` exits non-zero on
  no match so `$(larder ask --cmd-only "...")` doesn't substitute garbage.
- `digest::Aggregator` trait + `SqlAggregator` impl — frequency aggregation
  with normalized question key (lowercase + trim). Trait seam left in place to
  swap in a Polars-backed aggregator later without changing the CLI surface.
- `lib::stats` and `lib::path` — implemented (`reindex` still stubbed).

#### Changed

- `sessions` schema gained `provider TEXT NOT NULL DEFAULT 'claude'` plus
  `idx_sessions_provider`. Schema version stays at 1 (pre-release, no
  migration needed). Multi-provider drop-in: add a module under
  `transcript::providers::` and register in `paths::walk`.
- `TranscriptPath` gained a `provider: &'static str` field.

#### Verified

- 74 sessions / 7,614 entries (4,396 Bash · 3,218 QA) ingested from a real
  `~/.claude/projects` tree in ~35s wall.
- Re-ingest is idempotent (73 unchanged, 1 updated for the active session).
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo test` — 1 passing (`decode_project_path`).

### 2026-05-04

Initial commit (`4546edd`) — structural foundation. No runnable behaviour
yet; every function body was a `todo!()` placeholder, but the spine was
deliberate enough that the 2026-05-05 implementation pass was almost entirely
filling in pre-shaped slots.

#### Added — Project scaffolding

- Dual licensing: `LICENSE-MIT` + `LICENSE-APACHE` (Rust ecosystem norm).
- `README.md` with status table (Claude Code first; Codex / Gemini CLI
  planned), install plan (`cargo install` / homebrew tap), privacy stance
  ("nothing sent over the network. Ever."), and the why-not-just-keep-asking
  framing.
- `.gitignore` covering Rust target, IDE detritus, macOS junk, env files,
  and SQLite sidecar files (`*.sqlite-{journal,shm,wal}`).
- `rust-toolchain.toml` pinning stable + rustfmt + clippy.
- `.github/workflows/ci.yml` — 3-OS matrix (ubuntu / macos / windows) running
  fmt-check, clippy with `-D warnings`, test, and release build.

#### Added — Cargo manifest and dependencies

- `Cargo.toml` with edition 2024, crate metadata (description, repo,
  keywords, categories), and a release profile tuned for binary size:
  `lto = true`, `codegen-units = 1`, `strip = true`.
- Dependency set chosen up front so later work didn't relitigate it:
  `anyhow` + `thiserror` (errors), `chrono` w/ serde (timestamps), `clap` v4
  derive (CLI), `directories` (XDG paths), `humantime` (`--since 7d`),
  `notify` + `notify-debouncer-mini` (watch loop), `owo-colors` (TTY
  rendering), `rusqlite` bundled w/ FTS5 (store), `serde` + `serde_json`
  (transcript parsing), `tracing` + `tracing-subscriber` w/ env-filter
  (logging), `walkdir` (transcript discovery).

#### Added — Module layout

- `src/lib.rs` + `src/main.rs` split (binary thin shim; library does the
  work, so integration tests and future MCP / library consumers can link
  against it).
- Submodule structure: `store/{mod,queries,schema}`,
  `transcript/{event,paths,mod}`, plus flat modules `cli`, `config`,
  `digest`, `extract`, `format`, `ingest`, `mcp`, `search`, `watch`. Bacterial
  cuts: each module owns one job.
- `main.rs` initialises `tracing-subscriber` with env-filter so
  `RUST_LOG=larder=debug` works out of the box.

#### Added — CLI surface

- All 8 subcommands defined in `cli.rs` with their final flag shape, so the
  contract was frozen before any implementation: `ingest` (`--since`,
  `--path`, `--dry-run`), `watch` (`--path`), `ask` (`--limit`, `--format`,
  `--cmd-only`, `--raw`, `--no-color`), `digest` (`--since`, `--top`,
  `--format`), `stats`, `path`, `reindex`, `serve` (`--stdio`).
- `OutputFormat` enum (`Text` / `Json` / `Md`) shared across `ask` and
  `digest`.

#### Added — Storage spine

- `Paths::resolve` using the `directories` crate to locate XDG data dir +
  `~/.claude/projects` transcript root.
- `store::Store` wrapper around `Arc<Mutex<Connection>>` (sync API; CLI
  doesn't need async).
- `store::Entry` and `EntryKind` (`Bash` / `Qa`) data model.
- Schema v1 (`store/schema.rs`) covering the full lifecycle:
  - `sessions` table keyed by `session_id`, with mtime/size for fingerprint
    skip-if-unchanged, plus started/ended/message_count for digest windows.
  - `entries` table with `kind`, `question`, `answer_summary`, `command`,
    `command_stdout`, `command_stderr`, `exit_code`, `interrupted`,
    `truncated`, `tool_use_id`, `parent_uuid`, `source_line`. Foreign key
    cascade-on-delete from sessions.
  - `UNIQUE (session_id, tool_use_id)` and `UNIQUE (session_id, source_line)`
    — the constraints that make `INSERT OR IGNORE` re-ingest idempotent.
  - Supporting indexes on `sessions(project_path)`, `sessions(ended_at DESC)`,
    `entries(session_id)`, `entries(ts DESC)`, `entries(kind)`.
  - `entries_fts` virtual table (FTS5, `tokenize='porter unicode61'`,
    `content='entries'`) plus AI/AD/AU triggers to keep it synced
    automatically — so search code never has to think about FTS upkeep.
  - `schema_version` table seeded to `1` for the future migration path.

#### Added — Transcript model

- `transcript::Event` enum (`User` / `Assistant` / `QueueOperation` /
  `Other`) with `#[serde(tag = "type", rename_all = "kebab-case")]` so JSONL
  variants dispatch automatically.
- `MessageEnvelope` + `ContentBlock` (`Text` / `Thinking` / `ToolUse` /
  `ToolResult` / `Other`) — the shape of Claude Code messages, including
  `tool_use_id` linkage between `ToolUse` and `ToolResult`.
- `ToolUseResult` for the user-event-level `toolUseResult` field carrying
  `stdout` / `stderr` / `interrupted` / `isImage`.
- `Extractor` shape — fields and lifecycle methods (`new`, `step`, `flush`)
  defined so the 2026-05-05 implementation just had to fill them in.

#### Added — Test infrastructure

- `tests/fixtures/` with a README enumerating the five hand-picked extractor
  edge cases to land alongside the extractor implementation
  (`bash_normal`, `bash_interrupted`, `qa_only`, `tool_result_array`,
  `queue_operation`).
