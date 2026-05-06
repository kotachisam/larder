# Testing

How to verify `larder` works on your own machine. Three layers: unit tests,
fixture-based extractor tests (planned), and real-world smoke tests against
your actual transcript directory.

## 1. Unit tests

```bash
cargo test
```

Covers pure functions only — currently `decode_project_path`. Add new tests
beside the code they cover (`#[cfg(test)] mod tests { ... }` at the bottom of
the same file).

## 2. Fixture-based extractor tests (planned)

See `tests/fixtures/README.md` for the planned set:

- `bash_normal.jsonl` — full user → assistant text + Bash → tool_result cycle
- `bash_interrupted.jsonl` — Bash with no matching tool_result (flush path)
- `qa_only.jsonl` — assistant text-only turn, no tool_use
- `tool_result_array.jsonl` — `tool_result.content` as array variant
- `queue_operation.jsonl` — user input via `queue-operation` event

These are derived from real transcripts and scrubbed of identifying content.
Each fixture pairs with an integration test that asserts the expected
`Vec<Entry>` shape. Land fixtures + tests as the extractor edges shift.

## 3. Real-world smoke tests

Run against your actual `~/.claude/projects` tree. The first ingest will take
seconds-to-minutes depending on how many sessions you've got.

### 3.1 Build a release binary once

```bash
cargo build --release
alias larder=$PWD/target/release/larder
```

(Or `cargo run --release --quiet --` if you prefer not to alias.)

### 3.2 Show resolved paths

```bash
larder path
```

Expect `data_dir`, `db_path`, `transcripts_dir` lines.
On macOS the data dir is `~/Library/Application Support/larder/`.
The DB lives at `<data_dir>/larder.sqlite`.

### 3.3 First ingest

```bash
larder ingest
```

Expect a single summary line of the form:

```text
ingest: <N> sessions seen (<N> new, 0 updated, 0 unchanged), <M> entries inserted
```

### 3.4 Idempotent re-ingest

Run `larder ingest` again. Should show `0 new, 0–1 updated, ~all unchanged`.
The `1 updated` case is normal if your active Claude Code session has written
to its transcript between runs. Idempotency is enforced by:

- session-level: `(source_mtime, source_size)` fingerprint match → skip parse.
- entry-level: `UNIQUE (session_id, tool_use_id)` and
  `UNIQUE (session_id, source_line)` → re-inserts collapse via
  `INSERT OR IGNORE`.

### 3.5 Stats sanity check

```bash
larder stats
```

Expected output shape:

```text
db:       /path/to/larder.sqlite
sessions: <total> (<top-level> top-level, <subagent> subagent)
entries:  <total> (<bash> bash, <qa> qa)
```

Bash + QA should sum to total entries. Top-level + subagent should sum to
total sessions. If subagent count is zero but you've used the Task tool or
parallel agents, the walker isn't reaching them — check your transcripts
directory has nested `<session>/subagents/agent-*.jsonl` files.

### 3.6 Recall queries

Pick a few topics you know you've discussed recently. Examples that worked
during initial verification:

```bash
larder ask "wrangler tail" --limit 3
larder ask "schema migration sqlite" --limit 3
larder ask "polylogue" --limit 2
```

Each hit shows: rank, timestamp, project_path, BM25 score (lower is better),
question, command, stdout snippet. Inspect by eye — do the hits match what you
actually asked? If a known query returns nothing, the entry probably wasn't
captured by the extractor (file a fixture).

### 3.7 Output formats

```bash
larder ask "wrangler tail" --format json
larder ask "wrangler tail" --format md
larder ask "wrangler tail" --raw            # preserve newlines in snippets
larder ask "wrangler tail" --no-color       # disable ANSI even on a TTY
```

### 3.8 Shell substitution path

```bash
$(larder ask --cmd-only "wrangler tail")
```

Should print the top hit's command and exit 0 if found, exit 1 (no stdout) if
not. The non-zero exit prevents accidental empty substitution.

**Self-recursion pitfall:** if your top hit happens to be a meta-command
that itself invokes `larder ask` (common when testing larder against its own
session transcripts), the substituted shell command becomes ambiguous —
pipes, redirects, and flags inside the result get re-parsed by your shell.
Symptom: clap errors like `error: unexpected argument '-2' found`. This is
harmless in real-world use (you're recalling commands from days ago, not
from the session you just ran). If it bites in testing, narrow your query
or use `larder ask "<query>" --limit 1` and copy the command manually.

### 3.9 Digest

```bash
larder digest --since 7d --top 10
larder digest --since 30d --top 20 --format md
```

`--since` accepts humantime durations: `90m`, `3h`, `7d`, `4w`. Aggregation
groups by `LOWER(TRIM(question))`, so prompts that differ only in whitespace
or case collapse together. Top-1 is often a canned prompt like "carry on" or
"proceed" — useful signal for identifying alias candidates.

### 3.10 Subagent verification

