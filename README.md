# larder

Local archive of your AI conversation history, with fast cross-source search.

`larder` reads transcripts from your terminal-AI tools (Claude Code today;
Codex / Gemini CLI / others on the roadmap), captures live conversations from
any Ollama-compatible client via an HTTP proxy, indexes everything in a local
SQLite database, and serves it back through unified search across
conversations, raw transcripts, typed-prompt history, and your filesystem.
Useful when you want to recall what your AI told you last week without
re-asking, especially on a flaky VPN or a plane.

## Status

v0.1 in active development. Not yet published.

| Source                       | How it's captured                          | Status     |
|------------------------------|--------------------------------------------|------------|
| Claude Code transcripts      | parse `~/.claude/projects/**/*.jsonl`      | v0.1       |
| Claude Code subagents        | parse `<session>/subagents/agent-*.jsonl`  | v0.1       |
| Claude Code typed prompts    | parse `~/.claude/history.jsonl`            | v0.1       |
| Open WebUI / Ollama chats    | live capture via `larder proxy` middleware | v0.1       |
| Codex                        | TBD                                        | planned    |
| Gemini CLI                   | parse `~/.gemini/tmp/*/logs.json`          | planned    |

## Install

Until published to crates.io / Homebrew:

```bash
git clone https://github.com/kotachisam/larder
cd larder
cargo install --path .
```

For automated daily ingest on macOS:

```bash
./scripts/install-launchd.sh   # daily 03:00, logs to ~/Library/Logs/larder/
```

## Use

```bash
# Capture
larder ingest                       # backfill all transcripts + history.jsonl
larder proxy                        # HTTP middleware for live Ollama capture
                                    # point Open WebUI at http://localhost:11435

# Recall
larder find "wrangler tail"         # unified search across all four layers
larder ask "restart nginx"          # ranked recall over indexed Q→A entries
larder asked "kanban" --since 90d   # FTS over typed-prompt history
larder grep -F "VITE_ALCHEMY"       # ripgrep over raw transcripts
larder digest --since 7d            # frequency report of recent questions
larder stats                        # session, entry, prompt counts
```

The `--cmd-only` flag on `ask` emits just the top result's command for
`$(larder ask --cmd-only "your query")` shell substitution — the fastest path
from "I asked Claude this last Tuesday" to a runnable shell command on a
disconnected machine.

`larder find` is the recommended human-facing entry point — it runs all four
search layers in one shot and renders sectioned output: indexed conversations,
literal transcript matches grouped per project, prompt history, and a bounded
filesystem scan.

## Privacy

`larder` reads your transcripts on disk and stores derived data (questions,
commands, command output, the first ~1000 chars of assistant prose) in a local
SQLite database under your XDG data directory. Nothing is sent over the
network. Ever. The optional `larder proxy` runs as local HTTP middleware
between your Ollama client and Ollama itself — bytes flow through your machine
only. Treat the larder database with the same care you'd treat your shell
history file: transcripts contain user-typed prose, secrets included.

## Why not just keep asking?

Asking is cheap. Remembering is expensive. The asking-loop works fine until
you're on call at 3am on hotel wifi and need the command Claude gave you three
days ago. larder is the offline cache for that moment, and the digest layer
surfaces patterns you can convert into your own aliases over time.

It's also the cross-source archive layer for anyone using more than one AI
tool: Claude Code in one terminal, Open WebUI in another, Gemini CLI from
last quarter, all retrievable from one search index that survives any
individual tool's retention policy.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your
option.
