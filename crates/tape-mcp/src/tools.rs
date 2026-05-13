//! The 12 deck tools. Each tool is `(name, description, input_schema, handler)`.
//!
//! Handlers consume `Deck` + JSON arguments, return JSON result.
//!
//! Spec contract for each tool is in the `tape-mcp-deck` skill. `tape.snapshot`
//! is the v0.1 addition for in-session recording from Claude Code's transcript
//! file (see DECISIONS.md §D2).

use serde_json::{json, Value};
use tape_format::reader::RawTape;
use tape_format::tracks::{self, Kind};
use tape_play::{label as track_label, parse_kind};

use crate::deck::{Deck, Loaded};

pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// All 12 tool definitions for `tools/list`.
pub fn definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "tape.load",
            description: "Mount a .tape file. Returns a handle plus a quick summary.",
            input_schema: json!({
                "type": "object",
                "required": ["path"],
                "properties": {"path": {"type": "string"}}
            }),
        },
        ToolDef {
            name: "tape.summary",
            description: "Returns meta + liner-notes for a loaded handle.",
            input_schema: json!({
                "type": "object",
                "required": ["handle"],
                "properties": {"handle": {"type": "string"}}
            }),
        },
        ToolDef {
            name: "tape.tracks",
            description: "Lightweight track listing. Filter by kind, range, or substring.",
            input_schema: json!({
                "type": "object",
                "required": ["handle"],
                "properties": {
                    "handle": {"type": "string"},
                    "kind": {"type": "string"},
                    "range": {"type": "array", "items": {"type": "integer"}},
                    "regex": {"type": "string"}
                }
            }),
        },
        ToolDef {
            name: "tape.play",
            description: "Full payload for one step or a range. Truncates at 200 KB.",
            input_schema: json!({
                "type": "object",
                "required": ["handle"],
                "properties": {
                    "handle": {"type": "string"},
                    "step": {"type": "integer"},
                    "range": {"type": "array", "items": {"type": "integer"}}
                }
            }),
        },
        ToolDef {
            name: "tape.seek",
            description: "Substring search across track payloads. Returns top-k hits.",
            input_schema: json!({
                "type": "object",
                "required": ["handle", "query"],
                "properties": {
                    "handle": {"type": "string"},
                    "query": {"type": "string"},
                    "k": {"type": "integer"}
                }
            }),
        },
        ToolDef {
            name: "tape.tools",
            description: "Filter to mcp_call tracks only, optionally narrowed by server/tool.",
            input_schema: json!({
                "type": "object",
                "required": ["handle"],
                "properties": {
                    "handle": {"type": "string"},
                    "server": {"type": "string"},
                    "tool": {"type": "string"}
                }
            }),
        },
        ToolDef {
            name: "tape.diff",
            description: "Compare two loaded tapes. Returns the JSON diff structure.",
            input_schema: json!({
                "type": "object",
                "required": ["a_handle", "b_handle"],
                "properties": {
                    "a_handle": {"type": "string"},
                    "b_handle": {"type": "string"},
                    "all": {"type": "boolean"}
                }
            }),
        },
        ToolDef {
            name: "tape.fork",
            description: "Branch from a step into a new in-memory handle.",
            input_schema: json!({
                "type": "object",
                "required": ["handle", "from_step"],
                "properties": {
                    "handle": {"type": "string"},
                    "from_step": {"type": "integer"},
                    "label": {"type": "string"}
                }
            }),
        },
        ToolDef {
            name: "tape.record",
            description: "Begin recording the current MCP session into a new handle.",
            input_schema: json!({
                "type": "object",
                "required": ["task"],
                "properties": {"task": {"type": "string"}}
            }),
        },
        ToolDef {
            name: "tape.annotate",
            description: "Pin an annotation to a step (or 'now' if recording).",
            input_schema: json!({
                "type": "object",
                "required": ["handle", "note"],
                "properties": {
                    "handle": {"type": "string"},
                    "step": {"type": "integer"},
                    "note": {"type": "string"},
                    "by": {"type": "string"}
                }
            }),
        },
        ToolDef {
            name: "tape.eject",
            description: "Save a handle (typically a recording or fork) to a path.",
            input_schema: json!({
                "type": "object",
                "required": ["handle", "out"],
                "properties": {
                    "handle": {"type": "string"},
                    "out": {"type": "string"}
                }
            }),
        },
        ToolDef {
            name: "tape.snapshot",
            description: "Capture this Claude Code session's transcript as a .tape file in one shot. Reads the active session JSONL from disk, converts to v0 events, runs the eject pipeline (artifact spillover + redaction). v0.1 addition.",
            input_schema: json!({
                "type": "object",
                "required": ["out"],
                "properties": {
                    "out": {"type": "string"},
                    "task": {"type": "string"},
                    "transcript_path": {"type": "string"}
                }
            }),
        },
    ]
}

