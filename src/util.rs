use anyhow::Result;
use chrono::{DateTime, Utc};

pub fn since_seconds(spec: Option<&str>) -> Result<i64> {
    let Some(s) = spec else {
        return Ok(0);
    };
    let now = Utc::now().timestamp();
    let dur = humantime::parse_duration(s)
        .map_err(|e| anyhow::anyhow!("invalid --since '{}': {}", s, e))?;
    Ok(now - dur.as_secs() as i64)
}

pub fn fmt_ts(ts: i64) -> String {
    if ts == 0 {
        return "?".to_string();
    }
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "?".to_string())
}

pub fn atty_stdout() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

pub fn snip(s: &str, max: usize, preserve_newlines: bool) -> String {
    let collapsed = if preserve_newlines {
        s.to_string()
    } else {
        s.replace('\n', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    if collapsed.len() <= max {
        return collapsed;
    }
    let mut end = max;
    while !collapsed.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}…", &collapsed[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snip_collapses_newlines_by_default() {
        let s = "hello\nworld\n  foo";
        assert_eq!(snip(s, 100, false), "hello world foo");
    }

    #[test]
    fn snip_preserves_newlines_when_asked() {
        let s = "hello\nworld";
        assert_eq!(snip(s, 100, true), "hello\nworld");
    }

    #[test]
    fn snip_truncates_with_ellipsis() {
        let s = "abcdefghij";
        assert_eq!(snip(s, 5, false), "abcde…");
    }

    #[test]
    fn snip_respects_char_boundaries() {
        let s = "héllo wörld";
        let out = snip(s, 3, false);
        assert!(out.ends_with('…'));
        assert!(out.is_char_boundary(out.len() - "…".len()));
    }

    #[test]
    fn fmt_ts_zero_is_question_mark() {
        assert_eq!(fmt_ts(0), "?");
    }

    #[test]
    fn fmt_ts_renders_unix_seconds() {
        assert_eq!(fmt_ts(1700000000), "2023-11-14 22:13");
    }

    #[test]
    fn since_seconds_none_returns_zero() {
        assert_eq!(since_seconds(None).unwrap(), 0);
    }

    #[test]
    fn since_seconds_parses_humantime() {
        let now = Utc::now().timestamp();
        let cutoff = since_seconds(Some("1h")).unwrap();
        let delta = now - cutoff;
        assert!((3590..=3610).contains(&delta), "delta was {}", delta);
    }

    #[test]
    fn since_seconds_invalid_errors() {
        assert!(since_seconds(Some("not-a-duration")).is_err());
    }
}
