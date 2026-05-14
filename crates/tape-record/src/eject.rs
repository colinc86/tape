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
use tape_format::meta::RedactionSummary;
use tape_format::meta::{Meta, Outcome, Recorder, ToolBudget};
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
    /// Pre-existing artifacts to carry through into the new tape. The deck's
    /// `tool_eject` populates this from a loaded tape's `RawTape.artifacts`
    /// so that re-ejecting (or forking + ejecting) doesn't drop the spilled
    /// bytes that the loaded tracks already reference via `{"ref": ...}`
    /// stubs. (Issue #41.) Live recordings leave this empty.
    pub inherited_artifacts: BTreeMap<String, Vec<u8>>,
    /// Caller-supplied label that lands in `meta.label`. `tape record --label`
    /// populates this; everything else passes `None`. SPEC §3.2. (Issue #72.)
    pub label: Option<String>,
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
    // 1. Snapshot.
    let mut snap = session.snapshot();

    // 2. Redaction Pass 1 — track payloads.
    //
    // Redaction MUST run before spillover. If we spilled first, oversize
    // strings would be `mem::take`n out of the payload and stored in
    // `artifacts/` before the engine ever saw them, so any secret embedded in
    // a >4 KiB value would land on disk unredacted. (Issue #11.)
    let mut redactions: Vec<Redaction> = Vec::new();
    if let Some(engine) = &opts.redact_engine {
        for t in &mut snap.tracks {
            let path = format!("$.tracks[{}].payload", t.step - 1);
            let recs = engine.redact_value(&mut t.payload, t.step, &path);
            redactions.extend(recs);
        }
    }

    // 3. Spillover — operates on already-redacted payload bytes.
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for t in &mut snap.tracks {
        spill_oversize_in_value(&mut t.payload, &mut t.refs, &mut artifacts);
    }

    // 3a. Carry inherited artifacts (from the source of a re-ejected /
    // forked tape) into the output. Track payloads can contain pre-existing
    // `{"ref": ...}` stubs that point at these bytes; without this step the
    // resulting `artifacts/` directory would be empty and the tape would
    // fail `tape verify` with MISSING_ARTIFACT. (Issue #41.)
    //
    // Content-addressed semantics: if a hash already exists locally (just
    // spilled), the inherited copy is redundant and we keep the local one.
    // Same-hash bytes are identical by definition, so the choice is moot,
    // but `or_insert_with` makes the precedence explicit.
    for (path, bytes) in &opts.inherited_artifacts {
        artifacts
            .entry(path.clone())
            .or_insert_with(|| bytes.clone());
    }

    // 4. Append eject event. The payload is a small, agent-built constant
    // (`{"outcome": ...}`) with no user-derived strings, so it bypasses the
    // redaction pass above without risk.
    //
    // Defensive: if the snapshot already ends with an eject (e.g. a forked
    // handle that included the source's terminator, or a session that took
    // an eject through `Session::append` via the recorder socket), drop it
    // so we write exactly one. (Issue #26.) SPEC §5.4 requires exactly one
    // eject, as the final event.
    if matches!(snap.tracks.last().map(|t| t.kind), Some(Kind::Eject)) {
        snap.tracks.pop();
    }
    let now = chrono::Utc::now();
    let wall_clock_ms = (now - snap.created_at).num_milliseconds().max(0) as u64;
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

    // 5. liner-notes.md (stub for now — real generation lands in step 10).
    let mut liner_md = stub_liner(&snap);

    // 6. Redaction Pass 2 — liner notes (text-only).
    if let Some(engine) = &opts.redact_engine {
        let (redacted, recs) = engine.redact_text(&liner_md, 0, "$.liner_notes");
        liner_md = redacted;
        redactions.extend(recs);
    }

    // 7. tracks.jsonl (rebuild after redaction + spillover).
    let mut tracks_jsonl = String::new();
    for t in &snap.tracks {
        let line = t.to_line()?;
        tracks_jsonl.push_str(&line);
        tracks_jsonl.push('\n');
    }

    // 8. meta.yaml.
    let id = uuid::Uuid::now_v7().to_string();
    let redaction_summary = if redactions.is_empty() {
        None
    } else {
        let mut rules_applied: Vec<String> = redactions.iter().map(|r| r.rule_id.clone()).collect();
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
        // Issue #109: SPEC §3.2's optional `tool_budget` was always `None`, which
        // silently broke `tape diff`'s Latency summary (it always reported
        // 0 ms / 0 ms / Δ0%). We now always populate the field — including when
        // call counts or token totals are zero — so consumers see honest data
        // for any real recording. `wall_clock_ms` is computed from the in-flight
        // `created_at` / eject `now` `DateTime<Utc>` values directly (no
        // RFC3339 round-trip), clamped at zero.
        tool_budget: Some(summarize_budget(&snap, wall_clock_ms)),
        redaction_summary,
        // Issue #72: `tape record --label X` used to populate the default
        // filename and nothing else. Now lands in meta.yaml as well so
        // downstream tooling can group cassettes by label.
        label: opts.label.clone(),
    };

    // 9. Redact meta.yaml itself (defense-in-depth: the task string and
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
        let (redacted_agent, recs) =
            engine.redact_text(&meta.recorder.agent, 0, "$.meta.recorder.agent");
        meta.recorder.agent = redacted_agent;
        meta_recs.extend(recs);

        // label — optional, user-provided (#77).
        if let Some(label) = meta.label.as_ref() {
            let (redacted_label, recs) = engine.redact_text(label, 0, "$.meta.label");
            meta.label = Some(redacted_label);
            meta_recs.extend(recs);
        }

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

    // 10. Defense-in-depth scan over meta.yaml + liner-notes.md + artifacts.
    //
    // Artifacts are scanned because spillover routes oversize payload values
    // around the inline-redaction path. If a future ordering regression (or
    // an artifact written by some path that doesn't go through Pass 1) leaks
    // a secret, this fail-closed check catches it. (Issue #11.)
    //
    // The scan uses the engine's own rule set — symmetric with the rules that
    // ran in Pass 1. Opt-in rules the user did NOT enable (e.g.
    // `generic_high_entropy`, `ipv4_private`) are NOT enforced here, so a
    // legitimate base64 blob or private IP doesn't trip a false-positive hard
    // failure. If no engine is configured (testing), the scan is skipped — it
    // can't enforce rules that weren't applied. (Issue #23.)
    let final_meta_yaml = meta.to_yaml()?;
    if let Some(engine) = &opts.redact_engine {
        let meta_hits = engine.scan(&final_meta_yaml);
        if !meta_hits.is_empty() {
            anyhow::bail!(
                "defense-in-depth: meta.yaml still matches configured rules: {:?}",
                meta_hits
            );
        }
        let liner_hits = engine.scan(&liner_md);
        if !liner_hits.is_empty() {
            anyhow::bail!(
                "defense-in-depth: liner-notes.md still matches configured rules: {:?}",
                liner_hits
            );
        }
        for (path, bytes) in &artifacts {
            // Many artifacts are binary; lossy decode is fine for pattern search.
            // A false-positive on random bytes is extremely unlikely for the
            // built-in rules (anchored prefixes + entropy thresholds).
            let text = String::from_utf8_lossy(bytes);
            let hits = engine.scan(&text);
            if !hits.is_empty() {
                anyhow::bail!(
                    "defense-in-depth: artifact {path} still matches configured rules: {hits:?}"
                );
            }
        }
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
        let vendor = t
            .payload
            .get("vendor")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned();
        let model = t
            .payload
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned();
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

/// Summarise the work that went into a recording. SPEC §3.2's `tool_budget`:
///
/// - `total_calls` — the count of `model_call` + `mcp_call` + `shell` tracks.
///   The `Eject` track itself is not a tool call and is excluded by the filter.
/// - `total_tokens_in` / `total_tokens_out` — sum of `payload.tokens_in` /
///   `payload.tokens_out` on `model_call` events; missing or non-integer keys
///   contribute zero (the proxy may not have extracted them for streamed
///   responses or errors). Zero totals are honest and still emitted.
/// - `wall_clock_ms` — computed by the caller from `now - snap.created_at`,
///   clamped at zero, passed in so this helper stays pure.
///
/// We always emit a `ToolBudget`, even when every field is zero (e.g. a
/// task-only recording). A non-zero `wall_clock_ms` alone is enough for
/// `tape diff`'s Latency summary to do something useful, which is the
/// motivation for issue #109.
fn summarize_budget(snap: &SessionSnapshot, wall_clock_ms: u64) -> ToolBudget {
    let total_calls = snap
        .tracks
        .iter()
        .filter(|t| matches!(t.kind, Kind::ModelCall | Kind::McpCall | Kind::Shell))
        .count() as u64;
    let total_tokens_in: u64 = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::ModelCall)
        .filter_map(|t| t.payload.get("tokens_in").and_then(Value::as_u64))
        .sum();
    let total_tokens_out: u64 = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::ModelCall)
        .filter_map(|t| t.payload.get("tokens_out").and_then(Value::as_u64))
        .sum();
    ToolBudget {
        total_calls,
        total_tokens_in,
        total_tokens_out,
        wall_clock_ms,
    }
}

