use std::fmt::Write;

use anyhow::{Result, bail};
use owo_colors::OwoColorize;

use crate::cli::OpenArgs;
use crate::config::Paths;
use crate::results_cache::{self, CachedHit};
use crate::store::{EntryRecord, Store};
use crate::util::{atty_stdout, clean_for_display, fmt_ts};

pub fn run(args: OpenArgs) -> Result<()> {
    let cache = match results_cache::read()? {
        Some(c) => c,
        None => bail!(
            "no recent search to open; run `larder find ...`, `larder ask ...`, or `larder grep ...` first"
        ),
    };
    if cache.hits.is_empty() {
        bail!("last search returned no hits; nothing to open");
    }
    let target = cache.hits.iter().find(|h| h.rank == args.rank);
    let Some(target) = target else {
        bail!(
            "rank {} out of range; last `{}` returned {} hits",
            args.rank,
            cache.produced_by,
            cache.hits.len()
        );
    };
    let paths = Paths::resolve()?;
    let store = Store::open(&paths.db_path)?;
    let color = !args.no_color && atty_stdout();
    let out = if args.session {
        render_session(&store, target, args.raw, color)?
    } else {
        render_turn(&store, target, args.raw, color)?
    };
    print!("{}", out);
    Ok(())
}

fn render_turn(store: &Store, target: &CachedHit, raw: bool, color: bool) -> Result<String> {
    let entry = match store.entry_by_id(target.entry_id)? {
        Some(e) => e,
        None => bail!(
            "entry id {} no longer in database (re-ingested?); re-run search",
            target.entry_id
        ),
    };
    let mut out = String::new();
    write_session_header(&mut out, &entry, color);
    out.push('\n');
    write_entry_body(&mut out, &entry, raw, color);
    Ok(out)
}

fn render_session(store: &Store, target: &CachedHit, raw: bool, color: bool) -> Result<String> {
    let entries = store.session_entries(&target.session_id)?;
    if entries.is_empty() {
        bail!(
            "session {} has no entries (re-ingested?); re-run search",
            target.session_id
        );
    }
    let mut out = String::new();
    let head = entries.first().expect("non-empty checked above");
    let badge = if head.is_subagent { " [subagent]" } else { "" };
    let header = format!(
        "[Session {} · {}{} · {} turns]",
        head.session_id,
        head.project_path,
        badge,
        entries.len()
    );
    let _ = if color {
        writeln!(out, "{}", header.bold())
    } else {
        writeln!(out, "{}", header)
    };
    out.push('\n');
    for (i, entry) in entries.iter().enumerate() {
        let is_match = entry.id == target.entry_id;
        let marker = if is_match { " >>> (matched)" } else { "" };
        let turn_header = format!("--- Turn {} · {}{} ---", i + 1, fmt_ts(entry.ts), marker);
        let _ = if color && is_match {
            writeln!(out, "{}", turn_header.yellow().bold())
        } else if color {
            writeln!(out, "{}", turn_header.dimmed())
        } else {
            writeln!(out, "{}", turn_header)
        };
        write_entry_body(&mut out, entry, raw, color);
        out.push('\n');
    }
    Ok(out)
}

fn write_session_header(out: &mut String, entry: &EntryRecord, color: bool) {
    let badge = if entry.is_subagent {
        match &entry.subagent_description {
            Some(d) if !d.is_empty() => format!(" [subagent: \"{}\"]", d),
            _ => " [subagent]".to_string(),
        }
    } else {
        String::new()
    };
    let header = format!(
        "[Session {} · {} · {}{}]",
        entry.session_id,
        fmt_ts(entry.ts),
        entry.project_path,
        badge
    );
    let _ = if color {
        writeln!(out, "{}", header.bold())
    } else {
        writeln!(out, "{}", header)
    };
}

fn write_entry_body(out: &mut String, entry: &EntryRecord, raw: bool, color: bool) {
    if let Some(q) = &entry.question {
        let q_text = if raw { q.clone() } else { clean_for_display(q) };
        let label = if color {
            "Q:".cyan().to_string()
        } else {
            "Q:".to_string()
        };
        let _ = writeln!(out, "{} {}", label, q_text);
        out.push('\n');
    }
    if let Some(answer) = &entry.answer_summary {
        let a_text = if raw {
            answer.clone()
        } else {
            clean_for_display(answer)
        };
        let label = if color {
            "A:".magenta().to_string()
        } else {
            "A:".to_string()
        };
        let _ = writeln!(out, "{} {}", label, a_text);
        out.push('\n');
    }
    if let Some(cmd) = &entry.command {
        let label = if color {
            "$".green().to_string()
        } else {
            "$".to_string()
        };
        let _ = writeln!(out, "{} {}", label, cmd);
    }
    if let Some(stdout) = &entry.command_stdout {
        let _ = writeln!(out, "↳ {}", stdout);
    }
    if let Some(stderr) = &entry.command_stderr {
        let _ = writeln!(out, "↳ stderr: {}", stderr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry() -> EntryRecord {
        EntryRecord {
            id: 1,
            session_id: "s".to_string(),
            ts: 1700000000,
            project_path: "/p".to_string(),
            kind: "qa".to_string(),
            question: Some("what is x?".to_string()),
            command: None,
            command_stdout: None,
            command_stderr: None,
            answer_summary: Some("x is the unknown".to_string()),
            thinking: None,
            is_subagent: false,
            subagent_description: None,
        }
    }

    #[test]
    fn write_entry_body_emits_q_and_a() {
        let mut out = String::new();
        write_entry_body(&mut out, &make_entry(), false, false);
        assert!(out.contains("Q: what is x?"));
        assert!(out.contains("A: x is the unknown"));
    }

    #[test]
    fn write_entry_body_filters_when_not_raw() {
        let mut e = make_entry();
        e.question = Some("[Pasted text #1 +5 lines] real q".to_string());
        let mut out = String::new();
        write_entry_body(&mut out, &e, false, false);
        assert!(out.contains("[paste] real q"));
        assert!(!out.contains("Pasted text #1"));
    }

    #[test]
    fn write_entry_body_preserves_when_raw() {
        let mut e = make_entry();
        e.question = Some("[Pasted text #1 +5 lines] real q".to_string());
        let mut out = String::new();
        write_entry_body(&mut out, &e, true, false);
        assert!(out.contains("[Pasted text #1 +5 lines] real q"));
    }

    #[test]
    fn write_entry_body_emits_command_and_stdout() {
        let mut e = make_entry();
        e.question = None;
        e.answer_summary = None;
        e.command = Some("ls -la".to_string());
        e.command_stdout = Some("file1\nfile2".to_string());
        let mut out = String::new();
        write_entry_body(&mut out, &e, false, false);
        assert!(out.contains("$ ls -la"));
        assert!(out.contains("↳ file1"));
    }
}
