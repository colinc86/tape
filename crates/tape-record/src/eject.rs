//! Eject pipeline. See SPEC §8.
//!
//! For step 5 of the build order, this is the minimum viable eject:
//! - drain the in-flight session,
//! - resolve oversized payloads → artifacts/,
//! - append the eject event,
//! - write meta.yaml + stub liner-notes.md + tracks.jsonl into a zip.
//!
//! Liner-notes generation (real) and the redaction pipeline plug in at
//! steps 9 and 10.

use std::collections::BTreeMap;

use serde_json::Value;
use tape_format::artifact::{artifact_path, blake3_hex};
use tape_format::meta::{Meta, Outcome, Recorder};
use tape_format::meta::RedactionSummary;
use tape_format::tracks::{Kind, Track};
use tape_format::writer::PendingTape;
use tape_format::PAYLOAD_INLINE_MAX;
use tape_redact::{Engine, Redaction};

use crate::session::{format_ts, Session, SessionSnapshot};

#[derive(Debug, Clone)]
pub struct EjectOptions {
    pub task: String,
    pub recorder_agent: String,
    pub outcome: Outcome,
    pub stub_liner_notes: bool,
    pub out_path: std::path::PathBuf,
    /// Redaction engine. `None` disables redaction (for testing only).
    pub redact_engine: Option<Engine>,
}

/// Final-shape result of an eject.
#[derive(Debug, Clone)]
pub struct EjectResult {
    pub path: std::path::PathBuf,
    pub track_count: u64,
    pub artifact_count: u64,
    pub redaction_count: u64,
}