fn summarize_tools(snap: &SessionSnapshot) -> Vec<tape_format::meta::ToolSummary> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<(String, Option<String>), u64> = BTreeMap::new();
    for t in &snap.tracks {
        if t.kind != Kind::McpCall {
            continue;
        }
        let server = t
            .payload
            .get("server")
            .and_then(Value::as_str)
            .map(str::to_owned);
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

/// Walk a payload value and spill any field whose JSON-serialized length
/// exceeds `PAYLOAD_INLINE_MAX`. The field is hashed, written to `artifacts`,
/// and replaced in-place with `{"ref": "sha:<hex>"}`. The ref is added to the
/// enclosing event's `refs` array.
///
/// SPEC §5.6: the top-level payload wrapper is itself not eligible — only its
/// fields are. Strings spill by their JSON-encoded length (quotes + escapes);
/// containers (Object/Array) spill wholesale when their full encoded form
/// exceeds the threshold. When a parent is spilled wholesale, its children
/// are not also spilled — the artifact captures the complete subtree.
fn spill_oversize_in_value(
    v: &mut Value,
    refs: &mut Vec<String>,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) {
    match v {
        Value::Object(map) => {
            if is_ref_stub_map(map) {
                return;
            }
            for child in map.values_mut() {
                spill_field(child, refs, artifacts);
            }
        }
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                spill_field(child, refs, artifacts);
            }
        }
        _ => {}
    }
}

