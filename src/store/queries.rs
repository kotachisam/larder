use anyhow::Result;
use rusqlite::{OptionalExtension, params};

use crate::store::{Entry, SessionMeta, Store};

impl Store {
    pub fn upsert_session(&self, meta: &SessionMeta, ingested_at: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO sessions (
                session_id, provider, project_path, source_path,
                source_mtime, source_size, started_at, ended_at,
                message_count, ingested_at, parent_session_id, is_subagent,
                subagent_description, subagent_type
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(session_id) DO UPDATE SET
                provider             = excluded.provider,
                project_path         = excluded.project_path,
                source_path          = excluded.source_path,
                source_mtime         = excluded.source_mtime,
                source_size          = excluded.source_size,
                started_at           = COALESCE(sessions.started_at, excluded.started_at),
                ended_at             = excluded.ended_at,
                message_count        = excluded.message_count,
                ingested_at          = excluded.ingested_at,
                parent_session_id    = excluded.parent_session_id,
                is_subagent          = excluded.is_subagent,
                subagent_description = excluded.subagent_description,
                subagent_type        = excluded.subagent_type
            "#,
            params![
                meta.session_id,
                meta.provider,
                meta.project_path,
                meta.source_path,
                meta.source_mtime,
                meta.source_size,
                meta.started_at,
                meta.ended_at,
                meta.message_count,
                ingested_at,
                meta.parent_session_id,
                meta.is_subagent as i64,
                meta.subagent_description,
                meta.subagent_type,
            ],
        )?;
        Ok(())
    }

    pub fn session_fingerprint(&self, session_id: &str) -> Result<Option<(i64, i64)>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT source_mtime, source_size FROM sessions WHERE session_id = ?1",
                params![session_id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    pub fn insert_entries(&self, _session_id: &str, entries: &[Entry]) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut inserted = 0usize;
        {
            let mut stmt = tx.prepare(
                r#"
                INSERT OR IGNORE INTO entries (
                    session_id, ts, kind, question, answer_summary,
                    command, command_stdout, command_stderr,
                    interrupted, truncated, tool_use_id, parent_uuid, source_line
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
            )?;
            for e in entries {
                let n = stmt.execute(params![
                    e.session_id,
                    e.ts,
                    e.kind.as_str(),
                    e.question,
                    e.answer_summary,
                    e.command,
                    e.command_stdout,
                    e.command_stderr,
                    e.interrupted as i64,
                    e.truncated as i64,
                    e.tool_use_id,
                    e.parent_uuid,
                    e.source_line,
                ])?;
                inserted += n;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn last_source_line(&self, session_id: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let max = conn
            .query_row(
                "SELECT MAX(source_line) FROM entries WHERE session_id = ?1",
                params![session_id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten();
        Ok(max)
    }

    pub fn session_count(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?)
    }

    pub fn entry_count(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))?)
    }

    pub fn entry_count_by_kind(&self, kind: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE kind = ?1",
            params![kind],
            |r| r.get(0),
        )?)
    }

    pub fn refresh_session_metadata(&self, meta: &SessionMeta) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            UPDATE sessions SET
                parent_session_id    = ?2,
                is_subagent          = ?3,
                subagent_description = COALESCE(?4, subagent_description),
                subagent_type        = COALESCE(?5, subagent_type)
            WHERE session_id = ?1
            "#,
            params![
                meta.session_id,
                meta.parent_session_id,
                meta.is_subagent as i64,
                meta.subagent_description,
                meta.subagent_type,
            ],
        )?;
        Ok(())
    }

    pub fn subagent_session_count(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE is_subagent = 1",
            [],
            |r| r.get(0),
        )?)
    }
}
