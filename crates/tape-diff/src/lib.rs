//! `tape diff` — align two tapes, classify each pair, optionally narrate.
//!
//! v0 is intentionally light on the alignment side: we use `similar`'s LCS
//! over per-track step-intent labels rather than Needleman-Wunsch with
//! embedding similarity. This gives correct results when steps are
//! identical-or-similar and avoids dragging in an embedding backend.
//! Embedding-based alignment is a v0.1 upgrade — see DECISIONS.md.
//!
//! Public surface:
//!   - [`Diff`] — the structured diff result.
//!   - [`align`] — produces a list of paired steps.
//!   - [`classify_pair`] — assigns a class to one aligned pair.
//!   - [`compute`] — top-level: load + align + classify, no narration.
//!   - [`render_text`] / [`render_json`] — output formatters.
//!   - [`narrate`] — `--judge`-mode narration via [`tape_judge::JudgeClient`].

pub mod narrate;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tape_format::reader::RawTape;
use tape_format::tracks::{self, Kind, Track};
use tape_judge::JudgeCallRecord;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diff {
    pub task: String,
    pub outcome: Outcomes,
    pub alignment: Vec<AlignedPair>,
    pub summary: Summary,
    /// Audit trail of every successful judge call made while building
    /// this diff. Empty in the no-`--judge` path. Serialized into the
    /// `--format json` output so a user who redirects to a file gets
    /// the audit rows alongside the structural diff (cassettes
    /// themselves stay immutable).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub judge_calls: Vec<JudgeCallRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Outcomes {
    pub a: String,
    pub b: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlignedPair {
    pub a_step: Option<u64>,
    pub b_step: Option<u64>,
    pub class: Class,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narration: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub downstream_b: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b_label: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Identical,
    Cosmetic,
    Substantive,
    Causal,
    Inserted,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Summary {
    pub answers_equivalent: bool,
    pub tool_budget: Budget,
    pub latency_ms: Latency,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Budget {
    pub a: u64,
    pub b: u64,
    /// Percent change from a to b. `None` when undefined (a is zero, b nonzero).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_pct: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Latency {
    pub a: u64,
    pub b: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_pct: Option<i64>,
}

/// Compute a diff from two .tape file paths.
pub fn compute(a_path: &std::path::Path, b_path: &std::path::Path) -> anyhow::Result<Diff> {
    let a_raw = RawTape::open(a_path)?;
    let b_raw = RawTape::open(b_path)?;

    let a_meta: tape_format::meta::Meta =
        serde_yaml::from_str(a_raw.meta_yaml.as_deref().unwrap_or(""))?;
    let b_meta: tape_format::meta::Meta =
        serde_yaml::from_str(b_raw.meta_yaml.as_deref().unwrap_or(""))?;
    let a_tracks = tracks::parse_jsonl(a_raw.tracks_jsonl.as_deref().unwrap_or(""))?;
    let b_tracks = tracks::parse_jsonl(b_raw.tracks_jsonl.as_deref().unwrap_or(""))?;

    let pairs = align(&a_tracks, &b_tracks);
    let mut alignment: Vec<AlignedPair> = pairs
        .into_iter()
        .map(|(a, b)| classify_pair(&a_tracks, &b_tracks, a, b))
        .collect();

    // Compute summary numbers.
    let a_calls = a_tracks
        .iter()
        .filter(|t| matches!(t.kind, Kind::ModelCall | Kind::McpCall | Kind::Shell))
        .count() as u64;
    let b_calls = b_tracks
        .iter()
        .filter(|t| matches!(t.kind, Kind::ModelCall | Kind::McpCall | Kind::Shell))
        .count() as u64;
    let a_lat = a_meta.tool_budget.map(|b| b.wall_clock_ms).unwrap_or(0);
    let b_lat = b_meta.tool_budget.map(|b| b.wall_clock_ms).unwrap_or(0);

    // Final answers equivalence — extract last model_call response or last annotation.
    let a_answer = last_answer(&a_tracks);
    let b_answer = last_answer(&b_tracks);
    let answers_equivalent = a_answer.is_some() && a_answer == b_answer;

    let summary = Summary {
        answers_equivalent,
        tool_budget: Budget {
            a: a_calls,
            b: b_calls,
            delta_pct: pct_delta(a_calls, b_calls),
        },
        latency_ms: Latency {
            a: a_lat,
            b: b_lat,
            delta_pct: pct_delta(a_lat, b_lat),
        },
    };

    // Decorate pairs with labels for nicer text output.
    for pair in &mut alignment {
        pair.a_label = pair
            .a_step
            .and_then(|s| a_tracks.iter().find(|t| t.step == s))
            .map(tape_play::label);
        pair.b_label = pair
            .b_step
            .and_then(|s| b_tracks.iter().find(|t| t.step == s))
            .map(tape_play::label);
    }

    Ok(Diff {
        task: a_meta.task,
        outcome: Outcomes {
            a: format!("{:?}", a_meta.outcome).to_lowercase(),
            b: format!("{:?}", b_meta.outcome).to_lowercase(),
        },
        alignment,
        summary,
        judge_calls: Vec::new(),
    })
}

/// Align two track lists by their step-intent labels using LCS.
pub fn align(a: &[Track], b: &[Track]) -> Vec<(Option<u64>, Option<u64>)> {
    use similar::{ChangeTag, TextDiff};

    let a_labels: Vec<String> = a.iter().map(intent).collect();
    let b_labels: Vec<String> = b.iter().map(intent).collect();
    let a_text = a_labels.join("\n");
    let b_text = b_labels.join("\n");
    let diff = TextDiff::from_lines(&a_text, &b_text);

    let mut a_iter = a.iter().peekable();
    let mut b_iter = b.iter().peekable();
    let mut pairs = Vec::new();

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                let a_step = a_iter.next().map(|t| t.step);
                let b_step = b_iter.next().map(|t| t.step);
                pairs.push((a_step, b_step));
            }
            ChangeTag::Delete => {
                let a_step = a_iter.next().map(|t| t.step);
                pairs.push((a_step, None));
            }
            ChangeTag::Insert => {
                let b_step = b_iter.next().map(|t| t.step);
                pairs.push((None, b_step));
            }
        }
    }
    pairs
}

