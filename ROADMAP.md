# Roadmap

What's stubbed and shipped vs. still `todo!()` after v0.1 lands.

## Build order & rationale

The primary goal beyond v0.1 is **useful to people other than me**. That goal
reorders the work — it's not just "what's left to build", it's "what
compounds toward someone else picking this up and using it".

Three phases, in order:

1. **Phase 1 — `larder serve --stdio` (MCP).** Highest-leverage feature
   left. Today larder is a CLI you have to learn; MCP turns it into a tool
   Claude itself uses. The value prop transforms from a feature ("transcript
   search CLI") into a story ("Claude can answer questions about your own
   past conversations"). Stories get discovered; features don't.
2. **Phase 2 — Distribution.** Once MCP works, the install pitch is one
   line: "add this to your Claude Desktop config". That's when packaging
   and a launch moment are worth the effort. Without MCP, distribution
   sells a niche CLI to a niche audience — same package, completely
   different conversion rate.
3. **Phase 3 — Multi-provider expansion.** Tempting to do first because
   each provider parser feels like progress, but it's lateral, not vertical.
   Multi-provider doesn't change the story, it broadens the data source.
   Better to do this *after* launch, where it can be pulled by user PRs
   ("currently Claude Code; PR welcome for Codex/Gemini/Cursor") rather
   than pushed pre-emptively.

Items below `## Lower priority` are real but parked — they don't block the
three phases above and shouldn't be picked up while the critical path has
unfinished work.

## Phase 1 — `larder serve --stdio` (MCP server)

**Status:** stub at `src/mcp.rs` (`todo!("MCP serve --stdio: JSON-RPC stdio
loop, one tool: search")`).

**What it does:** exposes larder as a [Model Context
Protocol](https://modelcontextprotocol.io/) server over stdio, so any
MCP-aware AI client (Claude Desktop, Claude Code, Continue.dev, etc.) can
query your conversation history as a tool. The use case: "Claude, what was
the wrangler tail issue I worked through with you in March?" — and Claude
hits larder via MCP to find out.

**Scope:** ~250 LoC. Implement the MCP JSON-RPC stdio loop. Three tools to
expose initially: `search` (wraps `larder ask`), `find` (wraps `larder
find`), `digest` (wraps `larder digest`). Use the `rmcp` crate or hand-roll
JSON-RPC over stdin/stdout — both viable.

**Dependencies to add:** `rmcp = "0.x"` (or just `serde_json` for
hand-rolled).

**Configuration after install:**

```json
// ~/Library/Application Support/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "larder": {
      "command": "/Users/sam_r/.cargo/bin/larder",
      "args": ["serve", "--stdio"]
    }
  }
}
```

## Phase 2 — Distribution

No code stubs — this phase is packaging, onboarding, and reach.

**Goal:** make the install instruction one line that someone outside the
existing audience would actually run.

**Tasks, smallest-leverage-cost first:**

- **Publish to crates.io.** `cargo install larder` becomes the canonical
  install. Removes the `--path .` friction. Tiny effort, real credibility
  signal.
- **Homebrew formula** (`brew install larder` or via tap). Largest single
  install-friction reduction for the macOS audience that overlaps most
  with Claude Code users.
- **`curl | sh` installer script.** For Linux users without cargo who
  just want the binary. Lowest-effort cross-platform parity.
- **Onboarding flow.** First-run hint if no DB exists ("run `larder
  ingest` to backfill"). Surface the launchd job (`scripts/`) as an
  install step in the README. Make MCP setup a one-line `larder
  serve --install` helper if friction warrants.
- **Story-driven launch.** Screenshot of Claude itself querying past
  conversations via MCP. Post to /r/ClaudeAI, HN Show HN, X. The launch
  pitch is the MCP story, not the CLI feature list.

**Order matters within this phase:** crates.io first (anyone can install
post-launch), Homebrew second (smoother for most likely users), launch
third (only after the first two are real).

## Phase 3 — Multi-provider expansion

These are mentioned in the README as planned but don't have stubs yet, and
shouldn't have stubs until Phase 1 + 2 are done.

### Codex CLI provider

**Status:** unmodelled.

**Scope:** depends on what Codex CLI's transcript format looks like — would
need to be researched. Polylogue (<https://github.com/Sinity/polylogue>)
has a Codex parser worth referencing.

### Gemini CLI provider

**Status:** scoped during v0.1 design but **deferred** because the local
data is stale (last meaningful Gemini CLI usage in August 2025) and the
logs only contain user prompts (no assistant responses), making the
provider mostly equivalent to history.jsonl ingestion. Worth revisiting if
Gemini CLI usage picks back up.

**Scope if revived:** ~80 LoC. Add `source` column on `prompts` (schema v6
already added this in advance), walk `~/.gemini/tmp/<hash>/logs.json`,
insert each user message as a prompt with `source='gemini'`.

### Cursor / Aider / others

**Status:** unmodelled. Best as user PRs post-launch, with the MCP story
already shipping. Pull, don't push.

## Lower priority — parked behind the three phases

These are real but shouldn't displace work on the critical path.

### `larder watch` — live tail of new transcripts

**Status:** stub at `src/watch.rs` (`todo!("notify-debouncer-mini loop with
300ms debounce")`).

**What it does:** complementary to the daily launchd ingest. Watches
`~/.claude/projects/` and `~/.claude/history.jsonl` with filesystem-event
notifications and re-ingests changed sessions within seconds rather than
waiting for 03:00.

**Why parked:** the launchd job covers 99% of the use case. Watch is the
"never lose more than a few minutes" upgrade — nice but not urgent and
doesn't move the useful-to-others needle.

**Scope:** ~150 LoC. Re-add `notify` + `notify-debouncer-mini` as deps
(removed in cleanup since unused). Reuse existing `ingest::ingest_one` per
debounced file event. Skip subagent dirs to avoid event storms during
dispatched-agent activity.

**Dependencies to re-add:**

```toml
notify = "8"
notify-debouncer-mini = "0.7"
```

### `larder reindex` — rebuild FTS index from scratch

**Status:** stub at `src/lib.rs::reindex()` (`bail!("reindex not yet
implemented")`).

**What it does:** drops `entries_fts` and `prompts_fts`, then rebuilds them
from source tables. Recovery tool for FTS5 tokenizer changes or migration
breakage.

**Why parked:** the auto-rebuild triggers on `entries` / `prompts` keep the
FTS tables in sync during normal operation. Reindex is a recovery tool for
situations that haven't happened yet.

**Scope:** ~30 LoC. Two SQL operations:

```sql
INSERT INTO entries_fts(entries_fts) VALUES('rebuild');
INSERT INTO prompts_fts(prompts_fts) VALUES('rebuild');
```

Plus a progress indicator if the corpus grows.

## Iterative — CC TUI marker list

The CC paste collapser in `src/util.rs` (`CC_LINE_START_GLYPHS` and
`CC_SEPARATOR_CHARS`) is structured as data-driven const arrays so new
TUI markers can be added with a one-line diff plus a unit test. Expect to
extend this list opportunistically as new pastes surface in real-world
use — not a phase on its own, just ongoing maintenance.

## Things that aren't on the roadmap

Bacterial principle — the following sound nice but would expand scope
without serving the core "recall on a flaky VPN" use case:

- Web UI / TUI explorer (use the CLI, or query the SQLite directly)
- Vector / semantic search (FTS5 + BM25 is sufficient at this corpus size)
- Cloud sync of the SQLite DB (out of scope; rclone or similar handles it)
- Encryption at rest (filesystem encryption handles this; no need to
  reinvent)
- Multi-user / RBAC (single-user tool by design)