// ---------- helpers ----------

fn handle_arg(args: &Value, key: &str) -> Result<String, ToolErr> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ToolErr::params(format!("missing or non-string `{key}`")))
}

fn build_summary(loaded: &Loaded) -> Value {
    let kinds = count_kinds(&loaded.tracks);
    let meta: Value =
        serde_yaml::from_str(&loaded.meta_yaml).unwrap_or(serde_json::Value::Null);
    json!({
        "meta": meta,
        "liner_notes": loaded.liner_md,
        "track_count": loaded.tracks.len(),
        "kinds": kinds,
    })
}

fn count_kinds(tracks: &[tape_format::tracks::Track]) -> Value {
    use std::collections::BTreeMap;
    let mut m: BTreeMap<&str, u64> = BTreeMap::new();
    for t in tracks {
        let k = kind_str(t.kind);
        *m.entry(k).or_insert(0) += 1;
    }
    serde_json::to_value(m).unwrap_or(Value::Null)
}

fn kind_str(k: Kind) -> &'static str {
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

#[derive(Debug, Clone)]
pub struct ToolErr {
    pub code: &'static str,
    pub message: String,
}

impl ToolErr {
    pub fn params(msg: impl Into<String>) -> Self {
        Self {
            code: "INVALID_PARAMS",
            message: msg.into(),
        }
    }
    pub fn invalid_handle() -> Self {
        Self {
            code: "INVALID_HANDLE",
            message: "no such handle in this session".into(),
        }
    }
    pub fn out_of_range(msg: impl Into<String>) -> Self {
        Self {
            code: "OUT_OF_RANGE",
            message: msg.into(),
        }
    }
}

// ---------- handlers ----------

pub fn dispatch(deck: &Deck, name: &str, args: &Value) -> Result<Value, ToolErr> {
    match name {
        "tape.load" => tool_load(deck, args),
        "tape.summary" => tool_summary(deck, args),
        "tape.tracks" => tool_tracks(deck, args),
        "tape.play" => tool_play(deck, args),
        "tape.seek" => tool_seek(deck, args),
        "tape.tools" => tool_tools(deck, args),
        "tape.diff" => tool_diff(deck, args),
        "tape.fork" => tool_fork(deck, args),
        "tape.record" => tool_record(deck, args),
        "tape.annotate" => tool_annotate(deck, args),
        "tape.eject" => tool_eject(deck, args),
        "tape.snapshot" => tool_snapshot(deck, args),
        _ => Err(ToolErr {
            code: "METHOD_NOT_FOUND",
            message: format!("unknown tool: {name}"),
        }),
    }
}

fn tool_load(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let path = handle_arg(args, "path")?;
    let raw = RawTape::open(&path).map_err(|e| ToolErr {
        code: "TAPE_NOT_FOUND",
        message: format!("{e}"),
    })?;
    let report = tape_format::verify::verify(&raw);
    if !report.is_valid() {
        let codes: Vec<&str> = report.errors().map(|d| d.code.as_str()).collect();
        return Err(ToolErr {
            code: "MALFORMED_TAPE",
            message: format!("verify failed: {codes:?}"),
        });
    }
    let tracks = tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap_or(""))
        .map_err(|e| ToolErr::params(e.to_string()))?;
    let loaded = Loaded {
        path: path.clone().into(),
        meta_yaml: raw.meta_yaml.clone().unwrap_or_default(),
        liner_md: raw.liner_md.clone().unwrap_or_default(),
        tracks,
        raw: std::sync::Arc::new(raw),
        recording: false,
    };
    let mut state = deck.state.lock().unwrap();
    let handle = state.mint_handle();
    let summary = build_summary(&loaded);
    state.put(handle.clone(), loaded);
    Ok(json!({"handle": handle, "summary": summary}))
}

