# Extractor fixtures

Hand-picked transcript snippets covering load-bearing edge cases. Each file is a small JSONL slice; the matching extractor test asserts the expected `Vec<Entry>` output.

Planned fixtures (TBD as extraction lands):

- `bash_normal.jsonl` — complete user → assistant text + Bash tool_use → tool_result cycle
- `bash_interrupted.jsonl` — Bash tool_use with no matching tool_result
- `qa_only.jsonl` — assistant message with only text blocks, no tool_use
- `tool_result_array.jsonl` — `tool_result.content` as an array variant rather than a string
- `queue_operation.jsonl` — user input arriving via a `queue-operation` event

Fixtures are derived from real transcripts and scrubbed of identifying content. Any text block that isn't load-bearing for the schema test gets replaced with a `lorem ipsum`-style placeholder.
