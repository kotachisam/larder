pub const SCHEMA_VERSION: i32 = 5;

pub const SCHEMA_V1_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  session_id     TEXT PRIMARY KEY,
  provider       TEXT NOT NULL DEFAULT 'claude',
  project_path   TEXT NOT NULL,
  source_path    TEXT NOT NULL,
  source_mtime   INTEGER NOT NULL,
  source_size    INTEGER NOT NULL,
  started_at     INTEGER,
  ended_at       INTEGER,
  message_count  INTEGER NOT NULL DEFAULT 0,
  ingested_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_project  ON sessions(project_path);
CREATE INDEX IF NOT EXISTS idx_sessions_ended    ON sessions(ended_at DESC);
CREATE INDEX IF NOT EXISTS idx_sessions_provider ON sessions(provider);

CREATE TABLE IF NOT EXISTS entries (
  id              INTEGER PRIMARY KEY,
  session_id      TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
  ts              INTEGER NOT NULL,
  kind            TEXT NOT NULL,
  question        TEXT,
  answer_summary  TEXT,
  command         TEXT,
  command_stdout  TEXT,
  command_stderr  TEXT,
  exit_code       INTEGER,
  interrupted     INTEGER NOT NULL DEFAULT 0,
  truncated       INTEGER NOT NULL DEFAULT 0,
  tool_use_id     TEXT,
  parent_uuid     TEXT,
  source_line     INTEGER NOT NULL,
  UNIQUE (session_id, tool_use_id),
  UNIQUE (session_id, source_line)
);

CREATE INDEX IF NOT EXISTS idx_entries_session ON entries(session_id);
CREATE INDEX IF NOT EXISTS idx_entries_ts      ON entries(ts DESC);
CREATE INDEX IF NOT EXISTS idx_entries_kind    ON entries(kind);

CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
  question, answer_summary, command, command_stdout,
  content='entries', content_rowid='id',
  tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
  INSERT INTO entries_fts(rowid, question, answer_summary, command, command_stdout)
    VALUES (new.id, new.question, new.answer_summary, new.command, new.command_stdout);
END;

CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
  INSERT INTO entries_fts(entries_fts, rowid, question, answer_summary, command, command_stdout)
    VALUES('delete', old.id, old.question, old.answer_summary, old.command, old.command_stdout);
END;

CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE ON entries BEGIN
  INSERT INTO entries_fts(entries_fts, rowid, question, answer_summary, command, command_stdout)
    VALUES('delete', old.id, old.question, old.answer_summary, old.command, old.command_stdout);
  INSERT INTO entries_fts(rowid, question, answer_summary, command, command_stdout)
    VALUES (new.id, new.question, new.answer_summary, new.command, new.command_stdout);
END;

CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
INSERT OR IGNORE INTO schema_version VALUES (1);
"#;

pub const SCHEMA_V2_SQL: &str = r#"
ALTER TABLE sessions ADD COLUMN parent_session_id TEXT;
ALTER TABLE sessions ADD COLUMN is_subagent INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_sessions_parent      ON sessions(parent_session_id);
CREATE INDEX IF NOT EXISTS idx_sessions_is_subagent ON sessions(is_subagent);
UPDATE schema_version SET version = 2;
"#;

pub const SCHEMA_V3_SQL: &str = r#"
ALTER TABLE sessions ADD COLUMN subagent_description TEXT;
ALTER TABLE sessions ADD COLUMN subagent_type TEXT;
UPDATE schema_version SET version = 3;
"#;

pub const SCHEMA_V4_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS prompts (
  id            INTEGER PRIMARY KEY,
  ts            INTEGER NOT NULL,
  project_path  TEXT NOT NULL,
  prompt_text   TEXT NOT NULL,
  pasted_chars  INTEGER NOT NULL DEFAULT 0,
  source_hash   TEXT NOT NULL UNIQUE,
  ingested_at   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_prompts_ts      ON prompts(ts DESC);
CREATE INDEX IF NOT EXISTS idx_prompts_project ON prompts(project_path);

CREATE VIRTUAL TABLE IF NOT EXISTS prompts_fts USING fts5(
  prompt_text,
  content='prompts', content_rowid='id',
  tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS prompts_ai AFTER INSERT ON prompts BEGIN
  INSERT INTO prompts_fts(rowid, prompt_text) VALUES (new.id, new.prompt_text);
END;
CREATE TRIGGER IF NOT EXISTS prompts_ad AFTER DELETE ON prompts BEGIN
  INSERT INTO prompts_fts(prompts_fts, rowid, prompt_text) VALUES('delete', old.id, old.prompt_text);
END;
CREATE TRIGGER IF NOT EXISTS prompts_au AFTER UPDATE ON prompts BEGIN
  INSERT INTO prompts_fts(prompts_fts, rowid, prompt_text) VALUES('delete', old.id, old.prompt_text);
  INSERT INTO prompts_fts(rowid, prompt_text) VALUES (new.id, new.prompt_text);
END;

UPDATE schema_version SET version = 4;
"#;

pub const SCHEMA_V5_SQL: &str = r#"
ALTER TABLE entries ADD COLUMN thinking TEXT;
UPDATE schema_version SET version = 5;
"#;

pub const MIGRATIONS: &[(i32, &str)] = &[
    (2, SCHEMA_V2_SQL),
    (3, SCHEMA_V3_SQL),
    (4, SCHEMA_V4_SQL),
    (5, SCHEMA_V5_SQL),
];
