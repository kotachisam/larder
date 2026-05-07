use anyhow::Result;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy)]
pub enum DisplayMode {
    Compact,
    Full,
    Raw,
}

const XML_BLOCKS_TO_STRIP: &[&str] = &[
    "system-reminder",
    "command-name",
    "command-message",
    "local-command-stdout",
    "local-command-caveat",
    "thinking",
];

pub fn clean_for_display(s: &str) -> String {
    let mut out = s.to_string();
    for tag in XML_BLOCKS_TO_STRIP {
        out = strip_xml_block(&out, tag);
    }
    out = compress_marker(&out, "[Pasted text #", "]", "[paste]");
    out = compress_marker(&out, "[Image #", "]", "[image]");
    out = collapse_cc_paste_blocks(&out);
    out.trim().to_string()
}

fn line_is_cc_marker(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('⏺') || trimmed.starts_with('⎿') || trimmed.starts_with('❯')
}

fn line_is_indented_or_blank(line: &str) -> bool {
    line.trim().is_empty() || line.starts_with(' ') || line.starts_with('\t')
}

fn collapse_cc_paste_blocks(s: &str) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let mut out_lines: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if line_is_cc_marker(lines[i]) {
            let mut j = i + 1;
            while j < lines.len()
                && (line_is_cc_marker(lines[j]) || line_is_indented_or_blank(lines[j]))
            {
                j += 1;
            }
            out_lines.push("[paste]".to_string());
            i = j;
        } else {
            out_lines.push(lines[i].to_string());
            i += 1;
        }
    }
    out_lines.join("\n")
}

fn strip_xml_block(s: &str, tag: &str) -> String {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let mut result = String::with_capacity(s.len());
    let mut cursor = 0;
    while let Some(start_rel) = s[cursor..].find(&open) {
        let abs_start = cursor + start_rel;
        result.push_str(&s[cursor..abs_start]);
        let body_start = abs_start + open.len();
        match s[body_start..].find(&close) {
            Some(end_rel) => cursor = body_start + end_rel + close.len(),
            None => {
                result.push_str(&s[abs_start..]);
                return result;
            }
        }
    }
    result.push_str(&s[cursor..]);
    result
}

fn compress_marker(s: &str, start_marker: &str, end_marker: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut cursor = 0;
    while let Some(start_rel) = s[cursor..].find(start_marker) {
        let abs_start = cursor + start_rel;
        result.push_str(&s[cursor..abs_start]);
        let body_start = abs_start + start_marker.len();
        match s[body_start..].find(end_marker) {
            Some(end_rel) => {
                cursor = body_start + end_rel + end_marker.len();
                result.push_str(replacement);
            }
            None => {
                result.push_str(&s[abs_start..]);
                return result;
            }
        }
    }
    result.push_str(&s[cursor..]);
    result
}

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

    #[test]
    fn clean_strips_system_reminder() {
        let s = "before <system-reminder>noise here</system-reminder> after";
        assert_eq!(clean_for_display(s), "before  after");
    }

    #[test]
    fn clean_strips_multiline_xml_block() {
        let s = "before\n<local-command-stdout>line1\nline2</local-command-stdout>\nafter";
        assert_eq!(clean_for_display(s), "before\n\nafter");
    }

    #[test]
    fn clean_strips_multiple_block_types() {
        let s = "<system-reminder>a</system-reminder>middle<command-name>b</command-name>";
        assert_eq!(clean_for_display(s), "middle");
    }

    #[test]
    fn clean_handles_repeated_blocks() {
        let s = "<thinking>x</thinking>keep<thinking>y</thinking>";
        assert_eq!(clean_for_display(s), "keep");
    }

    #[test]
    fn clean_leaves_unclosed_block_alone() {
        let s = "before <system-reminder>unclosed text";
        assert_eq!(
            clean_for_display(s),
            "before <system-reminder>unclosed text"
        );
    }

    #[test]
    fn clean_compresses_paste_marker() {
        let s = "[Pasted text #43 +35 lines] here is the question";
        assert_eq!(clean_for_display(s), "[paste] here is the question");
    }

    #[test]
    fn clean_compresses_image_marker() {
        let s = "look at [Image #17] and [Image #18]";
        assert_eq!(clean_for_display(s), "look at [image] and [image]");
    }

    #[test]
    fn clean_passthrough_when_no_patterns() {
        let s = "just normal user prose with no harness gunk";
        assert_eq!(clean_for_display(s), s);
    }

    #[test]
    fn clean_is_idempotent() {
        let s = "<system-reminder>a</system-reminder>[Pasted text #1 +2 lines]b";
        let once = clean_for_display(s);
        assert_eq!(once, clean_for_display(&once));
    }

    #[test]
    fn clean_real_world_zkp_example() {
        let s = "[Pasted text #43 +35 lines] here's the ZKP questions";
        assert_eq!(clean_for_display(s), "[paste] here's the ZKP questions");
    }

    #[test]
    fn cc_paste_at_line_start_collapses() {
        let s = "⏺ assistant said this\n  continuation line\n⎿ tool output";
        assert_eq!(clean_for_display(s), "[paste]");
    }

    #[test]
    fn cc_paste_with_wrapping_prose_keeps_prose() {
        let s = "Here is what I tried:\n⏺ assistant blah\n⎿ output\nWhat went wrong?";
        let out = clean_for_display(s);
        assert!(out.contains("Here is what I tried:"));
        assert!(out.contains("[paste]"));
        assert!(out.contains("What went wrong?"));
        assert!(!out.contains('⏺'));
        assert!(!out.contains('⎿'));
    }

    #[test]
    fn cc_glyph_in_middle_of_prose_is_not_collapsed() {
        let s = "I noticed the ⏺ glyph appears in CC prompt prefixes";
        assert_eq!(
            clean_for_display(s),
            "I noticed the ⏺ glyph appears in CC prompt prefixes"
        );
    }

    #[test]
    fn multiple_cc_blocks_each_collapse() {
        let s = "first thing\n⏺ paste one\nmiddle prose\n❯ paste two\nfinal thought";
        let out = clean_for_display(s);
        assert!(out.contains("first thing"));
        assert!(out.contains("middle prose"));
        assert!(out.contains("final thought"));
        assert_eq!(out.matches("[paste]").count(), 2);
    }

    #[test]
    fn cc_block_includes_indented_continuations() {
        let s = "⏺ Bash(docker images)\n  ⎿  REPOSITORY  TAG  ID\n     postgres   latest  abc\n     mongo      latest  def\nWhat are these?";
        let out = clean_for_display(s);
        assert!(out.contains("[paste]"));
        assert!(out.contains("What are these?"));
        assert!(!out.contains("postgres"));
        assert!(!out.contains("REPOSITORY"));
    }

    #[test]
    fn pure_prose_unaffected_by_cc_collapser() {
        let s = "Just a normal question with no harness gunk at all.";
        assert_eq!(clean_for_display(s), s);
    }
}