pub fn eject(session: &Session, opts: &EjectOptions) -> anyhow::Result<EjectResult> {
    // 1-2. Snapshot + spillover.
    let mut snap = session.snapshot();
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for t in &mut snap.tracks {
        spill_oversize_in_value(&mut t.payload, &mut t.refs, &mut artifacts);
    }

    // 3. Append eject event.
    let now = chrono::Utc::now();
    let ejected_at = format_ts(now);
    let next_step = (snap.tracks.len() as u64) + 1;
    snap.tracks.push(Track {
        step: next_step,
        kind: Kind::Eject,
        ts: ejected_at.clone(),
        payload: serde_json::json!({"outcome": outcome_str(opts.outcome)}),
        parent_step: None,
        refs: vec![],
        annotations: vec![],
    });

    // 4. liner-notes.md (stub for now — real generation lands in step 10).
    let mut liner_md = stub_liner(&snap);

    // 5. Redaction Pass 1 — pattern apply over track payloads + meta + liner.
    let mut redactions: Vec<Redaction> = Vec::new();
    if let Some(engine) = &opts.redact_engine {
        for t in &mut snap.tracks {
            let path = format!("$.tracks[{}].payload", t.step - 1);
            let recs = engine.redact_value(&mut t.payload, t.step, &path);
            redactions.extend(recs);
        }
        // Liner notes — text redaction.
        let (redacted, recs) = engine.redact_text(&liner_md, 0, "$.liner_notes");
        liner_md = redacted;
        redactions.extend(recs);
    }

    // 6. tracks.jsonl (rebuild after redaction).
    let mut tracks_jsonl = String::new();
    for t in &snap.tracks {
        let line = t.to_line()?;
        tracks_jsonl.push_str(&line);
        tracks_jsonl.push('\n');
    }

    // 7. meta.yaml.
    let id = uuid::Uuid::now_v7().to_string();
    let redaction_summary = if redactions.is_empty() {
        None
    } else {
        let mut rules_applied: Vec<String> =
            redactions.iter().map(|r| r.rule_id.clone()).collect();
        rules_applied.sort();
        rules_applied.dedup();
        Some(RedactionSummary {
            rules_applied,
            redaction_count: redactions.len() as u64,
        })
    };

    let mut meta = Meta {
        tape_version: tape_format::TAPE_VERSION.into(),
        id,
        created_at: format_ts(snap.created_at),
        ejected_at,
        task: opts.task.clone(),
        recorder: Recorder {
            agent: opts.recorder_agent.clone(),
            user: None,
        },
        outcome: opts.outcome,
        models: summarize_models(&snap),
        tools: summarize_tools(&snap),
        tool_budget: None,
        redaction_summary,
    };

    // 8. Redact meta.yaml itself (defense-in-depth: the task string and
    // recorder.user can carry secrets too). We redact the individual fields
    // we know about (task, recorder.user) instead of redacting the whole
    // serialized YAML as text, so that a redaction insertion that contains
    // YAML-significant punctuation (e.g. `:`) can never break the document
    // structure. P2 #9.
    if let Some(engine) = &opts.redact_engine {
        let mut meta_recs: Vec<tape_redact::Redaction> = Vec::new();

        // task — always present, often user-provided.
        let (redacted_task, recs) = engine.redact_text(&meta.task, 0, "$.meta.task");
        meta.task = redacted_task;
        meta_recs.extend(recs);

        // recorder.user — optional.
        if let Some(user) = meta.recorder.user.as_ref() {
            let (redacted_user, recs) = engine.redact_text(user, 0, "$.meta.recorder.user");
            meta.recorder.user = Some(redacted_user);
            meta_recs.extend(recs);
        }

        // recorder.agent — usually our own string but redact for safety.
        let (redacted_agent, recs) = engine.redact_text(&meta.recorder.agent, 0, "$.meta.recorder.agent");
        meta.recorder.agent = redacted_agent;
        meta_recs.extend(recs);

        if !meta_recs.is_empty() {
            redactions.extend(meta_recs);
            let mut rules_applied: Vec<String> =
                redactions.iter().map(|r| r.rule_id.clone()).collect();
            rules_applied.sort();
            rules_applied.dedup();
            meta.redaction_summary = Some(RedactionSummary {
                rules_applied,
                redaction_count: redactions.len() as u64,
            });
        }
    }

    // 9. Defense-in-depth scan over meta.yaml + liner-notes.md.
    let final_meta_yaml = meta.to_yaml()?;
    let meta_hits = tape_redact::scan_for_secrets(&final_meta_yaml);
    let liner_hits = tape_redact::scan_for_secrets(&liner_md);
    if !meta_hits.is_empty() {
        anyhow::bail!(
            "defense-in-depth: meta.yaml still matches built-in rules: {:?}",
            meta_hits
        );
    }
    if !liner_hits.is_empty() {
        anyhow::bail!(
            "defense-in-depth: liner-notes.md still matches built-in rules: {:?}",
            liner_hits
        );
    }

    let redactions_json = if redactions.is_empty() {
        None
    } else {
        Some(serde_json::to_string_pretty(&redactions)?)
    };

    let redaction_count = redactions.len() as u64;

    let pending = PendingTape {
        meta_yaml: final_meta_yaml,
        liner_md,
        tracks_jsonl,
        redactions_json,
        artifacts,
    };

    if let Some(parent) = opts.out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    pending.write_to(&opts.out_path)?;

    let track_count = snap.tracks.len() as u64;
    let artifact_count = pending.artifacts.len() as u64;
    Ok(EjectResult {
        path: opts.out_path.clone(),
        track_count,
        artifact_count,
        redaction_count,
    })
}

fn outcome_str(o: Outcome) -> &'static str {
    match o {
        Outcome::Success => "success",
        Outcome::Failure => "failure",
        Outcome::Abandoned => "abandoned",
        Outcome::Unknown => "unknown",
    }
}

fn stub_liner(snap: &SessionSnapshot) -> String {
    let model_calls = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::ModelCall)
        .count();
    let mcp_calls = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::McpCall)
        .count();

    format!(
        "## What I was asked to do
{}

## What I found
This recording captured {} model calls and {} MCP calls. Liner notes were not generated by a model at eject time.

## Suggested next step / fix
Inspect the tracks via `tape ls` and `tape play` to understand the run.

## What I'm uncertain about
This is a stub. The recording agent did not produce narrative notes.
",
        snap.task, model_calls, mcp_calls
    )
}