fn tool_summary(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let handle = handle_arg(args, "handle")?;
    let state = deck.state.lock().unwrap();
    let loaded = state.get(&handle).ok_or_else(ToolErr::invalid_handle)?;
    Ok(build_summary(loaded))
}

fn tool_tracks(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let handle = handle_arg(args, "handle")?;
    let kind = args.get("kind").and_then(Value::as_str).map(str::to_owned);
    let range = args.get("range").and_then(|v| v.as_array()).and_then(|a| {
        if a.len() == 2 {
            Some((a[0].as_u64().unwrap_or(0), a[1].as_u64().unwrap_or(0)))
        } else {
            None
        }
    });
    let regex = args.get("regex").and_then(Value::as_str);

    let state = deck.state.lock().unwrap();
    let loaded = state.get(&handle).ok_or_else(ToolErr::invalid_handle)?;

    let kind_filter = kind.as_deref().and_then(parse_kind);
    let regex_filter = regex
        .map(regex::Regex::new)
        .transpose()
        .map_err(|e| ToolErr::params(format!("bad regex: {e}")))?;

    let mut out = Vec::new();
    for t in &loaded.tracks {
        if let Some(k) = kind_filter {
            if t.kind != k {
                continue;
            }
        }
        if let Some((lo, hi)) = range {
            if t.step < lo || t.step > hi {
                continue;
            }
        }
        let lbl = track_label(t);
        if let Some(re) = &regex_filter {
            if !re.is_match(&lbl) {
                continue;
            }
        }
        out.push(json!({
            "step": t.step,
            "kind": kind_str(t.kind),
            "ts": t.ts,
            "label": lbl,
        }));
    }
    Ok(json!({"tracks": out}))
}

fn tool_play(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let handle = handle_arg(args, "handle")?;
    let step = args.get("step").and_then(Value::as_u64);
    let range = args.get("range").and_then(|v| v.as_array()).and_then(|a| {
        if a.len() == 2 {
            Some((a[0].as_u64().unwrap_or(0), a[1].as_u64().unwrap_or(0)))
        } else {
            None
        }
    });

    let state = deck.state.lock().unwrap();
    let loaded = state.get(&handle).ok_or_else(ToolErr::invalid_handle)?;

    let mut out = Vec::new();
    let mut total_bytes = 0usize;
    const CAP: usize = 200 * 1024;

    for t in &loaded.tracks {
        let include = match (step, range) {
            (Some(s), _) => t.step == s,
            (None, Some((lo, hi))) => t.step >= lo && t.step <= hi,
            (None, None) => false,
        };
        if !include {
            continue;
        }
        // Resolve any artifact refs to full bytes if present.
        let track_value = serde_json::to_value(t).unwrap_or(Value::Null);
        let serialized = track_value.to_string();
        if total_bytes + serialized.len() > CAP {
            return Err(ToolErr {
                code: "OUT_OF_RANGE",
                message: format!("response exceeds 200 KB cap; narrow the range"),
            });
        }
        total_bytes += serialized.len();
        out.push(track_value);
    }

    if step.is_some() && out.is_empty() {
        return Err(ToolErr {
            code: "INVALID_STEP",
            message: format!("step {} not found", step.unwrap()),
        });
    }
    if step.is_none() && range.is_none() {
        return Err(ToolErr::params("must supply `step` or `range`"));
    }

    Ok(json!({"tracks": out}))
}

fn tool_seek(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let handle = handle_arg(args, "handle")?;
    let query = handle_arg(args, "query")?;
    let k = args
        .get("k")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .max(1) as usize;

    let state = deck.state.lock().unwrap();
    let loaded = state.get(&handle).ok_or_else(ToolErr::invalid_handle)?;
    let q_lower = query.to_lowercase();

    let mut hits: Vec<Value> = Vec::new();
    for t in &loaded.tracks {
        let lbl = track_label(t);
        let payload_str = t.payload.to_string().to_lowercase();
        let label_lower = lbl.to_lowercase();
        let in_label = label_lower.contains(&q_lower);
        let in_payload = payload_str.contains(&q_lower);
        if !(in_label || in_payload) {
            continue;
        }
        let snippet = if in_label {
            lbl.clone()
        } else {
            payload_snippet(&t.payload.to_string(), &q_lower)
        };
        hits.push(json!({
            "step": t.step,
            "kind": kind_str(t.kind),
            "score": if in_label { 1.0 } else { 0.5 },
            "snippet": snippet,
        }));
        if hits.len() >= k {
            break;
        }
    }
    Ok(json!({"hits": hits}))
}

