# Roadmap

What's stubbed and shipped vs. still `todo!()` after v0.1 lands. Listed in
rough build order, not strict commitment.

## 1. `larder watch` — live tail of new transcripts

**Status:** stub at `src/watch.rs` (`todo!("notify-debouncer-mini loop with
300ms debounce")`).

**What it does:** complementary to the daily launchd ingest. Watches
`~/.claude/projects/` and `~/.claude/history.jsonl` with filesystem-event
notifications and re-ingests changed sessions within seconds rather than
waiting for 03:00. Useful if you want recall on conversations you finished
five minutes ago.

**Why deferred:** the launchd job covers 99% of the use case. Watch is the
"never lose more than a few minutes" upgrade — nice but not urgent.

**Scope:** ~150 LoC. Re-add `notify` + `notify-debouncer-mini` as deps (we
removed them in cleanup since they were unused). Reuse existing
`ingest::ingest_one` per debounced file event. Skip subagent dirs to avoid
event storms during dispatched-agent activity.

**Dependencies to re-add:**

```toml
notify = "8"
notify-debouncer-mini = "0.7"
```

## 2. `larder serve --stdio` — MCP server

**Status:** stub at `src/mcp.rs` (`todo!("MCP serve --stdio: JSON-RPC stdio
loop, one tool: search")`).

**What it does:** exposes larder as a [Model Context
Protocol](https://modelcontextprotocol.io/) server over stdio, so any
MCP-aware AI client (Claude Desktop, Claude Code, Continue.dev, etc.) can
query your conversation history as a tool. The use case: "Claude, what was
the wrangler tail issue I worked through with you in March?" — and Claude
hits larder via MCP to find out.

**Why deferred:** v0.1 is about the human-facing CLI. MCP is the AI-facing
surface. Both useful, both legitimate, but the CLI is the load-bearing
surface for daily use right now.

**Scope:** ~250 LoC. Implement the MCP JSON-RPC stdio loop. Three tools
to expose initially: `search` (wraps `larder ask`), `find` (wraps
`larder find`), `digest` (wraps `larder digest`). Use the `rmcp` crate or
hand-roll JSON-RPC over stdin/stdout — both viable.

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

## 3. `larder reindex` — rebuild FTS index from scratch

**Status:** stub at `src/lib.rs::reindex()` (`bail!("reindex not yet
implemented")`).

**What it does:** drops `entries_fts` and `prompts_fts`, then rebuilds them
from the source tables. Needed when the FTS5 tokenizer changes, when a
schema migration broke the indexes, or when search results look stale and
you want to confirm.

**Why deferred:** the auto-rebuild triggers on `entries` / `prompts` keep
the FTS tables in sync during normal operation. Reindex is a recovery tool
for situations that haven't happened yet.

**Scope:** ~30 LoC. Two SQL operations:

```sql
INSERT INTO entries_fts(entries_fts) VALUES('rebuild');
INSERT INTO prompts_fts(prompts_fts) VALUES('rebuild');
```

Plus a progress indicator if the user has a lot of entries (currently ~10k,
not a problem yet).

## Also planned — provider expansion

These are listed in the README as planned but don't have stubs yet:

### Codex CLI provider

**Status:** unmodelled.

**Scope:** depends on what Codex CLI's transcript format looks like — would
need to be researched. Polylogue (https://github.com/Sinity/polylogue) has
a Codex parser worth referencing.

### Gemini CLI provider

**Status:** scoped during v0.1 design but **deferred** because the local
data is stale (last meaningful Gemini CLI usage in August 2025) and the
logs only contain user prompts (no assistant responses), making the
provider mostly equivalent to history.jsonl ingestion. Worth revisiting
if Gemini CLI usage picks back up.

**Scope if revived:** ~80 LoC. Add `source` column on `prompts` (schema v6
already added this in advance), walk `~/.gemini/tmp/<hash>/logs.json`,
insert each user message as a prompt with `source='gemini'`.

## Things that aren't on the roadmap

Bacterial principle — the following sound nice but would expand scope
without serving the core "recall on a flaky VPN" use case:

- Web UI / TUI explorer (use the CLI, or query the SQLite directly)
- Vector / semantic search (FTS5 + BM25 is sufficient at this corpus size)
- Cloud sync of the SQLite DB (out of scope; rclone or similar handles it)
- Encryption at rest (filesystem encryption handles this; no need to
  reinvent)
- Multi-user / RBAC (single-user tool by design)