fn summarize_models(snap: &SessionSnapshot) -> Vec<tape_format::meta::ModelSummary> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<(String, String), u64> = BTreeMap::new();
    for t in &snap.tracks {
        if t.kind != Kind::ModelCall {
            continue;
        }
        let vendor = t.payload.get("vendor").and_then(Value::as_str).unwrap_or("?").to_owned();
        let model = t.payload.get("model").and_then(Value::as_str).unwrap_or("?").to_owned();
        *map.entry((vendor, model)).or_insert(0) += 1;
    }
    map.into_iter()
        .map(|((vendor, model), calls)| tape_format::meta::ModelSummary {
            vendor,
            model,
            calls,
        })
        .collect()
}

fn summarize_tools(snap: &SessionSnapshot) -> Vec<tape_format::meta::ToolSummary> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<(String, Option<String>), u64> = BTreeMap::new();
    for t in &snap.tracks {
        if t.kind != Kind::McpCall {
            continue;
        }
        let server = t.payload.get("server").and_then(Value::as_str).map(str::to_owned);
        *map.entry(("mcp".to_owned(), server)).or_insert(0) += 1;
    }
    map.into_iter()
        .map(|((kind, server), calls)| tape_format::meta::ToolSummary {
            kind,
            server,
            tool: None,
            calls,
        })
        .collect()
}

/// Walk a JSON value; for any string field whose JSON-serialized length
/// exceeds `PAYLOAD_INLINE_MAX`, hash the bytes, place them in `artifacts`,
/// and replace the value in-place with `{"ref": "sha:<hex>"}`. Add the ref to
/// the enclosing event's `refs` array.
///
/// SPEC §5.6 measures the JSON-encoded value, which adds quotes plus
/// any required escapes. A 4096-byte raw string serializes to ≥4098 bytes
/// JSON-encoded and thus belongs in artifacts.
fn spill_oversize_in_value(
    v: &mut Value,
    refs: &mut Vec<String>,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) {
    match v {
        Value::String(s) if json_encoded_len(s) > PAYLOAD_INLINE_MAX => {
            let bytes = std::mem::take(s).into_bytes();
            let hex = blake3_hex(&bytes);
            artifacts.insert(artifact_path(&hex), bytes);
            let ref_id = format!("sha:{hex}");
            if !refs.iter().any(|r| r == &ref_id) {
                refs.push(ref_id.clone());
            }
            *v = serde_json::json!({"ref": ref_id});
        }
        Value::Object(map) => {
            // skip if it's already a ref stub
            if map.len() == 1 && map.contains_key("ref") {
                return;
            }
            for v in map.values_mut() {
                spill_oversize_in_value(v, refs, artifacts);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                spill_oversize_in_value(v, refs, artifacts);
            }
        }
        _ => {}
    }
}

/// Length of `s` as it would appear JSON-encoded (quotes + escapes).
/// Used to enforce SPEC §5.6's spillover threshold against the encoded form.
fn json_encoded_len(s: &str) -> usize {
    serde_json::to_string(s).map(|v| v.len()).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spill_replaces_large_strings() {
        let big = "Q".repeat(8_000);
        let mut v = serde_json::json!({"big": big, "small": "ok"});
        let mut refs = Vec::new();
        let mut artifacts = BTreeMap::new();
        spill_oversize_in_value(&mut v, &mut refs, &mut artifacts);
        assert_eq!(refs.len(), 1);
        assert_eq!(artifacts.len(), 1);
        // The "big" field is now a ref stub
        assert!(v["big"].get("ref").is_some());
        assert_eq!(v["small"], serde_json::json!("ok"));
    }
}
