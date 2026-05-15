//! `tape export` — render a cassette to portable, non-Claude-Code-friendly
//! formats.
//!
//! Step-1 vertical slice (issue #8): GitHub-flavored Markdown only.
//! HTML rendering with themes / filter chips, the post-render
//! defense-in-depth re-scan, audience presets, `--strip-internal`,
//! per-kind payload caps, `.taperc::export:` config, and the
//! `/tape:tape-export` plugin slash command are explicit Step 2–4
//! follow-ons.
//!
//! ## Surface
//!
//! - [`render_markdown`] — pure-function over a parsed cassette.
//!   Returns the rendered string (or a typed error if the cassette
//!   itself is malformed). No IO.
//!
//! Design notes:
//!
//! - Output targets the lowest-common-denominator Jira / Linear /
//!   Notion / Slack-paste use case. No HTML, no `<details>`
//!   collapsibles. That richness lives in the Step-2 HTML renderer.
//! - Per-track headers use `###` (H3) so the document fits cleanly
//!   underneath a parent doc that already has an H2 ("Postmortem:
//!   bug-447 — investigation tape").
//! - Per-kind payload bodies are deliberately small. The cassette
//!   itself is the source of truth; the export is a *summary*. A
//!   reader who wants every byte runs `tape play` on the file.

use std::fmt::Write;

use thiserror::Error;

use tape_format::meta::{Meta, Outcome};
use tape_format::reader::RawTape;
use tape_format::tracks::{parse_jsonl, Kind, Track};

/// What can go wrong rendering a cassette. The valid `Ok(String)`
/// branch is "rendered markdown text"; everything here is a refusal
/// because the cassette itself is missing pieces the renderer can't
/// fabricate.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("input cassette is missing meta.yaml")]
    MissingMeta,
    #[error("meta.yaml does not parse: {0}")]
    BadMeta(String),
    #[error("input cassette is missing tracks.jsonl")]
    MissingTracks,
    #[error("tracks.jsonl does not parse: {0}")]
    BadTracks(String),
}

/// Render a parsed `RawTape` to Markdown. The function is pure — no
/// IO, no time-dependent values. Snapshot tests pin the exact shape
/// against a small set of fixture cassettes.
///
/// The layout, in order:
///
/// 1. H1 title — `meta.label` if present, else first 60 chars of
///    `meta.task`, else `meta.id`.
/// 2. Four-line metadata block: recorded / outcome / tracks /
///    redactions.
/// 3. H2 "Liner notes" inlining `liner-notes.md`.
/// 4. H2 "Tracklist" — one H3 per track plus a payload-specific
///    body.
pub fn render_markdown(raw: &RawTape) -> Result<String, ExportError> {
    let meta_yaml = raw.meta_yaml.as_deref().ok_or(ExportError::MissingMeta)?;
    let meta = Meta::parse(meta_yaml).map_err(|e| ExportError::BadMeta(e.to_string()))?;
    let tracks_jsonl = raw
        .tracks_jsonl
        .as_deref()
        .ok_or(ExportError::MissingTracks)?;
    let tracks = parse_jsonl(tracks_jsonl).map_err(|e| ExportError::BadTracks(e.to_string()))?;
    let liner = raw.liner_md.as_deref().unwrap_or("").trim_end();

    let mut s = String::with_capacity(1024 + tracks_jsonl.len());

    // 1. Title.
    let title = title_from(&meta);
    let _ = writeln!(s, "# {title}");
    s.push('\n');

    // 2. Metadata block.
    write_metadata_block(&mut s, &meta, &tracks);

    // 3. Liner notes section.
    s.push_str("## Liner notes\n\n");
    if liner.is_empty() {
        s.push_str("_(no liner notes recorded)_\n");
    } else {
        s.push_str(liner);
        if !liner.ends_with('\n') {
            s.push('\n');
        }
    }
    s.push('\n');

    // 4. Tracklist.
    s.push_str("## Tracklist\n\n");
    if tracks.is_empty() {
        s.push_str("_(no tracks)_\n");
    } else {
        for t in &tracks {
            write_track_block(&mut s, t);
        }
    }

    Ok(s)
}

fn title_from(meta: &Meta) -> String {
    if let Some(label) = meta.label.as_deref().filter(|s| !s.is_empty()) {
        return label.to_owned();
    }
    if !meta.task.is_empty() {
        let trimmed = meta.task.trim();
        let short: String = trimmed.chars().take(60).collect();
        if trimmed.chars().count() > 60 {
            return format!("{short}…");
        }
        return short;
    }
    meta.id.clone()
}

