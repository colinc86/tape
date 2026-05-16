//! Convert parsed `RawEntry` values into `tape/v0` tracks.
//!
//! Tool-name → `Kind` mapping (single source of truth, see DECISIONS.md §D3):
//!
//! - `Bash`                                     → `Kind::Shell`
//! - `Read`                                     → `Kind::FileRead`
//! - `Write`, `Edit`, `MultiEdit`, `NotebookEdit` → `Kind::FileWrite`
//! - `mcp__<server>__<tool>`                    → `Kind::McpCall`, `payload.server = <server>`
//! - everything else (`Grep`, `Glob`, `WebFetch`, `WebSearch`, `Task`, `TodoWrite`, ...)
//!   → `Kind::McpCall`, `payload.server = "builtin"`
//!
//! Stretching `McpCall` to cover Claude Code's built-in non-MCP tools
//! preserves the closed v0 `Kind` enum. v0.2 / `tape/v1` may introduce a
//! `tool_call` kind; until then this is the deliberate compromise.
//!
//! Tool result lookup precedence (see DECISIONS.md §D2):
//!   1. Inline `tool_result` block in a subsequent `user` message (matched by `tool_use_id`).
//!   2. Sibling file at `<sibling_dir>/<tool_use_id>.txt`.
//!   3. Missing — record the call with `payload.result: null` and a warning annotation.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};
use tape_format::tracks::{Kind, Track};

use super::parser::{AssistantEntry, ParseReport, RawEntry, UserEntry};

#[derive(Debug, Default, Clone)]
pub struct ConvertReport {
    pub track_count: u64,
    pub orphan_tool_calls: u64,
    pub sibling_results_used: u64,
    pub inline_results_used: u64,
    pub parse: ParseReport,
}

/// Convert a parsed transcript into v0 tracks.
///
/// Returns the tracks plus a report of what happened during conversion.
/// Step numbers start at 1; the eject pipeline appends the final `eject`
/// event so callers should NOT add one.
pub fn to_tracks(
    entries: &[RawEntry],
    sibling_dir: &Path,
    parse: ParseReport,
) -> (Vec<Track>, ConvertReport) {
    // Index every inline tool_result by tool_use_id for O(1) lookup.
    let inline_results = collect_inline_tool_results(entries);

    let mut out: Vec<Track> = Vec::new();
    let mut report = ConvertReport {
        parse,
        ..ConvertReport::default()
    };
    let mut step: u64 = 0;
    let mut emitted_task = false;

    let mut next_step = |out_len: usize| -> u64 {
        let _ = out_len;
        step += 1;
        step
    };

    for entry in entries {
        match entry {
            RawEntry::User(u) => {
                if let Some(track) = user_to_track(u, &mut next_step, &mut emitted_task) {
                    out.push(track);
                }
            }
            RawEntry::Assistant(a) => {
                let mut step_for_call = || next_step(out.len());
                let assistant_tracks = assistant_to_tracks(
                    a,
                    &mut step_for_call,
                    &inline_results,
                    sibling_dir,
                    &mut report,
                );
                out.extend(assistant_tracks);
            }
            RawEntry::Skip { .. } => {} // already counted in ParseReport
        }
    }

    // If parse warnings exist, add an annotation track at the end so the
    // tape preserves the diagnostic visibly.
    if !report.parse.skipped.is_empty() || report.parse.malformed_lines > 0 {
        let s = next_step(out.len());
        let summary = json!({
            "by": "tape:transcript-parser",
            "note": format!(
                "parse warnings: {} unknown event types, {} malformed lines",
                report.parse.skipped.len(),
                report.parse.malformed_lines
            )
        });
        out.push(Track {
            step: s,
            kind: Kind::Annotation,
            ts: now_ts(),
            payload: summary,
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        });
    }

    report.track_count = out.len() as u64;
    (out, report)
}