Pick a query likely to surface dispatched-agent work (audits, multi-file
explorations, parallel research):

```bash
larder ask "comprehensive audit code" --limit 5
```

Expect inline badges on subagent hits:

```text
[1] 2026-04-28 19:48 · /Users/sam_r/Developer/samuelk [subagent: "Audit admin serializer bug surface"] · score -9.73
```

Three things to confirm:

- Subagent hits render with `[subagent: "..."]` when `.meta.json` was
  present, plain `[subagent]` when absent. Around half of historical
  subagents lack `.meta.json` — graceful degradation is expected.
- Filter works: `larder ask "comprehensive audit code" --no-subagents`
  returns only top-level hits (often zero for subagent-typical queries).
- Search ordering is BM25, not subagent-vs-not — relevant subagent results
  can rank above less-relevant top-level results. Intended.

### 3.11 Schema migration verification

Check the current schema version:

```bash
sqlite3 "$(larder path | awk '/db_path:/ {print $2}')" \
  'SELECT MAX(version) FROM schema_version;'
```

Expect the value to match `SCHEMA_VERSION` in `src/store/schema.rs`
(currently `3`).

To verify migrations apply cleanly to a fresh DB:

```bash
rm -rf "$(larder path | awk '/data_dir:/ {print $2}')"
larder ingest                   # builds v3 from scratch
larder stats                    # should match shape from 3.5
```

To verify metadata backfill (re-ingest populates new columns on previously
unchanged sessions):

```bash
sqlite3 "$(larder path | awk '/db_path:/ {print $2}')" \
  'SELECT COUNT(*) AS total, COUNT(subagent_description) AS with_desc
   FROM sessions WHERE is_subagent = 1;'
```

`with_desc` should be roughly 50% of `total` for a typical Claude Code
history (older subagent dispatches lack `.meta.json`). If it's zero on a DB
that previously held subagents, the `refresh_session_metadata` path isn't
firing — bug.

### 3.12 Transcript grep

`larder grep` shells out to `rg` against the raw JSONL transcripts under
`~/.claude/projects`, then enriches each match by joining back to the
indexed `entries` table — you see the *conversation turn* the literal
match belongs to, not the raw JSON-encoded line.

```bash
larder grep "VITE_ALCHEMY"                          # regex (default)
larder grep -F "VITE_ALCHEMY"                       # literal string
larder grep "wrangler.*--env" --since 7d            # restrict by mtime
larder grep "TODO" --project /Users/sam_r/Developer/jam
larder grep "rate limit" --by-hits -l 5             # sort by match count
```

Output groups matches by `(session, question)` so a single conversation
turn that ran multiple Bash commands collapses into one block:

```text
[3] 2026-05-05 18:28 · /Users/sam_r/Developer/biz/jam · 7 matches across 5 commands
  Q: I think the alchemy key is failing because in prod I split alchemy
     into 2 keys in a previous session, 1 for frontend and 1 for server...
  > Searching transcripts for the previous discussion. The split was:
    VITE_ALCHEMY_API_KEY (frontend, domain-restricted) and ALCHEMY_API_KEY...
  $ rg -l "ALCHEMY.*split|split.*alchemy|VITE_ALCHEMY" ...    (1 match)
    ↳ /Users/sam_r/.claude/projects/...
  $ rg -A 2 "VITE_ALCHEMY|split.*alchemy" ...                 (2 matches)
    ↳ {"parentUuid":"0eef5bbd-...
  ... (3 more commands in this turn)
```

Three things to verify:

- The `Q:` line shows your prompt (deduplicated per turn).
- The `>` line shows the assistant's full response, pulled from the
  matching QA entry.
- Each `$` line is one Bash command from that turn, with its own match
  count. Total in the header sums them.

#### Flags

- `-l/--limit <N>` — max grouped turns to render (default 10).
- `--by-hits` — sort by total match count (descending) instead of recency.
  Recency is default.
- `--raw` — bypass enrichment, fall back to plain `rg --heading
  --line-number --max-columns=300 --max-columns-preview` over the same
  filtered file set. Use when you want raw JSONL line context.
- Anything after `--` passes through to `rg` in both modes:
  `larder grep "X" -- -A 2 -B 1` for context lines, `-- --json` for
  machine-readable output.

#### jq recipes for structured extraction

Raw rg output is JSON-encoded transcript events. Pipe through `rg --json`
and `jq` for clean structured access:

```bash
# Just file:line for every match (no JSON noise)
larder grep "VITE_ALCHEMY" --since 30d -- --json \
  | jq -r 'select(.type=="match") | "\(.data.path.text):\(.data.line_number)"'

# Match count grouped by transcript file
larder grep "VITE_ALCHEMY" --since 30d -- --json \
  | jq -r 'select(.type=="match") | .data.path.text' \
  | sort | uniq -c | sort -rn

# Pull human-readable content (timestamp · type · text/command) per match
larder grep -F "VITE_ALCHEMY" --since 7d -- --json --no-filename \
  | jq -r '
      select(.type=="match")
      | .data.lines.text
      | fromjson?
      | .timestamp + " " + .type + ": " +
        ( if .message.content | type == "string" then .message.content
          elif .message.content then
            [.message.content[]? | .text? // .input?.command? // empty]
            | join(" | ")
          else "" end )
    '
```

