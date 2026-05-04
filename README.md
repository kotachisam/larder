# larder

Local cache of LLM CLI transcripts for offline retrieval and querying.

`larder` reads your terminal-AI session logs, extracts question → answer → tool-use triples, indexes them in a local SQLite database, and serves them back via fuzzy keyword search. Useful when you want to recall what your AI told you last week without re-asking, especially on a flaky VPN or a plane.

Designed to grow across LLM CLIs (Claude Code, Codex, Gemini CLI, others). v0.1 ships with Claude Code support first.

## Status

v0.1 in active development. Not yet published.

| Provider     | Status         | Source format             |
|--------------|----------------|---------------------------|
| Claude Code  | v0.1 (current) | `~/.claude/projects/*.jsonl` |
| Codex        | planned        | TBD                       |
| Gemini CLI   | planned        | TBD                       |

## Install (eventually)

```bash
cargo install larder
# or
brew install kotachisam/larder/larder
```

## Use

```bash
larder ingest              # backfill from your AI CLI's transcript directory
larder watch               # tail new sessions live
larder ask "restart nginx" # search the cache
larder digest --since 7d   # frequency report of recent questions
larder serve --stdio       # MCP server (one tool: search)
```

The `--cmd-only` flag on `ask` emits just the top result's command, designed for `$(larder ask --cmd-only "your query")` shell substitution. The fastest path from "I asked Claude this last Tuesday" to a runnable shell command on a disconnected machine.

## Privacy

`larder` reads your transcripts on disk and stores derived data (questions, commands, command output, the first ~1000 chars of assistant prose) in a local SQLite database under your XDG data directory. Nothing is sent over the network. Ever. Note that transcripts contain user-typed prose; treat the larder database with the same care you'd treat your shell history file.

## Why not just keep asking?

Asking is cheap. Remembering is expensive. The asking-loop works fine until you're on call at 3am on hotel wifi and need the command Claude gave you three days ago. larder is the offline cache for that moment, and the digest layer surfaces patterns you can convert into your own aliases over time.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