fn write_metadata_block(s: &mut String, meta: &Meta, tracks: &[Track]) {
    let _ = writeln!(
        s,
        "**Recorded:** {} by `{}`",
        meta.created_at, meta.recorder.agent
    );
    let _ = writeln!(s, "**Outcome:** {}", outcome_str(meta.outcome));
    let _ = writeln!(
        s,
        "**Tracks:** {} ({})",
        tracks.len(),
        kind_histogram(tracks)
    );
    if let Some(rs) = meta.redaction_summary.as_ref() {
        let rules = if rs.rules_applied.is_empty() {
            "no rules listed".to_owned()
        } else {
            rs.rules_applied.join(", ")
        };
        let _ = writeln!(s, "**Redactions:** {} ({})", rs.redaction_count, rules);
    }
    s.push('\n');
}

fn outcome_str(o: Outcome) -> &'static str {
    match o {
        Outcome::Success => "success",
        Outcome::Failure => "failure",
        Outcome::Abandoned => "abandoned",
        Outcome::Unknown => "unknown",
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

/// `kind ×N` histogram in `tape ls` order, omitting zero counts. The
/// fixed ordering keeps the rendered string deterministic across
/// HashMap iteration whims and makes snapshot tests robust.
fn kind_histogram(tracks: &[Track]) -> String {
    let mut h = [0u64; 8];
    for t in tracks {
        h[t.kind as usize] += 1;
    }
    let order = [
        Kind::Task,
        Kind::ModelCall,
        Kind::McpCall,
        Kind::Shell,
        Kind::FileRead,
        Kind::FileWrite,
        Kind::Annotation,
        Kind::Eject,
    ];
    let parts: Vec<String> = order
        .iter()
        .filter_map(|k| {
            let n = h[*k as usize];
            if n == 0 {
                None
            } else {
                Some(format!("`{}` ×{}", kind_name(*k), n))
            }
        })
        .collect();
    parts.join(", ")
}

fn write_track_block(s: &mut String, t: &Track) {
    // Header: `### step. `kind` · ts`. The per-kind body below
    // carries everything the `tape_play::label` summary would have
    // duplicated (vendor/model, command text, file path, etc.), so
    // we don't render the label line — keeps the markdown cleaner
    // for paste-into-Slack readers without losing information.
    let _ = writeln!(s, "### {}. `{}` · {}", t.step, kind_name(t.kind), t.ts);
    s.push('\n');

    // Body. Per-kind payload renderers are intentionally small in
    // Step 1: enough to make the export skim-readable, not enough
    // to replace `tape play`. Steps 2+ grow the model_call summary
    // (collapsed via <details> in HTML) and add audience-aware
    // truncation.
    match t.kind {
        Kind::Task => render_task_body(s, t),
        Kind::Shell => render_shell_body(s, t),
        Kind::Annotation => render_annotation_body(s, t),
        Kind::ModelCall => render_model_call_body(s, t),
        Kind::FileRead | Kind::FileWrite => render_file_body(s, t),
        Kind::McpCall => render_mcp_call_body(s, t),
        Kind::Eject => render_eject_body(s, t),
    }
}

fn render_task_body(s: &mut String, t: &Track) {
    let prompt = t
        .payload
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if prompt.is_empty() {
        return;
    }
    write_blockquote(s, prompt);
    s.push('\n');
}

fn render_shell_body(s: &mut String, t: &Track) {
    let cmd = t
        .payload
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if cmd.is_empty() {
        return;
    }
    s.push_str("```console\n$ ");
    s.push_str(cmd);
    if !cmd.ends_with('\n') {
        s.push('\n');
    }
    s.push_str("```\n\n");
}

fn render_annotation_body(s: &mut String, t: &Track) {
    let by = t
        .payload
        .get("by")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");
    let note = t
        .payload
        .get("note")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let _ = writeln!(s, "📌 by `{by}`");
    s.push('\n');
    if !note.is_empty() {
        write_blockquote(s, note);
        s.push('\n');
    }
}

fn render_model_call_body(s: &mut String, t: &Track) {
    let p = &t.payload;
    let vendor = p.get("vendor").and_then(serde_json::Value::as_str);
    let model = p.get("model").and_then(serde_json::Value::as_str);
    if let (Some(v), Some(m)) = (vendor, model) {
        let _ = writeln!(s, "Model: `{v}/{m}`");
    }
    if let Some(tin) = p.get("tokens_in").and_then(serde_json::Value::as_u64) {
        let tout = p.get("tokens_out").and_then(serde_json::Value::as_u64);
        match tout {
            Some(out) => {
                let _ = writeln!(s, "Tokens: in {tin} · out {out}");
            }
            None => {
                let _ = writeln!(s, "Tokens: in {tin}");
            }
        }
    }
    s.push('\n');
}

fn render_file_body(s: &mut String, t: &Track) {
    let path = t
        .payload
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let bytes = t.payload.get("bytes").and_then(serde_json::Value::as_u64);
    if path.is_empty() && bytes.is_none() {
        return;
    }
    if let Some(b) = bytes {
        let _ = writeln!(s, "`{path}` — {b} bytes");
    } else {
        let _ = writeln!(s, "`{path}`");
    }
    s.push('\n');
}

fn render_mcp_call_body(s: &mut String, t: &Track) {
    let server = t
        .payload
        .get("server")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");
    let tool = t
        .payload
        .get("tool")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");
    let _ = writeln!(s, "Tool: `{server}.{tool}`");
    s.push('\n');
}

fn render_eject_body(s: &mut String, t: &Track) {
    let outcome = t
        .payload
        .get("outcome")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if !outcome.is_empty() {
        let _ = writeln!(s, "Outcome: `{outcome}`");
    }
    s.push('\n');
}

/// Render `text` as a Markdown blockquote. Multi-line strings get a
/// `> ` prefix on every line; carriage returns are normalised to
/// newlines so a Windows-recorded note doesn't break the quote.
fn write_blockquote(s: &mut String, text: &str) {
    for line in text.replace('\r', "").split('\n') {
        if line.is_empty() {
            s.push_str(">\n");
        } else {
            s.push_str("> ");
            s.push_str(line);
            s.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_prefers_label_over_task() {
        let mut meta = synth_meta();
        meta.label = Some("bug-447".into());
        meta.task = "investigate the refund race".into();
        assert_eq!(title_from(&meta), "bug-447");
    }

    #[test]
    fn title_falls_back_to_task_truncated() {
        let mut meta = synth_meta();
        meta.label = None;
        meta.task = "a".repeat(120);
        let title = title_from(&meta);
        // 60 chars + the ellipsis.
        assert_eq!(title.chars().count(), 61);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn title_falls_back_to_id_when_empty_task() {
        let mut meta = synth_meta();
        meta.label = None;
        meta.task = String::new();
        meta.id = "01h8xyz-id".into();
        assert_eq!(title_from(&meta), "01h8xyz-id");
    }

    #[test]
    fn kind_histogram_is_deterministic_order() {
        let tracks = vec![
            stub_track(1, Kind::Task),
            stub_track(2, Kind::ModelCall),
            stub_track(3, Kind::Shell),
            stub_track(4, Kind::ModelCall),
            stub_track(5, Kind::Eject),
        ];
        // `model_call` ×2 comes before `shell` ×1 because the fixed
        // order puts model_call ahead of shell, regardless of when
        // tracks of each kind first appear.
        assert_eq!(
            kind_histogram(&tracks),
            "`task` ×1, `model_call` ×2, `shell` ×1, `eject` ×1"
        );
    }

    #[test]
    fn blockquote_handles_empty_lines() {
        let mut s = String::new();
        write_blockquote(&mut s, "first\n\nthird");
        assert_eq!(s, "> first\n>\n> third\n");
    }

    fn synth_meta() -> Meta {
        Meta {
            tape_version: "tape/v0".into(),
            id: "01h8xy00-0000-7000-b8aa-000000000008".into(),
            created_at: "2026-05-14T09:00:00Z".into(),
            ejected_at: "2026-05-14T09:00:30Z".into(),
            task: "test".into(),
            recorder: tape_format::meta::Recorder {
                agent: "test/0.0.1".into(),
                user: None,
            },
            outcome: Outcome::Success,
            models: vec![],
            tools: vec![],
            tool_budget: None,
            redaction_summary: None,
            label: None,
            recap: None,
            recaps: vec![],
            new_block: None,
        }
    }

    fn stub_track(step: u64, kind: Kind) -> Track {
        Track {
            step,
            kind,
            ts: "2026-05-14T09:00:00Z".into(),
            payload: serde_json::json!({}),
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        }
    }
}