/// Decide whether a single field (direct child of a payload container) should
/// be spilled wholesale or descended into to find smaller oversize fields.
fn spill_field(v: &mut Value, refs: &mut Vec<String>, artifacts: &mut BTreeMap<String, Vec<u8>>) {
    if let Value::Object(map) = v {
        if is_ref_stub_map(map) {
            return;
        }
    }
    if encoded_len(v) > PAYLOAD_INLINE_MAX {
        spill_whole(v, refs, artifacts);
    } else if matches!(v, Value::Object(_) | Value::Array(_)) {
        // The field fits but is a container — keep walking so nested oversize
        // siblings within it still get caught. (A fitting container can't have
        // an oversize descendant strictly larger than itself, but a nested
        // sibling can still exceed the threshold if it serialises with escapes
        // that the parent's other children don't share. Cheap to check.)
        spill_oversize_in_value(v, refs, artifacts);
    }
}

/// Spill a value's complete JSON-encoded bytes to artifacts and replace it
/// in-place with a ref stub. Strings spill as raw UTF-8 bytes (without the
/// surrounding JSON quotes); other values spill as their canonical JSON.
fn spill_whole(v: &mut Value, refs: &mut Vec<String>, artifacts: &mut BTreeMap<String, Vec<u8>>) {
    let bytes = match v {
        Value::String(s) => std::mem::take(s).into_bytes(),
        _ => serde_json::to_vec(v).unwrap_or_default(),
    };
    let hex = blake3_hex(&bytes);
    artifacts.insert(artifact_path(&hex), bytes);
    let ref_id = format!("sha:{hex}");
    if !refs.iter().any(|r| r == &ref_id) {
        refs.push(ref_id.clone());
    }
    *v = serde_json::json!({"ref": ref_id});
}

fn is_ref_stub_map(map: &serde_json::Map<String, Value>) -> bool {
    map.len() == 1 && map.contains_key("ref")
}