/// Pull an ~80-byte window of `s` around the first case-insensitive match of
/// `q_lower` (which the caller has already lowercased). Both endpoints are
/// nudged outward to the nearest UTF-8 char boundary so the returned slice
/// never bisects a multi-byte character — slicing on a non-boundary byte
/// would panic and take down the deck.
fn payload_snippet(s: &str, q_lower: &str) -> String {
    let lo = s.to_lowercase().find(q_lower).unwrap_or(0);
    let mut start = lo.saturating_sub(40);
    let mut end = (lo + q_lower.len() + 40).min(s.len());
    while start > 0 && !s.is_char_boundary(start) {
        start -= 1;
    }
    while end < s.len() && !s.is_char_boundary(end) {
        end += 1;
    }
    s[start..end].to_string()
}

fn tool_tools(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let handle = handle_arg(args, "handle")?;
    let server = args.get("server").and_then(Value::as_str);
    let tool = args.get("tool").and_then(Value::as_str);

    let state = deck.state.lock().unwrap();
    let loaded = state.get(&handle).ok_or_else(ToolErr::invalid_handle)?;

    let mut out = Vec::new();
    for t in &loaded.tracks {
        if t.kind != Kind::McpCall {
            continue;
        }
        if let Some(s) = server {
            if t.payload.get("server").and_then(Value::as_str) != Some(s) {
                continue;
            }
        }
        if let Some(tn) = tool {
            if t.payload.get("tool").and_then(Value::as_str) != Some(tn) {
                continue;
            }
        }
        out.push(serde_json::to_value(t).unwrap_or(Value::Null));
    }
    Ok(json!({"calls": out}))
}

fn tool_diff(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let a_handle = handle_arg(args, "a_handle")?;
    let b_handle = handle_arg(args, "b_handle")?;
    let state = deck.state.lock().unwrap();
    let a = state.get(&a_handle).ok_or_else(ToolErr::invalid_handle)?;
    let b = state.get(&b_handle).ok_or_else(ToolErr::invalid_handle)?;
    // Path-based compute requires .tape on disk. Both loaded handles point
    // at on-disk paths (tape.load mounts a file). For in-memory recordings,
    // diff is not yet supported — return an error.
    if a.path.as_os_str().is_empty() || b.path.as_os_str().is_empty() {
        return Err(ToolErr {
            code: "INVALID_PARAMS",
            message: "tape.diff requires both handles to be on-disk loads".into(),
        });
    }
    let diff = tape_diff::compute(&a.path, &b.path).map_err(|e| ToolErr {
        code: "INTERNAL_ERROR",
        message: e.to_string(),
    })?;
    serde_json::to_value(&diff).map(|v| json!({"diff": v})).map_err(|e| {
        ToolErr {
            code: "INTERNAL_ERROR",
            message: e.to_string(),
        }
    })
}

fn tool_fork(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let handle = handle_arg(args, "handle")?;
    let from_step = args
        .get("from_step")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolErr::params("missing `from_step`"))?;

    let mut state = deck.state.lock().unwrap();
    let source = state
        .get(&handle)
        .ok_or_else(ToolErr::invalid_handle)?
        .clone();
    if from_step == 0 || from_step as usize > source.tracks.len() {
        return Err(ToolErr::out_of_range(format!(
            "from_step {} out of [1, {}]",
            from_step,
            source.tracks.len()
        )));
    }
    let new_handle = state.mint_handle();
    let mut forked = source.clone();
    forked.tracks.truncate(from_step as usize);
    forked.recording = false;
    state.put(new_handle.clone(), forked);
    Ok(json!({"new_handle": new_handle}))
}