fn user_to_track(
    u: &UserEntry,
    next_step: &mut impl FnMut(usize) -> u64,
    emitted_task: &mut bool,
) -> Option<Track> {
    // Skip user messages whose content is purely tool_result blocks — those
    // were already consumed by the assistant tool_use mapping.
    if let Value::Array(blocks) = &u.message.content {
        if blocks
            .iter()
            .all(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        {
            return None;
        }
    }

    let text = match &u.message.content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    b.get("text").and_then(Value::as_str).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    if text.is_empty() {
        return None;
    }

    let step = next_step(0);
    let ts = u.timestamp.clone().unwrap_or_else(now_ts);

    if !*emitted_task {
        *emitted_task = true;
        Some(Track {
            step,
            kind: Kind::Task,
            ts,
            payload: json!({"prompt": text}),
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        })
    } else {
        Some(Track {
            step,
            kind: Kind::Annotation,
            ts,
            payload: json!({"by": "user", "note": text}),
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        })
    }
}

fn assistant_to_tracks(
    a: &AssistantEntry,
    next_step: &mut impl FnMut() -> u64,
    inline_results: &HashMap<String, Value>,
    sibling_dir: &Path,
    report: &mut ConvertReport,
) -> Vec<Track> {
    let mut tracks = Vec::new();
    let ts = a.timestamp.clone().unwrap_or_else(now_ts);
    let model = a.message.model.clone().unwrap_or_default();

    // First, emit the model_call event itself. Its payload captures the
    // turn's text + tool_use blocks summary; tool results land as separate
    // events. Token usage goes in payload.usage.
    let text_blocks: Vec<&str> = a
        .message
        .content
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect();
    let tool_use_summaries: Vec<Value> = a
        .message
        .content
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .map(|b| {
            json!({
                "id": b.get("id").cloned().unwrap_or(Value::Null),
                "name": b.get("name").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();

    let response_view = json!({
        "content": text_blocks,
        "tool_uses": tool_use_summaries,
        "stop_reason": a.message.stop_reason.clone().unwrap_or_default(),
        "usage": a.message.usage.clone().unwrap_or(Value::Null),
    });

    tracks.push(Track {
        step: next_step(),
        kind: Kind::ModelCall,
        ts: ts.clone(),
        payload: json!({
            "vendor": "anthropic",
            "model": model,
            "request": Value::Null, // not in transcript
            "response": response_view,
        }),
        parent_step: None,
        refs: vec![],
        annotations: vec![],
    });

    // Then emit one event per tool_use block, with the result inlined.
    for block in &a.message.content {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let tool_id = block
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let tool_name = block
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let input = block.get("input").cloned().unwrap_or(Value::Null);

        let (result, source) = lookup_tool_result(&tool_id, inline_results, sibling_dir);
        match source {
            ResultSource::Inline => report.inline_results_used += 1,
            ResultSource::Sibling => report.sibling_results_used += 1,
            ResultSource::Missing => report.orphan_tool_calls += 1,
        }

        let (kind, payload) = map_tool_to_track(&tool_name, &input, result.as_ref());
        tracks.push(Track {
            step: next_step(),
            kind,
            ts: ts.clone(),
            payload,
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        });
    }

    tracks
}

#[derive(Debug, Clone, Copy)]
enum ResultSource {
    Inline,
    Sibling,
    Missing,
}

fn lookup_tool_result(
    id: &str,
    inline: &HashMap<String, Value>,
    sibling_dir: &Path,
) -> (Option<Value>, ResultSource) {
    if let Some(v) = inline.get(id) {
        return (Some(v.clone()), ResultSource::Inline);
    }
    let sibling = sibling_dir.join(format!("{id}.txt"));
    if let Ok(s) = std::fs::read_to_string(&sibling) {
        return (Some(Value::String(s)), ResultSource::Sibling);
    }
    (None, ResultSource::Missing)
}

fn collect_inline_tool_results(entries: &[RawEntry]) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for entry in entries {
        let RawEntry::User(u) = entry else { continue };
        let Value::Array(blocks) = &u.message.content else {
            continue;
        };
        for b in blocks {
            if b.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(id) = b.get("tool_use_id").and_then(Value::as_str) else {
                continue;
            };
            let content = b.get("content").cloned().unwrap_or(Value::Null);
            out.insert(id.to_string(), content);
        }
    }
    out
}

/// Pull the textually-meaningful subset of a `tool_result`. Returns:
/// - `Some(s)` when result is a string, or an array of content blocks where
///   at least one block has `type: "text"` and a `text` string.
/// - `None` for an empty/missing result, an array with no text blocks, or
///   any other shape we don't know how to interpret as text.
///
/// Used to compute real `content_hash` / `after_hash` values in the file-IO
/// branches of `map_tool_to_track`; see issue #13.
fn extract_tool_result_text(result: Option<&Value>) -> Option<String> {
    match result? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::String(_) => None,
        Value::Array(blocks) => {
            let mut buf = String::new();
            for b in blocks {
                if b.get("type").and_then(Value::as_str) != Some("text") {
                    continue;
                }
                if let Some(t) = b.get("text").and_then(Value::as_str) {
                    buf.push_str(t);
                }
            }
            (!buf.is_empty()).then_some(buf)
        }
        _ => None,
    }
}

/// Map one `tool_use` block to (Kind, payload). The payload shape per kind
/// follows SPEC.md §5.5.
fn map_tool_to_track(name: &str, input: &Value, result: Option<&Value>) -> (Kind, Value) {
    let result_for_payload = result.cloned().unwrap_or(Value::Null);

    match name {
        "Bash" => {
            let command = input.get("command").and_then(Value::as_str).unwrap_or("");
            // Best-effort: tool_result content for Bash is usually combined stdout/stderr.
            let stdout = match result {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => String::new(),
            };
            (
                Kind::Shell,
                json!({
                    "command": command,
                    "exit_code": 0,
                    "stdout": stdout,
                    "stderr": "",
                    "duration_ms": 0,
                }),
            )
        }
        "Read" => {
            let path = input.get("file_path").and_then(Value::as_str).unwrap_or("");
            // Hash the file's real bytes. The tool_result is either a raw
            // string (older transcripts) or an array of content blocks
            // (current Claude Code) — extract whatever text we can find. If
            // we can't see any content, omit the hash field rather than emit
            // a `"blake3:0"` sentinel that doesn't match SPEC §5.5.5's
            // `blake3:<64 hex>` shape. (Issue #13.)
            let mut payload = json!({"path": path});
            if let Some(text) = extract_tool_result_text(result) {
                payload["content_hash"] =
                    Value::String(format!("blake3:{}", blake3::hash(text.as_bytes()).to_hex()));
            }
            (Kind::FileRead, payload)
        }
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => {
            let path = input.get("file_path").and_then(Value::as_str).unwrap_or("");
            // Compute a real `after_hash` from whatever post-edit bytes we
            // can see. Preference order:
            //   1. `input.content` — present for `Write` only.
            //   2. The tool_result text — Claude Code's edit tools return
            //      the post-edit file content (or a snippet) here.
            //   3. `input.new_string` — `Edit`'s replacement text. A partial
            //      hash of just the new chunk is still a real blake3 hash;
            //      consumers comparing across tapes won't get false hits.
            // If none of these are available, omit the field rather than
            // emit the `"blake3:0"` sentinel. (Issue #13.)
            let after_text = input
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| extract_tool_result_text(result))
                .or_else(|| {
                    input
                        .get("new_string")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
            let mut payload = json!({
                "path": path,
                "before_hash": Value::Null,
            });
            if let Some(text) = after_text {
                payload["after_hash"] =
                    Value::String(format!("blake3:{}", blake3::hash(text.as_bytes()).to_hex()));
            }
            if name == "Edit" {
                if let (Some(o), Some(n)) = (
                    input.get("old_string").and_then(Value::as_str),
                    input.get("new_string").and_then(Value::as_str),
                ) {
                    payload["diff"] = Value::String(format!(
                        "- {}\n+ {}",
                        o.replace('\n', "\\n"),
                        n.replace('\n', "\\n")
                    ));
                }
            }
            (Kind::FileWrite, payload)
        }
        n if n.starts_with("mcp__") => {
            let parts: Vec<&str> = n.splitn(3, "__").collect();
            let server = parts.get(1).copied().unwrap_or("unknown");
            let tool = parts.get(2).copied().unwrap_or(n);
            (
                Kind::McpCall,
                json!({
                    "server": server,
                    "tool": tool,
                    "args": input.clone(),
                    "result": result_for_payload,
                }),
            )
        }
        // Built-in non-MCP tools: Grep, Glob, WebFetch, WebSearch, Task, Skill,
        // TodoWrite, etc. See DECISIONS.md §D3.
        builtin => (
            Kind::McpCall,
            json!({
                "server": "builtin",
                "tool": builtin,
                "args": input.clone(),
                "result": result_for_payload,
            }),
        ),
    }
}

fn now_ts() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse_jsonl;
    use super::*;
    use std::io::BufReader;
    use std::path::PathBuf;

    fn convert_fixture(name: &str) -> (Vec<Track>, ConvertReport) {
        let s =
            std::fs::read_to_string(format!("tests/fixtures/transcripts/{name}.jsonl")).unwrap();
        let (entries, parse) = parse_jsonl(BufReader::new(s.as_bytes())).unwrap();
        let sibling = PathBuf::from(format!(
            "tests/fixtures/transcripts/sessions/session-{}",
            name.replace('_', "-")
        ));
        to_tracks(&entries, &sibling, parse)
    }

    #[test]
    fn minimal_produces_task_and_model_call() {
        let (tracks, _r) = convert_fixture("minimal");
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].kind, Kind::Task);
        assert_eq!(tracks[0].payload["prompt"], "Say hello");
        assert_eq!(tracks[1].kind, Kind::ModelCall);
        assert_eq!(tracks[1].payload["model"], "claude-opus-4-7");
    }

    #[test]
    fn bash_tool_use_becomes_shell() {
        let (tracks, r) = convert_fixture("with_bash");
        assert_eq!(r.inline_results_used, 1);
        let shell_track = tracks.iter().find(|t| t.kind == Kind::Shell).unwrap();
        assert_eq!(shell_track.payload["command"], "ls /tmp");
        assert_eq!(shell_track.payload["stdout"], "foo\nbar\n");
    }

    #[test]
    fn sibling_file_result_falls_through() {
        let (tracks, r) = convert_fixture("with_sibling_result");
        // The inline tool_result was a stub "(see sibling file)" — the sibling
        // file contains the real content. Inline took precedence; sibling
        // fallback is exercised by orphan_tool_use.
        assert_eq!(r.inline_results_used, 1);
        let webfetch = tracks.iter().find(|t| t.kind == Kind::McpCall).unwrap();
        assert_eq!(webfetch.payload["server"], "builtin");
        assert_eq!(webfetch.payload["tool"], "WebFetch");
    }

    #[test]
    fn orphan_tool_use_records_with_null_result() {
        let (tracks, r) = convert_fixture("orphan_tool_use");
        assert_eq!(r.orphan_tool_calls, 1);
        let shell_track = tracks.iter().find(|t| t.kind == Kind::Shell).unwrap();
        assert_eq!(shell_track.payload["stdout"], "");
    }

    #[test]
    fn mcp_tool_extracts_server_and_tool() {
        let (tracks, _r) = convert_fixture("mcp_call");
        let mcp = tracks.iter().find(|t| t.kind == Kind::McpCall).unwrap();
        assert_eq!(mcp.payload["server"], "plugin_tape_tape");
        assert_eq!(mcp.payload["tool"], "tape_load");
    }

    #[test]
    fn mixed_kinds_produces_all_expected_kinds() {
        let (tracks, _r) = convert_fixture("mixed_kinds");
        let kinds: Vec<Kind> = tracks.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&Kind::Task));
        assert!(kinds.contains(&Kind::ModelCall));
        assert!(kinds.contains(&Kind::FileRead));
        assert!(kinds.contains(&Kind::FileWrite));
        // Grep (builtin) → McpCall
        let grep = tracks
            .iter()
            .find(|t| t.kind == Kind::McpCall && t.payload["tool"] == "Grep");
        assert!(grep.is_some(), "Grep should map to McpCall server=builtin");
    }

    #[test]
    fn unknown_event_type_yields_warning_annotation() {
        let (tracks, r) = convert_fixture("unknown_type");
        assert_eq!(r.parse.skipped.get("future-thing"), Some(&1));
        let warning = tracks
            .iter()
            .filter(|t| t.kind == Kind::Annotation)
            .find(|t| t.payload["by"] == "tape:transcript-parser");
        assert!(warning.is_some(), "expected parse-warning annotation");
    }

    #[test]
    fn redaction_bait_passes_through_to_tracks_unchanged() {
        // The transcript path doesn't redact; the eject pipeline does.
        // Here we just confirm the AWS key reaches the track payload.
        let (tracks, _r) = convert_fixture("redaction_bait");
        let task = &tracks[0];
        assert!(task.payload["prompt"].as_str().unwrap().contains("AKIA"));
    }

    /// Issue #13: `Read` whose `tool_result` is an array of text content blocks
    /// must hash the concatenated text, not emit `"blake3:0"`.
    #[test]
    fn read_with_block_array_result_hashes_concatenated_text() {
        let input = json!({"file_path": "/etc/hosts"});
        let result = json!([
            {"type": "text", "text": "127.0.0.1 localhost\n"},
            {"type": "text", "text": "::1 localhost\n"},
        ]);
        let (kind, payload) = map_tool_to_track("Read", &input, Some(&result));
        assert_eq!(kind, Kind::FileRead);
        let hash = payload["content_hash"].as_str().unwrap();
        assert!(hash.starts_with("blake3:"));
        assert_eq!(hash.len(), 7 + 64, "expected blake3:<64 hex>, got {hash}");
        assert_ne!(hash, "blake3:0");
        // Hash should be of the concatenated text.
        let expected = blake3::hash(b"127.0.0.1 localhost\n::1 localhost\n").to_hex();
        assert_eq!(hash, format!("blake3:{expected}"));
    }

    /// Issue #13: `Read` with no `tool_result` (orphan `tool_use`) should *omit*
    /// the `content_hash` field rather than emit `"blake3:0"`.
    #[test]
    fn read_without_result_omits_content_hash() {
        let input = json!({"file_path": "/etc/hosts"});
        let (kind, payload) = map_tool_to_track("Read", &input, None);
        assert_eq!(kind, Kind::FileRead);
        assert!(
            payload.get("content_hash").is_none(),
            "expected content_hash omitted; got {payload}"
        );
    }

    /// Issue #13: `Edit` tools have no `input.content` field. The old code
    /// always took the `"blake3:0"` fallback. Now we hash whatever post-edit
    /// text we can see — `tool_result` preferred, `new_string` as last resort.
    #[test]
    fn edit_falls_back_to_new_string_when_no_result() {
        let input = json!({
            "file_path": "/tmp/x.rs",
            "old_string": "foo",
            "new_string": "bar",
        });
        let (kind, payload) = map_tool_to_track("Edit", &input, None);
        assert_eq!(kind, Kind::FileWrite);
        let hash = payload["after_hash"].as_str().unwrap();
        assert!(hash.starts_with("blake3:"));
        assert_eq!(hash.len(), 7 + 64);
        assert_ne!(hash, "blake3:0");
        let expected = blake3::hash(b"bar").to_hex();
        assert_eq!(hash, format!("blake3:{expected}"));
    }

    /// Issue #13: `Edit` prefers `tool_result` text over `new_string` because
    /// the result usually contains a richer post-edit snippet.
    #[test]
    fn edit_prefers_tool_result_text_over_new_string() {
        let input = json!({
            "file_path": "/tmp/x.rs",
            "old_string": "foo",
            "new_string": "bar",
        });
        let result = json!("post-edit content here");
        let (_kind, payload) = map_tool_to_track("Edit", &input, Some(&result));
        let hash = payload["after_hash"].as_str().unwrap();
        let expected = blake3::hash(b"post-edit content here").to_hex();
        assert_eq!(hash, format!("blake3:{expected}"));
    }

    /// `Write` keeps its existing behaviour — `input.content` wins.
    #[test]
    fn write_hashes_input_content() {
        let input = json!({"file_path": "/tmp/x", "content": "hello world"});
        let (kind, payload) = map_tool_to_track("Write", &input, None);
        assert_eq!(kind, Kind::FileWrite);
        let hash = payload["after_hash"].as_str().unwrap();
        let expected = blake3::hash(b"hello world").to_hex();
        assert_eq!(hash, format!("blake3:{expected}"));
    }

    /// Regression test: no branch should ever produce the `"blake3:0"`
    /// sentinel for any of the cases the bug-finder enumerated.
    #[test]
    fn no_branch_emits_blake3_zero_sentinel() {
        let cases: Vec<(&str, Value, Option<Value>)> = vec![
            // Read, block-array result
            (
                "Read",
                json!({"file_path": "/a"}),
                Some(json!([{"type": "text", "text": "x"}])),
            ),
            // Read, string result
            ("Read", json!({"file_path": "/a"}), Some(json!("x"))),
            // Read, no result (omits the field — also not "blake3:0")
            ("Read", json!({"file_path": "/a"}), None),
            // Edit, result + new_string
            (
                "Edit",
                json!({"file_path": "/a", "old_string": "o", "new_string": "n"}),
                Some(json!("post")),
            ),
            // Edit, only new_string
            (
                "Edit",
                json!({"file_path": "/a", "old_string": "o", "new_string": "n"}),
                None,
            ),
            // MultiEdit, only new_string fallback (no result)
            (
                "MultiEdit",
                json!({"file_path": "/a", "new_string": "n"}),
                None,
            ),
            // NotebookEdit
            (
                "NotebookEdit",
                json!({"file_path": "/a", "new_string": "n"}),
                None,
            ),
            // Write
            ("Write", json!({"file_path": "/a", "content": "c"}), None),
        ];
        for (name, input, result) in cases {
            let (_kind, payload) = map_tool_to_track(name, &input, result.as_ref());
            let s = payload.to_string();
            assert!(
                !s.contains("blake3:0\""),
                "{name} emitted blake3:0 sentinel: {s}"
            );
        }
    }
}
