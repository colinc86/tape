//! Read-side tools — `ls`, `play`, and shared label synthesis.
//!
//! All operations consume an already-loaded `RawTape` plus a parsed track list.
//! No IO happens in this crate beyond what its caller passes in.

use std::fmt::Write;

use serde_json::Value;
use tape_format::tracks::{Kind, Track};

/// Render one line per track for `tape ls`.
///
/// Format: `  <step:3> <kind:13> <label>`
pub fn render_ls(tracks: &[Track]) -> String {
    let mut out = String::new();
    for t in tracks {
        let _ = writeln!(
            out,
            "  {:>3}  {:<12}  {}",
            t.step,
            kind_name(t.kind),
            label(t)
        );
    }
    out
}

/// Render full track payloads for `tape play` (default, no filter — but
/// caller restricts via `--step` / `--range` / `--kind` before passing in).
pub fn render_play(tracks: &[Track]) -> String {
    let mut out = String::new();
    for t in tracks {
        let _ = writeln!(
            out,
            "── step {} · {} · {} ──",
            t.step,
            kind_name(t.kind),
            t.ts
        );
        let pretty = serde_json::to_string_pretty(&t.payload)
            .unwrap_or_else(|_| t.payload.to_string());
        out.push_str(&pretty);
        out.push_str("\n\n");
    }
    out
}

/// Default summary view for `tape play <file>` with no filter — meta line plus ls.
pub fn render_summary_view(meta_yaml: &str, liner_md: &str, tracks: &[Track]) -> String {
    let mut out = String::new();
    out.push_str("══ liner notes ══\n\n");
    out.push_str(liner_md);
    if !liner_md.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n══ meta ══\n\n");
    out.push_str(meta_yaml);
    if !meta_yaml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n══ tracks ══\n");
    out.push_str(&render_ls(tracks));
    out
}

/// One-line semantic label for a track. Used by `tape ls`,
/// the deck's `tape.tracks` tool, and the diff aligner.
pub fn label(t: &Track) -> String {
    match t.kind {
        Kind::Task => format!(
            "{:?}",
            t.payload.get("prompt").and_then(Value::as_str).unwrap_or("")
        ),
        Kind::ModelCall => {
            let vendor = t.payload.get("vendor").and_then(Value::as_str).unwrap_or("?");
            let model = t.payload.get("model").and_then(Value::as_str).unwrap_or("?");
            let tin = t
                .payload
                .get("tokens_in")
                .and_then(Value::as_u64)
                .map(|n| format!(" in:{n}"))
                .unwrap_or_default();
            let tout = t
                .payload
                .get("tokens_out")
                .and_then(Value::as_u64)
                .map(|n| format!(" out:{n}"))
                .unwrap_or_default();
            format!("{vendor}/{model}{tin}{tout}")
        }
        Kind::McpCall => {
            let server = t.payload.get("server").and_then(Value::as_str).unwrap_or("?");
            let tool = t.payload.get("tool").and_then(Value::as_str).unwrap_or("?");
            let args_summary = t
                .payload
                .get("args")
                .map(summarize_args)
                .unwrap_or_else(|| "()".into());
            format!("{server}.{tool}{args_summary}")
        }
        Kind::Shell => {
            let cmd = t.payload.get("command").and_then(Value::as_str).unwrap_or("");
            truncate(cmd, 80)
        }
        Kind::FileRead => {
            let path = t.payload.get("path").and_then(Value::as_str).unwrap_or("?");
            format!("read({path})")
        }
        Kind::FileWrite => {
            let path = t.payload.get("path").and_then(Value::as_str).unwrap_or("?");
            format!("write({path})")
        }
        Kind::Annotation => t
            .payload
            .get("note")
            .and_then(Value::as_str)
            .map(|s| format!("{:?}", truncate(s, 80)))
            .unwrap_or_else(|| "(no note)".into()),
        Kind::Eject => t
            .payload
            .get("outcome")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .into(),
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Task => "task",
        Kind::ModelCall => "model_call",
        Kind::McpCall => "mcp_call",
        Kind::Shell => "shell",
        Kind::FileRead => "file_read",
        Kind::FileWrite => "file_write",
        Kind::Annotation => "annotation",
        Kind::Eject => "eject",
    }
}

fn summarize_args(v: &Value) -> String {
    let s = v.to_string();
    let truncated = truncate(&s, 80);
    if truncated.starts_with('(') {
        truncated
    } else {
        format!("({truncated})")
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.replace('\n', " ").to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out.replace('\n', " ")
    }
}

/// Filter tracks by an optional kind name and step range.
pub fn filter<'a>(
    tracks: &'a [Track],
    step: Option<u64>,
    range: Option<(u64, u64)>,
    kind: Option<&str>,
) -> Vec<&'a Track> {
    let parsed_kind = kind.and_then(parse_kind);
    tracks
        .iter()
        .filter(|t| match step {
            Some(s) => t.step == s,
            None => true,
        })
        .filter(|t| match range {
            Some((lo, hi)) => t.step >= lo && t.step <= hi,
            None => true,
        })
        .filter(|t| match parsed_kind {
            Some(k) => t.kind == k,
            None => true,
        })
        .collect()
}

/// Parse a kind name from CLI input.
pub fn parse_kind(name: &str) -> Option<Kind> {
    match name {
        "task" => Some(Kind::Task),
        "model_call" => Some(Kind::ModelCall),
        "mcp_call" => Some(Kind::McpCall),
        "shell" => Some(Kind::Shell),
        "file_read" => Some(Kind::FileRead),
        "file_write" => Some(Kind::FileWrite),
        "annotation" => Some(Kind::Annotation),
        "eject" => Some(Kind::Eject),
        _ => None,
    }
}

/// Parse a `--range N..M` argument.
pub fn parse_range(s: &str) -> Option<(u64, u64)> {
    let (lo, hi) = s.split_once("..")?;
    Some((lo.parse().ok()?, hi.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn t(step: u64, kind: Kind, payload: Value) -> Track {
        Track {
            step,
            kind,
            ts: format!("2026-05-06T10:00:{step:02}Z"),
            payload,
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        }
    }

    #[test]
    fn label_task() {
        let track = t(1, Kind::Task, json!({"prompt": "Investigate"}));
        assert_eq!(label(&track), r#""Investigate""#);
    }

    #[test]
    fn label_mcp_call() {
        let track = t(
            2,
            Kind::McpCall,
            json!({"server": "db", "tool": "query", "args": {"sql": "SELECT 1"}}),
        );
        assert!(label(&track).starts_with("db.query("));
    }

    #[test]
    fn render_ls_has_one_line_per_track() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_ls(&tracks);
        assert_eq!(s.lines().count(), 2);
    }

    #[test]
    fn parse_range_works() {
        assert_eq!(parse_range("3..7"), Some((3, 7)));
        assert_eq!(parse_range("not-a-range"), None);
    }
}