/// Step-intent label used by the aligner. Stable, deterministic, and short.
pub fn intent(t: &Track) -> String {
    format!("{}::{}", kind_str(t.kind), tape_play::label(t))
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

/// Classify one aligned pair.
pub fn classify_pair(
    a_tracks: &[Track],
    b_tracks: &[Track],
    a_step: Option<u64>,
    b_step: Option<u64>,
) -> AlignedPair {
    let class = match (a_step, b_step) {
        (None, None) => Class::Identical, // unreachable in practice
        (Some(_), None) => Class::Deleted,
        (None, Some(_)) => Class::Inserted,
        (Some(a), Some(b)) => {
            let at = a_tracks
                .iter()
                .find(|t| t.step == a)
                .expect("a step exists");
            let bt = b_tracks
                .iter()
                .find(|t| t.step == b)
                .expect("b step exists");
            classify_present(at, bt)
        }
    };

    AlignedPair {
        a_step,
        b_step,
        class,
        narration: None,
        downstream_b: Vec::new(),
        a_label: None,
        b_label: None,
    }
}

fn classify_present(a: &Track, b: &Track) -> Class {
    if a.kind != b.kind {
        return Class::Substantive;
    }
    let na = normalize(&a.payload);
    let nb = normalize(&b.payload);
    if na == nb {
        Class::Identical
    } else {
        // Cheap cosmetic check: if the payloads are equal modulo whitespace
        // collapsing, call it cosmetic.
        if collapse_ws(&na.to_string()) == collapse_ws(&nb.to_string()) {
            Class::Cosmetic
        } else {
            Class::Substantive
        }
    }
}

/// Normalize a payload by stripping volatile fields (timestamps, ids, durations).
fn normalize(v: &Value) -> Value {
    let mut out = v.clone();
    strip_volatile(&mut out);
    out
}

fn strip_volatile(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for k in [
                "ts",
                "id",
                "request_id",
                "duration_ms",
                "wall_clock_ms",
                "stream_chunks",
                "tokens_in",
                "tokens_out",
                "created_at",
                "ejected_at",
            ] {
                map.remove(k);
            }
            for v in map.values_mut() {
                strip_volatile(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_volatile(v);
            }
        }
        _ => {}
    }
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Percent change from a to b.
/// - Returns `Some(0)` when both are 0 (no change).
/// - Returns `None` when a is 0 and b is non-zero (undefined ratio).
/// - Otherwise returns the integer percent delta.
fn pct_delta(a: u64, b: u64) -> Option<i64> {
    match (a, b) {
        (0, 0) => Some(0),
        (0, _) => None,
        _ => Some(((b as i64 - a as i64) * 100) / (a as i64)),
    }
}

fn last_answer(tracks: &[Track]) -> Option<String> {
    // Prefer the last annotation whose `by` field is exactly "agent" as the
    // canonical answer. Parser warnings (`by: "tape:transcript-parser"`) and
    // user notes (`by: "user"`) are explicitly skipped — they are not answers.
    // If no agent annotation exists, fall through to the last model_call.
    let agent_note = tracks.iter().rev().find(|t| {
        t.kind == Kind::Annotation && t.payload.get("by").and_then(Value::as_str) == Some("agent")
    });
    if let Some(t) = agent_note {
        return t
            .payload
            .get("note")
            .and_then(Value::as_str)
            .map(String::from);
    }
    // Else, last model_call response text.
    tracks
        .iter()
        .rev()
        .find(|t| t.kind == Kind::ModelCall)
        .and_then(|t| {
            let resp = t.payload.get("response")?;
            // Anthropic-shape: response.content[0].text
            if let Some(text) = resp
                .get("content")
                .and_then(|v| v.get(0))
                .and_then(|v| v.get("text"))
                .and_then(Value::as_str)
            {
                return Some(text.to_string());
            }
            // OpenAI-shape: response.choices[0].message.content
            if let Some(text) = resp
                .get("choices")
                .and_then(|v| v.get(0))
                .and_then(|v| v.get("message"))
                .and_then(|v| v.get("content"))
                .and_then(Value::as_str)
            {
                return Some(text.to_string());
            }
            None
        })
}

/// Render a Diff in human-readable text.
pub fn render_text(diff: &Diff, show_all: bool) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "Task:    {:?}", diff.task);
    let _ = writeln!(out, "Outcome: {} vs {}", diff.outcome.a, diff.outcome.b);
    let _ = writeln!(out);

    for pair in &diff.alignment {
        if !show_all && pair.class == Class::Identical {
            continue;
        }
        let class = class_str(pair.class);
        let label_a = pair.a_label.as_deref().unwrap_or("(missing)");
        let label_b = pair.b_label.as_deref().unwrap_or("(missing)");
        let step_marker = match (pair.a_step, pair.b_step) {
            (Some(a), Some(b)) if a == b => format!("Track {a}"),
            (Some(a), Some(b)) => format!("Track {a}/{b}"),
            (Some(a), None) => format!("Track {a} (A)"),
            (None, Some(b)) => format!("Track {b} (B)"),
            (None, None) => "Track ?".to_string(),
        };
        let _ = writeln!(out, "▸ {step_marker:<14} {class:<11} · {label_a}");
        if pair.a_step.is_some() && pair.b_step.is_some() && label_a != label_b {
            let _ = writeln!(out, "    before: {label_a}");
            let _ = writeln!(out, "    after:  {label_b}");
        }
        if let Some(narr) = &pair.narration {
            // AC1: stable `judge:` marker, 2-space indent. The narration
            // may be multi-line prose from the model; subsequent lines
            // are continued under the same indent so the marker stays
            // visually distinct from structural fields above.
            let mut lines = narr.lines();
            if let Some(first) = lines.next() {
                let _ = writeln!(out, "  judge: {first}");
                for line in lines {
                    let _ = writeln!(out, "         {line}");
                }
            }
        }
        if !pair.downstream_b.is_empty() {
            let _ = writeln!(out, "    impact: flows into Track {:?}", pair.downstream_b);
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Final answers: {}",
        if diff.summary.answers_equivalent {
            "semantically equivalent"
        } else {
            "divergent"
        }
    );
    let _ = writeln!(
        out,
        "Tool budget:   before {} calls · after {} calls ({})",
        diff.summary.tool_budget.a,
        diff.summary.tool_budget.b,
        format_delta(diff.summary.tool_budget.delta_pct)
    );
    let _ = writeln!(
        out,
        "Latency:       before {} ms · after {} ms ({})",
        diff.summary.latency_ms.a,
        diff.summary.latency_ms.b,
        format_delta(diff.summary.latency_ms.delta_pct)
    );
    out
}