/// Length of `v` as it would appear JSON-encoded.
/// Used to enforce SPEC §5.6's spillover threshold against the encoded form.
fn encoded_len(v: &Value) -> usize {
    serde_json::to_string(v)
        .map(|s| s.len())
        .unwrap_or(usize::MAX)
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

    /// SPEC §5.6: a large array of small strings still belongs in artifacts.
    /// Regression test for the reproduction in issue #1.
    #[test]
    fn spill_misses_large_arrays() {
        let arr: Vec<serde_json::Value> = (0..500)
            .map(|i| serde_json::Value::String(format!("item-with-id-{i:04}")))
            .collect();
        let mut v = serde_json::json!({"choices": arr});
        let encoded = serde_json::to_string(&v).unwrap();
        let len = encoded.len();
        assert!(
            len > PAYLOAD_INLINE_MAX,
            "fixture is {len} bytes — exceeds threshold"
        );

        let mut refs = Vec::new();
        let mut artifacts = BTreeMap::new();
        spill_oversize_in_value(&mut v, &mut refs, &mut artifacts);

        assert_eq!(refs.len(), 1, "oversize array should have been spilled");
        assert_eq!(artifacts.len(), 1);
        assert!(v["choices"].get("ref").is_some());
    }

    /// Same shape as `spill_misses_large_arrays`, but with an oversize object
    /// of small string values rather than an array. Both shapes are normative.
    #[test]
    fn spill_catches_large_objects() {
        let mut map = serde_json::Map::new();
        for i in 0..500 {
            map.insert(
                format!("k{i:04}"),
                serde_json::Value::String(format!("v{i:04}")),
            );
        }
        let mut v = serde_json::json!({"response": serde_json::Value::Object(map)});
        let encoded = serde_json::to_string(&v).unwrap();
        assert!(encoded.len() > PAYLOAD_INLINE_MAX);

        let mut refs = Vec::new();
        let mut artifacts = BTreeMap::new();
        spill_oversize_in_value(&mut v, &mut refs, &mut artifacts);

        assert_eq!(refs.len(), 1);
        assert_eq!(artifacts.len(), 1);
        assert!(v["response"].get("ref").is_some());
    }

    /// A small fitting container next to an oversize sibling should not be
    /// spilled — only the oversize one is moved to artifacts.
    #[test]
    fn spill_preserves_small_siblings() {
        let arr: Vec<serde_json::Value> = (0..500)
            .map(|i| serde_json::Value::String(format!("item-with-id-{i:04}")))
            .collect();
        let mut v = serde_json::json!({
            "choices": arr,
            "small_list": ["a", "b", "c"],
            "scalar": 42,
            "stub": {"ref": "sha:deadbeef"},
        });

        let mut refs = Vec::new();
        let mut artifacts = BTreeMap::new();
        spill_oversize_in_value(&mut v, &mut refs, &mut artifacts);

        assert_eq!(refs.len(), 1);
        assert_eq!(artifacts.len(), 1);
        assert!(v["choices"].get("ref").is_some());
        assert_eq!(v["small_list"], serde_json::json!(["a", "b", "c"]));
        assert_eq!(v["scalar"], serde_json::json!(42));
        // Pre-existing stub left untouched.
        assert_eq!(v["stub"]["ref"], serde_json::json!("sha:deadbeef"));
    }

    /// Wholesale spillover should not also spill children of the spilled
    /// subtree (the artifact already contains them verbatim).
    #[test]
    fn spill_wholesale_does_not_double_spill_children() {
        let inner_big = "X".repeat(8_000);
        let mut v = serde_json::json!({
            "outer": {"inner": inner_big, "tag": "ok"},
        });
        let mut refs = Vec::new();
        let mut artifacts = BTreeMap::new();
        spill_oversize_in_value(&mut v, &mut refs, &mut artifacts);

        // The outer object is oversize as a whole, so it spills wholesale.
        // Only one artifact + one ref should result.
        assert_eq!(refs.len(), 1, "expected exactly one ref, got {refs:?}");
        assert_eq!(artifacts.len(), 1);
        assert!(v["outer"].get("ref").is_some());
    }
}