fn tool_record(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let task = handle_arg(args, "task")?;
    let mut state = deck.state.lock().unwrap();
    // P1 #3: enforce the deck contract — refuse if any handle is already
    // recording in this MCP session.
    if state.any_recording() {
        return Err(ToolErr {
            code: "ALREADY_RECORDING",
            message: "this session already has an active recording; eject it first".into(),
        });
    }
    let new_handle = state.mint_handle();
    let now_ts = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let task_event = tape_format::tracks::Track {
        step: 1,
        kind: Kind::Task,
        ts: now_ts,
        payload: json!({"prompt": task}),
        parent_step: None,
        refs: vec![],
        annotations: vec![],
    };
    let loaded = Loaded {
        path: std::path::PathBuf::new(),
        meta_yaml: format!(
            "tape_version: \"tape/v0\"\nid: \"{}\"\ncreated_at: \"{}\"\nejected_at: \"{}\"\ntask: {:?}\nrecorder:\n  agent: \"tape-mcp/{}\"\noutcome: unknown\n",
            uuid::Uuid::now_v7(),
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            task,
            env!("CARGO_PKG_VERSION"),
        ),
        liner_md: String::new(),
        tracks: vec![task_event],
        raw: std::sync::Arc::new(RawTape {
            meta_yaml: None,
            liner_md: None,
            tracks_jsonl: None,
            redactions_json: None,
            artifacts: Default::default(),
            unknown_entries: Vec::new(),
        }),
        recording: true,
    };
    state.put(new_handle.clone(), loaded);
    Ok(json!({"handle": new_handle, "recording": true}))
}

fn tool_annotate(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let handle = handle_arg(args, "handle")?;
    let note = handle_arg(args, "note")?;
    let by = args.get("by").and_then(Value::as_str).unwrap_or("agent").to_owned();
    let step_arg = args.get("step").and_then(Value::as_u64);

    let mut state = deck.state.lock().unwrap();
    let loaded = state.get_mut(&handle).ok_or_else(ToolErr::invalid_handle)?;

    let next_step = (loaded.tracks.len() as u64) + 1;
    // SPEC §5.3 — `parent_step`, when present, MUST be in [1, step). The new
    // annotation event has `step == next_step`, so `step_arg` (which becomes
    // its `parent_step`) must satisfy `1 <= step_arg < next_step`. Reject
    // early so the deck never produces a non-conforming tape. (Issue #3.)
    if let Some(s) = step_arg {
        if s == 0 || s >= next_step {
            return Err(ToolErr::params(format!(
                "step must be in [1, {next_step}); got {s}"
            )));
        }
    }
    let new_track = tape_format::tracks::Track {
        step: next_step,
        kind: Kind::Annotation,
        ts: chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
        payload: json!({"by": by, "note": note}),
        parent_step: step_arg,
        refs: vec![],
        annotations: vec![],
    };
    loaded.tracks.push(new_track);
    Ok(json!({"step": next_step}))
}

fn tool_eject(deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    let handle = handle_arg(args, "handle")?;
    let out = handle_arg(args, "out")?;

    let mut state = deck.state.lock().unwrap();
    let loaded = state.get(&handle).ok_or_else(ToolErr::invalid_handle)?.clone();
    // Mark the handle as no longer recording so a future tape.record in this
    // session is allowed (otherwise ALREADY_RECORDING fires forever).
    if let Some(l) = state.get_mut(&handle) {
        l.recording = false;
    }
    drop(state);

    if !loaded.recording && loaded.tracks.is_empty() {
        return Err(ToolErr {
            code: "NOT_RECORDING",
            message: "handle is not in a recordable state".into(),
        });
    }

    // Build a session-shape struct in-memory and reuse the eject pipeline.
    // We do this by constructing a Session and replaying tracks into it.
    let session = tape_record::session::Session::start(
        &extract_task(&loaded),
        format!("tape-mcp/{}", env!("CARGO_PKG_VERSION")),
    );
    // Skip the auto-injected step 1 (task) and replay the rest.
    for t in loaded.tracks.iter().skip(1) {
        session.append(t.kind, t.payload.clone());
    }

    // Issue #17: load `.taperc` from the current workspace so custom rules,
    // enable_optional, and disable_default take effect on MCP-driven ejects.
    let cwd = std::env::current_dir().map_err(|e| ToolErr {
        code: "INTERNAL_ERROR",
        message: format!("cwd: {e}"),
    })?;
    let redact_engine = tape_redact::engine_with_taperc(&cwd).map_err(|e| ToolErr {
        code: "TAPERC_INVALID",
        message: format!("failed to load .taperc: {e}"),
    })?;
    let result = tape_record::eject::eject(
        &session,
        &tape_record::eject::EjectOptions {
            task: extract_task(&loaded),
            recorder_agent: format!("tape-mcp/{}", env!("CARGO_PKG_VERSION")),
            outcome: tape_format::meta::Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone().into(),
            redact_engine: Some(redact_engine),
        },
    )
    .map_err(|e| ToolErr {
        code: "EJECT_FAILED",
        message: e.to_string(),
    })?;

    Ok(json!({
        "path": result.path,
        "redactions": result.redaction_count
    }))
}