pub fn render_json(diff: &Diff) -> String {
    serde_json::to_string_pretty(diff).unwrap_or_else(|_| "{}".to_string())
}

fn format_delta(d: Option<i64>) -> String {
    match d {
        Some(0) => "Δ0%".to_string(),
        Some(n) => format!("Δ{n}%"),
        None => "Δ n/a".to_string(),
    }
}

fn class_str(c: Class) -> &'static str {
    match c {
        Class::Identical => "identical",
        Class::Cosmetic => "cosmetic",
        Class::Substantive => "substantive",
        Class::Causal => "causal",
        Class::Inserted => "inserted",
        Class::Deleted => "deleted",
    }
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
    fn aligned_identical_steps_classify_identical() {
        let a = t(1, Kind::Task, json!({"prompt": "x"}));
        let b = t(1, Kind::Task, json!({"prompt": "x"}));
        let p = classify_pair(&[a.clone()], &[b.clone()], Some(1), Some(1));
        assert_eq!(p.class, Class::Identical);
    }

    #[test]
    fn whitespace_only_difference_is_cosmetic() {
        let a = t(1, Kind::Task, json!({"prompt": "hello world"}));
        let b = t(1, Kind::Task, json!({"prompt": "hello   world"}));
        let p = classify_pair(&[a.clone()], &[b.clone()], Some(1), Some(1));
        assert_eq!(p.class, Class::Cosmetic);
    }

    #[test]
    fn semantic_difference_is_substantive() {
        let a = t(1, Kind::Task, json!({"prompt": "investigate"}));
        let b = t(1, Kind::Task, json!({"prompt": "ignore"}));
        let p = classify_pair(&[a.clone()], &[b.clone()], Some(1), Some(1));
        assert_eq!(p.class, Class::Substantive);
    }

    #[test]
    fn align_inserts_and_deletes_correctly() {
        let a = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Eject, json!({"outcome": "success"})),
        ];
        let b = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Annotation, json!({"by": "agent", "note": "extra"})),
            t(3, Kind::Eject, json!({"outcome": "success"})),
        ];
        let pairs = align(&a, &b);
        // Expected: (1,1) Equal, (None, 2) Insert, (2, 3) Equal
        assert!(pairs.contains(&(Some(1), Some(1))));
        assert!(pairs.contains(&(None, Some(2))));
        assert!(pairs.contains(&(Some(2), Some(3))));
    }

    #[test]
    fn pct_delta_basic() {
        assert_eq!(pct_delta(100, 130), Some(30));
        assert_eq!(pct_delta(100, 70), Some(-30));
        assert_eq!(pct_delta(0, 0), Some(0));
    }

    #[test]
    fn pct_delta_undefined_when_a_is_zero_b_nonzero() {
        // P3 #17: 0 → 5 has no meaningful percent delta. Returns None.
        assert_eq!(pct_delta(0, 5), None);
        assert_eq!(pct_delta(0, 1), None);
    }

    fn model_call_with_text(step: u64, text: &str) -> Track {
        t(
            step,
            Kind::ModelCall,
            json!({
                "response": {
                    "content": [{"type": "text", "text": text}]
                }
            }),
        )
    }

    #[test]
    fn last_answer_skips_parser_warning_at_end() {
        // Issue #15: parser-warning annotation at end must not be picked as the
        // canonical answer — last_answer should fall through to model_call text.
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            model_call_with_text(2, "the real model answer"),
            t(
                3,
                Kind::Annotation,
                json!({
                    "by": "tape:transcript-parser",
                    "note": "parse warnings: 1 unknown event types, 0 malformed lines",
                }),
            ),
        ];
        assert_eq!(
            last_answer(&tracks),
            Some("the real model answer".to_string())
        );
    }

    #[test]
    fn last_answer_prefers_agent_annotation_over_later_parser_warning() {
        // Agent annotation at step 5, parser annotation at step 9 — agent wins
        // even though it's not the last annotation chronologically.
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            model_call_with_text(2, "intermediate model text"),
            t(
                5,
                Kind::Annotation,
                json!({"by": "agent", "note": "agent's canonical answer"}),
            ),
            t(
                9,
                Kind::Annotation,
                json!({
                    "by": "tape:transcript-parser",
                    "note": "parse warnings: 2 unknown event types",
                }),
            ),
        ];
        assert_eq!(
            last_answer(&tracks),
            Some("agent's canonical answer".to_string())
        );
    }

    #[test]
    fn last_answer_falls_through_when_only_user_annotations() {
        // Only user annotations, no agent — must fall through to model_call.
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            model_call_with_text(2, "model text wins"),
            t(
                3,
                Kind::Annotation,
                json!({"by": "user", "note": "looks good, ship it"}),
            ),
        ];
        assert_eq!(last_answer(&tracks), Some("model text wins".to_string()));
    }

    #[test]
    fn last_answer_no_annotations_returns_model_call_text() {
        // No annotations at all — behavior unchanged: returns last model_call.
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            model_call_with_text(2, "only model answer"),
        ];
        assert_eq!(last_answer(&tracks), Some("only model answer".to_string()));
    }
}
