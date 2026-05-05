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

Expect non-zero session and entry counts. Bash + QA should sum to total
entries.

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

### 3.9 Digest

```bash
larder digest --since 7d --top 10
larder digest --since 30d --top 20 --format md
```

`--since` accepts humantime durations: `90m`, `3h`, `7d`, `4w`. Aggregation
groups by `LOWER(TRIM(question))`, so prompts that differ only in whitespace
or case collapse together. Top-1 is often a canned prompt like "carry on" or
"proceed" — useful signal for identifying alias candidates.

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