The third recipe goes through `rg --json` rather than parsing rg's text
output directly — cleaner, and `fromjson?` silently drops any line that
isn't a transcript event (e.g. binary chunks). Output: one
`<timestamp> <type>: <content>` line per hit.

For a richer "show me the conversation around this hit" experience, that's
a future `larder grep --pretty` mode (not built yet — file an issue if you
hit a case where you need it).

### 3.13 Long-tail prompt recall (history.jsonl)

`larder ingest` also reads `~/.claude/history.jsonl` (Claude Code's
append-only log of every prompt you've ever typed, persisted independently
of transcript retention) and indexes them as a separate `prompts` table.
This recovers the *what was I asking about* signal for conversations whose
transcripts have already been pruned.

```bash
larder ingest                                      # ingests transcripts AND history
larder stats                                       # shows prompt count
larder asked "tjournal sqlite" -l 5                # text-only prompt search
larder asked "kanban" --since 60d                  # restrict by prompt timestamp
larder asked "RESEND" --project /Users/sam_r/Developer/jam
```

Output renders with a `[history]` badge:

```text
[1] 2026-04-28 17:39 · /Users/sam_r/Developer/oss/tjournal [history] · score -14.68
  Q: cargo run -- --sqlite-file-path /tmp/tjournal-p-test.db --backend-type sqlite
     Well previously you've given me this (snagged from atuin) but I'd like a perma-path
```

Three things to verify:

- `larder stats` shows a non-zero `prompts:` line. Expect roughly 10k-15k
  for a heavy user with several months of history.
- `larder ingest` summary mentions both transcripts and history:
  `history: <N> lines seen, <M> prompts new, <K> duplicate, <L> noise filtered`.
- Prompt-only hits surface old conversations that `larder ask` can't find
  because their transcripts were pruned by Claude Code's `cleanupPeriodDays`
  before larder existed. This is the recovery layer.

#### What gets filtered

- Empty prompts.
- Slash commands (`/fast`, `/clear`, `/skill-name`, etc.).

Single-character commands (`q`, `y`, `n`) are kept — short isn't the same
as noise. Tune `is_meaningful` in `src/history.rs` if your history has
patterns worth filtering.

#### Why a separate command

`larder ask` searches the rich Q→A→Bash entries (post-extraction). Mixing
prompt-only hits into those results would dilute BM25 ranking with
incomplete data (no answer, no command). `larder asked` is the dedicated
prompt-only command — use it as a fallback when `larder ask` returns
nothing for an old topic.

## 4. Lint and format

Run before committing:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
```

CI parity: both must be clean. `clippy` is run with `-D warnings` so any new
warning is a hard failure.

## 5. Manual reset (if you need a clean slate)

```bash
rm -rf "$(larder path | awk '/data_dir:/ {print $2}')"
larder ingest
```

Wipes the database and re-ingests from scratch. Useful when iterating on the
extractor or schema.

## 6. Known limitations

- **Orphan subagents.** Subagent transcripts whose parent session was
  pruned by Claude Code's `cleanupPeriodDays` (30-day default) before
  larder ingested them have a `parent_session_id` that doesn't resolve to
  any row in `sessions`. They still surface in search; only the parent
  thread context is lost. Mitigation: set `"cleanupPeriodDays": 3650` in
  `~/.claude/settings.json` so this stops happening forward.
- **Subagent metadata coverage ~50%.** Claude Code only started writing
  `agent-*.meta.json` files at some point in its lifecycle. Older subagent
  dispatches render as plain `[subagent]` instead of
  `[subagent: "<description>"]`. Nothing to fix in larder — the data isn't
  there.
- **`decode_project_path` is lossy.** Underscores in original paths (e.g.
  `sam_r`) are encoded as hyphens by Claude Code, indistinguishable from
  real path separators. Mitigated by reading `cwd` from the first event of
  each transcript when present (the canonical path lives there); the
  decoder is only a fallback. `larder grep --project` queries against the
  canonical `sessions.project_path` directly so the lossiness no longer
  bites on filtering.
- **`--cmd-only` self-recursion.** See section 3.8. `larder ask --cmd-only`
  output containing nested `larder ask` calls breaks `$()` substitution.
  Won't fix in v0.1; document and move on.
- **Entry-level subagent linkage absent.** Session-level linkage
  (subagent.parent_session_id → parent_session.session_id) works. But
  Claude Code does not record which specific Task tool_use call dispatched
  which subagent — the `.meta.json` lacks a `tool_use_id` pointer. Could
  be inferred heuristically (description + timestamp match) but ambiguous
  when descriptions repeat.