fn extract_task(loaded: &Loaded) -> String {
    loaded
        .tracks
        .first()
        .filter(|t| t.kind == Kind::Task)
        .and_then(|t| t.payload.get("prompt").and_then(Value::as_str))
        .unwrap_or("")
        .to_owned()
}

/// Reduce a multi-line / over-long task prompt to a single one-liner suitable
/// for `meta.task` (per SPEC §3.1). Takes the first non-empty line; clamps to
/// 200 chars with an ellipsis.
fn one_line_summary(raw: &str) -> String {
    const MAX_LEN: usize = 200;
    let first_line = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(raw.trim());
    if first_line.chars().count() <= MAX_LEN {
        first_line.to_string()
    } else {
        let truncated: String = first_line.chars().take(MAX_LEN - 1).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod task_summary_tests {
    use super::one_line_summary;

    #[test]
    fn one_line_takes_first_nonempty_line() {
        assert_eq!(one_line_summary("\n\n  hello world\nmore"), "hello world");
    }

    #[test]
    fn one_line_truncates_long_prompts() {
        let long: String = "x".repeat(500);
        let s = one_line_summary(&long);
        assert!(s.chars().count() <= 200);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn one_line_passes_through_short_input() {
        assert_eq!(one_line_summary("short prompt"), "short prompt");
    }
}

#[cfg(test)]
mod payload_snippet_tests {
    use super::payload_snippet;

    /// Regression for issue #7 — the original byte-index slice panicked at
    /// `byte index 30 is not a char boundary`.
    #[test]
    fn does_not_panic_on_multibyte_payload() {
        let s = format!("{}{}{}", "a".repeat(10), "日本".repeat(10), "b".repeat(10));
        let out = payload_snippet(&s, "b");
        assert!(out.is_char_boundary(0) && out.is_char_boundary(out.len()));
        assert!(out.contains('b'));
    }

    #[test]
    fn keeps_emoji_intact() {
        // A 40-byte window on either side of "match" reaches past the emoji,
        // so we expect the snippet to include it whole. The act of returning
        // a String already proves the slice landed on char boundaries (Rust
        // would have panicked otherwise).
        let s = "foo 🎯 bar match here baz";
        let out = payload_snippet(s, "match");
        assert!(out.contains("match"));
        assert!(out.contains('🎯'));
    }

    #[test]
    fn boundary_walk_never_panics_on_short_input() {
        // Short input where the window extends past both ends.
        let out = payload_snippet("é", "é");
        assert_eq!(out, "é");
    }

    #[test]
    fn handles_query_at_string_end() {
        let s = "padding 日本語 match";
        let out = payload_snippet(s, "match");
        assert!(out.ends_with("match"));
    }

    #[test]
    fn empty_match_does_not_panic() {
        // `find("")` returns Some(0); make sure that path is also safe.
        let s = "日本語日本語";
        let _ = payload_snippet(s, "");
    }
}

/// `tape.snapshot` — read Claude Code's active session transcript and produce
/// a `.tape` file. See DECISIONS.md §D2.
fn tool_snapshot(_deck: &Deck, args: &Value) -> Result<Value, ToolErr> {
    use std::io::BufReader;
    use tape_record::transcript::{find_active_session, parse_jsonl, to_tracks};

    let out = handle_arg(args, "out")?;
    let task = args
        .get("task")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let explicit_path = args
        .get("transcript_path")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from);

    let handle = if let Some(path) = explicit_path {
        // Caller supplied an explicit path — used by tests and advanced flows.
        let session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let sibling_dir = path
            .parent()
            .map(|p| p.join(&session_id))
            .unwrap_or_else(|| std::path::PathBuf::from(&session_id));
        tape_record::transcript::TranscriptHandle {
            jsonl_path: path,
            session_id,
            sibling_dir,
        }
    } else {
        let cwd = std::env::current_dir().map_err(|e| ToolErr {
            code: "INTERNAL_ERROR",
            message: format!("cwd: {e}"),
        })?;
        find_active_session(&cwd).map_err(|e| ToolErr {
            code: "TAPE_NOT_FOUND",
            message: format!("no active Claude Code session transcript: {e}"),
        })?
    };

    let file = std::fs::File::open(&handle.jsonl_path).map_err(|e| ToolErr {
        code: "INTERNAL_ERROR",
        message: format!("open transcript {}: {e}", handle.jsonl_path.display()),
    })?;
    let (entries, parse_report) =
        parse_jsonl(BufReader::new(file)).map_err(|e| ToolErr {
            code: "INTERNAL_ERROR",
            message: format!("parse transcript: {e}"),
        })?;

    let (tracks, convert_report) =
        to_tracks(&entries, &handle.sibling_dir, parse_report);

    // Derive task: explicit arg wins; else first user prompt; else session-id.
    let raw_task_text = task
        .or_else(|| {
            tracks
                .first()
                .filter(|t| t.kind == Kind::Task)
                .and_then(|t| t.payload.get("prompt").and_then(Value::as_str))
                .map(String::from)
        })
        .unwrap_or_else(|| format!("session {}", handle.session_id));

    // P3 #16: meta.task is one line. Truncate to first newline, then to ≤200
    // chars so a giant first prompt doesn't blow up meta.yaml.
    let task_text = one_line_summary(&raw_task_text);

    // P3 #15: align Session::created_at with the transcript's first event
    // timestamp instead of "now". If we can't parse the first ts, fall back
    // to current time.
    let started_at = tracks
        .first()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t.ts).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    // Replay tracks into a fresh Session, then call the existing eject pipeline.
    let recorder_agent = format!("tape-mcp/{}+transcript", env!("CARGO_PKG_VERSION"));
    let session =
        tape_record::session::Session::start_at(&task_text, recorder_agent.clone(), started_at);
    // Convert produces a Task event as track 1; the session's start_at
    // injection already placed a task at step 1. Skip the converted Task
    // and append the rest so we don't duplicate.
    //
    // Each converted track already carries the timestamp of when that event
    // really happened (per the transcript JSONL). Use `append_at` to preserve
    // it; calling `append` here would replace every per-event ts with "now"
    // and collapse the entire conversation into a single instant. (Issue #5.)
    let skip_first = tracks.first().is_some_and(|t| t.kind == Kind::Task);
    let to_replay: &[_] = if skip_first { &tracks[1..] } else { &tracks[..] };
    for t in to_replay {
        let ts = chrono::DateTime::parse_from_rfc3339(&t.ts)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        session.append_at(t.kind, t.payload.clone(), ts);
    }

    let out_path = std::path::PathBuf::from(&out);
    // Issue #17: load `.taperc` so a user's custom rules + enable/disable
    // settings apply to snapshots, not just default built-ins.
    let snapshot_cwd = std::env::current_dir().map_err(|e| ToolErr {
        code: "INTERNAL_ERROR",
        message: format!("cwd: {e}"),
    })?;
    let redact_engine =
        tape_redact::engine_with_taperc(&snapshot_cwd).map_err(|e| ToolErr {
            code: "TAPERC_INVALID",
            message: format!("failed to load .taperc: {e}"),
        })?;
    let result = tape_record::eject::eject(
        &session,
        &tape_record::eject::EjectOptions {
            task: task_text,
            recorder_agent,
            outcome: tape_format::meta::Outcome::Unknown,
            stub_liner_notes: true,
            out_path: out_path.clone(),
            redact_engine: Some(redact_engine),
        },
    )
    .map_err(|e| ToolErr {
        code: "EJECT_FAILED",
        message: e.to_string(),
    })?;

    Ok(json!({
        "path": result.path,
        "track_count": result.track_count,
        "redactions": result.redaction_count,
        "parse_warnings": {
            "unknown_event_types": convert_report.parse.skipped,
            "malformed_lines": convert_report.parse.malformed_lines,
            "orphan_tool_calls": convert_report.orphan_tool_calls,
            "inline_results_used": convert_report.inline_results_used,
            "sibling_results_used": convert_report.sibling_results_used,
        },
        "transcript_path": handle.jsonl_path,
    }))
}
