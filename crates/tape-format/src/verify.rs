//! `tape verify` — validate a tape against the SPEC §10 rules.
//!
//! This module does NOT read from disk; it consumes a `RawTape` produced by
//! `reader::RawTape`. The CLI binds the two together.

use crate::artifact;
use crate::liner;
use crate::meta::{Meta, Outcome};
use crate::reader::RawTape;
use crate::redactions;
use crate::tracks::{self, Kind};
use crate::TAPE_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    MalformedZip,
    MissingRequiredEntry,
    InvalidMetaYaml,
    WrongTapeVersion,
    InvalidLinerNotes,
    MissingLinerSection,
    LinerSectionsOutOfOrder,
    InvalidTracksJson,
    StepGap,
    UnknownKind,
    MissingTaskEvent,
    MissingEjectEvent,
    EjectNotLast,
    BadTimestamp,
    TsNotMonotonic,
    InvalidPayload,
    MissingArtifact,
    ArtifactHashMismatch,
    OversizedInlinePayload,
    InvalidParentStep,
    OutcomeMismatch,
    RedactionSummaryMismatch,
    LeakedSecretInMeta,
    LeakedSecretInLiner,
    UnknownEntry,
    ReservedKind,
    UnsafePath,
}

impl DiagnosticCode {
    pub fn as_str(self) -> &'static str {
        use DiagnosticCode::*;
        match self {
            MalformedZip => "MALFORMED_ZIP",
            MissingRequiredEntry => "MISSING_REQUIRED_ENTRY",
            InvalidMetaYaml => "INVALID_META_YAML",
            WrongTapeVersion => "WRONG_TAPE_VERSION",
            InvalidLinerNotes => "INVALID_LINER_NOTES",
            MissingLinerSection => "MISSING_LINER_SECTION",
            LinerSectionsOutOfOrder => "LINER_SECTIONS_OUT_OF_ORDER",
            InvalidTracksJson => "INVALID_TRACKS_JSON",
            StepGap => "STEP_GAP",
            UnknownKind => "UNKNOWN_KIND",
            MissingTaskEvent => "MISSING_TASK_EVENT",
            MissingEjectEvent => "MISSING_EJECT_EVENT",
            EjectNotLast => "EJECT_NOT_LAST",
            BadTimestamp => "BAD_TIMESTAMP",
            TsNotMonotonic => "TS_NOT_MONOTONIC",
            InvalidPayload => "INVALID_PAYLOAD",
            MissingArtifact => "MISSING_ARTIFACT",
            ArtifactHashMismatch => "ARTIFACT_HASH_MISMATCH",
            OversizedInlinePayload => "OVERSIZED_INLINE_PAYLOAD",
            InvalidParentStep => "INVALID_PARENT_STEP",
            OutcomeMismatch => "OUTCOME_MISMATCH",
            RedactionSummaryMismatch => "REDACTION_SUMMARY_MISMATCH",
            LeakedSecretInMeta => "LEAKED_SECRET_IN_META",
            LeakedSecretInLiner => "LEAKED_SECRET_IN_LINER",
            UnknownEntry => "UNKNOWN_ENTRY",
            ReservedKind => "RESERVED_KIND",
            UnsafePath => "UNSAFE_PATH",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub message: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Diagnostic {
    pub fn error(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            severity: Severity::Error,
        }
    }
    pub fn warning(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            severity: Severity::Warning,
        }
    }
}

#[derive(Debug, Default)]
pub struct VerifyReport {
    pub diagnostics: Vec<Diagnostic>,
}

impl VerifyReport {
    pub fn is_valid(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }
    fn push(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }
}

/// Run all verification checks on a raw tape.
///
/// Pure — does not read from disk.
pub fn verify(raw: &RawTape) -> VerifyReport {
    let mut report = VerifyReport::default();

    // §10.1 structural — required entries
    let Some(meta_yaml) = raw.meta_yaml.as_deref() else {
        report.push(Diagnostic::error(
            DiagnosticCode::MissingRequiredEntry,
            "meta.yaml missing",
        ));
        return report;
    };
    let Some(liner_md) = raw.liner_md.as_deref() else {
        report.push(Diagnostic::error(
            DiagnosticCode::MissingRequiredEntry,
            "liner-notes.md missing",
        ));
        return report;
    };
    let Some(tracks_jsonl) = raw.tracks_jsonl.as_deref() else {
        report.push(Diagnostic::error(
            DiagnosticCode::MissingRequiredEntry,
            "tracks.jsonl missing",
        ));
        return report;
    };

    for entry in &raw.unknown_entries {
        report.push(Diagnostic::warning(
            DiagnosticCode::UnknownEntry,
            format!("unrecognized zip entry: {entry}"),
        ));
    }

    // §10.2 schema — meta.yaml
    let meta = match Meta::parse(meta_yaml) {
        Ok(m) => m,
        Err(e) => {
            report.push(Diagnostic::error(
                DiagnosticCode::InvalidMetaYaml,
                format!("meta.yaml does not parse: {e}"),
            ));
            return report;
        }
    };

    if meta.tape_version != TAPE_VERSION {
        report.push(Diagnostic::error(
            DiagnosticCode::WrongTapeVersion,
            format!(
                "tape_version is {:?}, expected {:?}",
                meta.tape_version, TAPE_VERSION
            ),
        ));
    }

    // §10.2 — liner notes structure
    {
        let missing = liner::missing_or_empty_sections(liner_md);
        for sect in &missing {
            report.push(Diagnostic::error(
                DiagnosticCode::MissingLinerSection,
                format!("liner-notes.md missing or empty section: {sect:?}"),
            ));
        }
        if missing.is_empty() && !liner::sections_in_order(liner_md) {
            report.push(Diagnostic::error(
                DiagnosticCode::LinerSectionsOutOfOrder,
                "liner-notes.md required sections are not in canonical order",
            ));
        }
    }

    // §10.2 — tracks parse
    let tracks = match tracks::parse_jsonl(tracks_jsonl) {
        Ok(t) => t,
        Err(e) => {
            // SPEC §5.4 / §11: `fork` and `splice` are RESERVED future kinds;
            // v0 readers MUST reject them with a dedicated code rather than the
            // generic `INVALID_TRACKS_JSON`. Serde's closed-enum failure looks
            // identical to any other unknown kind, so when the initial parse
            // fails we do a line-by-line salvage pass that peeks at `kind` as
            // a raw string and emits `RESERVED_KIND` for matching lines.
            // If at least one reserved kind is found we suppress
            // `INVALID_TRACKS_JSON`, because the reserved-kind diagnostic is
            // the specific, actionable cause of the parse failure.
            let mut found_reserved = false;
            for (i, line) in tracks_jsonl.split('\n').enumerate() {
                if line.is_empty() || line.bytes().all(|b| b.is_ascii_whitespace()) {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                let Some(kind) = v.get("kind").and_then(|k| k.as_str()) else {
                    continue;
                };
                if kind == "fork" || kind == "splice" {
                    found_reserved = true;
                    let step = v
                        .get("step")
                        .and_then(serde_json::Value::as_u64)
                        .map_or_else(|| format!("at line {}", i + 1), |n| n.to_string());
                    report.push(Diagnostic::error(
                        DiagnosticCode::ReservedKind,
                        format!(
                            "step {step} has reserved kind `{kind}`; v0 readers MUST reject (SPEC §5.4)"
                        ),
                    ));
                }
            }
            if !found_reserved {
                report.push(Diagnostic::error(
                    DiagnosticCode::InvalidTracksJson,
                    format!("tracks.jsonl: {e}"),
                ));
            }
            return report;
        }
    };

    if tracks.is_empty() {
        report.push(Diagnostic::error(
            DiagnosticCode::MissingTaskEvent,
            "tracks.jsonl has no events",
        ));
        return report;
    }

    // step contiguous from 1
    for (i, t) in tracks.iter().enumerate() {
        let expected = (i as u64) + 1;
        if t.step != expected {
            report.push(Diagnostic::error(
                DiagnosticCode::StepGap,
                format!(
                    "step at line {} is {}, expected {}",
                    i + 1,
                    t.step,
                    expected
                ),
            ));
        }
    }

    // SPEC §5.3 — when parent_step is present, it MUST be in `[1, step)` and
    // MUST reference a step that exists in this tape. Because `step` is
    // contiguous from 1 (checked above), an existing step is exactly any
    // value in `[1, tracks.len()]`; the stricter `< step` rule subsumes the
    // upper bound for any well-numbered tape, but we still report both forms
    // so a tape that fails BOTH StepGap and InvalidParentStep at once gets
    // both errors.
    let max_step = tracks.len() as u64;
    for t in &tracks {
        if let Some(p) = t.parent_step {
            if p == 0 || p >= t.step {
                report.push(Diagnostic::error(
                    DiagnosticCode::InvalidParentStep,
                    format!(
                        "step {} has parent_step={}, must be in [1, {})",
                        t.step, p, t.step
                    ),
                ));
            } else if p > max_step {
                report.push(Diagnostic::error(
                    DiagnosticCode::InvalidParentStep,
                    format!(
                        "step {} parent_step={} references nonexistent step",
                        t.step, p
                    ),
                ));
            }
        }
    }

    // first must be task, last must be eject
    if tracks.first().map(|t| t.kind) != Some(Kind::Task) {
        report.push(Diagnostic::error(
            DiagnosticCode::MissingTaskEvent,
            "first event is not kind=task",
        ));
    }
    let last = tracks.last().expect("non-empty checked above");
    if last.kind != Kind::Eject {
        report.push(Diagnostic::error(
            DiagnosticCode::MissingEjectEvent,
            "last event is not kind=eject",
        ));
    }
    // an eject event in any non-final position
    for t in &tracks[..tracks.len().saturating_sub(1)] {
        if t.kind == Kind::Eject {
            report.push(Diagnostic::error(
                DiagnosticCode::EjectNotLast,
                format!("eject event at step {} is not the last event", t.step),
            ));
        }
    }

    // SPEC §5.4 cardinality: exactly one `task` event, exactly one `eject`
    // event. The first/last checks above don't catch duplicates anywhere
    // else in the tape (a tape with `[task, task, ..., eject]` passes the
    // first-is-task check, and `[task, ..., eject, eject]` passes the
    // last-is-eject check while the EjectNotLast check fires only once
    // for the non-final eject). Count explicitly so the cardinality
    // violation is named precisely. (Issue #86.)
    let task_count = tracks.iter().filter(|t| t.kind == Kind::Task).count();
    if task_count > 1 {
        report.push(Diagnostic::error(
            DiagnosticCode::MissingTaskEvent,
            format!("tape contains {task_count} task events; SPEC §5.4 requires exactly one"),
        ));
    }
    let eject_count = tracks.iter().filter(|t| t.kind == Kind::Eject).count();
    if eject_count > 1 {
        report.push(Diagnostic::error(
            DiagnosticCode::EjectNotLast,
            format!("tape contains {eject_count} eject events; SPEC §5.4 requires exactly one"),
        ));
    }

    // ts monotonic
    {
        let mut prev: Option<&str> = None;
        for t in &tracks {
            if let Some(p) = prev {
                if t.ts.as_str() < p {
                    report.push(Diagnostic::error(
                        DiagnosticCode::TsNotMonotonic,
                        format!("step {} ts={} earlier than previous {}", t.step, t.ts, p),
                    ));
                }
            }
            // Cheap ISO-8601 sniff: must contain T and end with Z or have +/-HH:MM
            if !looks_like_iso8601(&t.ts) {
                report.push(Diagnostic::error(
                    DiagnosticCode::BadTimestamp,
                    format!("step {} ts={:?} not ISO-8601 with timezone", t.step, t.ts),
                ));
            }
            prev = Some(&t.ts);
        }
    }

    // §10.3 reference + spillover checks
    for t in &tracks {
        // No payload field can exceed PAYLOAD_INLINE_MAX as serialized JSON
        check_payload_size(&t.payload, t.step, &mut report);

        for r in &t.refs {
            let Some(hex) = r.strip_prefix("sha:") else {
                report.push(Diagnostic::error(
                    DiagnosticCode::InvalidPayload,
                    format!("step {} ref {:?} not in sha:<hex> form", t.step, r),
                ));
                continue;
            };
            let path = artifact::artifact_path(hex);
            let Some(bytes) = raw.artifacts.get(&path) else {
                report.push(Diagnostic::error(
                    DiagnosticCode::MissingArtifact,
                    format!("step {} refs missing artifact at {}", t.step, path),
                ));
                continue;
            };
            let actual = artifact::blake3_hex(bytes);
            if actual != hex {
                report.push(Diagnostic::error(
                    DiagnosticCode::ArtifactHashMismatch,
                    format!(
                        "artifact {} hash mismatch: claimed {}, computed {}",
                        path, hex, actual
                    ),
                ));
            }
        }
    }

    // §10.4 outcome consistency
    if let Some(eject) = tracks.last() {
        if eject.kind == Kind::Eject {
            if let Some(o) = eject.payload.get("outcome").and_then(|v| v.as_str()) {
                let event_outcome = match o {
                    "success" => Some(Outcome::Success),
                    "failure" => Some(Outcome::Failure),
                    "abandoned" => Some(Outcome::Abandoned),
                    "unknown" => Some(Outcome::Unknown),
                    _ => None,
                };
                if event_outcome != Some(meta.outcome) {
                    report.push(Diagnostic::error(
                        DiagnosticCode::OutcomeMismatch,
                        format!(
                            "meta.outcome={:?} but eject.payload.outcome={:?}",
                            meta.outcome, o
                        ),
                    ));
                }
            } else {
                report.push(Diagnostic::error(
                    DiagnosticCode::InvalidPayload,
                    "eject event has no payload.outcome",
                ));
            }
        }
    }

    // §10.4 redaction-summary consistency
    match (&meta.redaction_summary, &raw.redactions_json) {
        (Some(summary), Some(content)) => match redactions::parse(content) {
            Ok(records) => {
                if records.len() as u64 != summary.redaction_count {
                    report.push(Diagnostic::error(
                        DiagnosticCode::RedactionSummaryMismatch,
                        format!(
                            "redaction_count={} but redactions.json has {} entries",
                            summary.redaction_count,
                            records.len()
                        ),
                    ));
                }
                let actual_rules: std::collections::BTreeSet<_> =
                    records.iter().map(|r| r.rule_id.as_str()).collect();
                let claimed_rules: std::collections::BTreeSet<_> =
                    summary.rules_applied.iter().map(String::as_str).collect();
                if actual_rules != claimed_rules {
                    report.push(Diagnostic::error(
                        DiagnosticCode::RedactionSummaryMismatch,
                        format!(
                            "rules_applied={:?} but redactions.json has {:?}",
                            claimed_rules, actual_rules
                        ),
                    ));
                }
            }
            Err(e) => {
                report.push(Diagnostic::error(
                    DiagnosticCode::InvalidPayload,
                    format!("redactions.json does not parse: {e}"),
                ));
            }
        },
        (Some(_), None) => report.push(Diagnostic::error(
            DiagnosticCode::RedactionSummaryMismatch,
            "meta.redaction_summary present but redactions.json missing",
        )),
        (None, Some(_)) => report.push(Diagnostic::error(
            DiagnosticCode::RedactionSummaryMismatch,
            "redactions.json present but meta.redaction_summary missing",
        )),
        (None, None) => {}
    }

    // §10.5 defense-in-depth — full default-enabled rule set over meta +
    // liner. Mirrors `tape_redact::rules::built_in()`'s default-enabled
    // patterns; see `crate::secret_scan` for the rationale on duplication.
    // (Issue #33.)
    full_secret_scan(meta_yaml, DiagnosticCode::LeakedSecretInMeta, &mut report);
    full_secret_scan(liner_md, DiagnosticCode::LeakedSecretInLiner, &mut report);

    report
}

/// Cheap ISO-8601 sniff: looks like `YYYY-MM-DDTHH:MM:SS(.fff)?(Z|+HH:MM|-HH:MM)`.
fn looks_like_iso8601(s: &str) -> bool {
    if s.len() < 20 {
        return false;
    }
    let bytes = s.as_bytes();
    let has_t = bytes.get(10) == Some(&b'T');
    let last = *bytes.last().unwrap();
    let timezone_ok = last == b'Z'
        || s.ends_with(":00")
        || s.ends_with(":15")
        || s.ends_with(":30")
        || s.ends_with(":45")
        || (bytes.len() >= 6 && (bytes[bytes.len() - 6] == b'+' || bytes[bytes.len() - 6] == b'-'));
    has_t && timezone_ok
}

/// Walk a payload value; for any field whose JSON-encoded form exceeds
/// `PAYLOAD_INLINE_MAX`, emit `OversizedInlinePayload`. Stub `{ref: ...}`
/// objects are exempt (they're already spilled). Mirrors
/// `spill_oversize_in_value` in `tape-record::eject`: the top-level payload
/// wrapper is itself not eligible — only its fields are, and a container
/// field that exceeds the threshold as a whole is flagged without recursing
/// into it.
fn check_payload_size(v: &serde_json::Value, step: u64, report: &mut VerifyReport) {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            if map.len() == 1 && map.contains_key("ref") {
                return;
            }
            for child in map.values() {
                check_field_size(child, step, report);
            }
        }
        Value::Array(arr) => {
            for child in arr {
                check_field_size(child, step, report);
            }
        }
        _ => {}
    }
}

fn check_field_size(v: &serde_json::Value, step: u64, report: &mut VerifyReport) {
    use serde_json::Value;
    if let Value::Object(map) = v {
        if map.len() == 1 && map.contains_key("ref") {
            return;
        }
    }
    let encoded_len = serde_json::to_string(v)
        .map(|s| s.len())
        .unwrap_or(usize::MAX);
    if encoded_len > crate::PAYLOAD_INLINE_MAX {
        let kind = match v {
            Value::String(_) => "string",
            Value::Object(_) => "object",
            Value::Array(_) => "array",
            _ => "value",
        };
        report.push(Diagnostic::error(
            DiagnosticCode::OversizedInlinePayload,
            format!(
                "step {step} has an inline {kind} of {encoded_len} encoded bytes (max {})",
                crate::PAYLOAD_INLINE_MAX
            ),
        ));
        return;
    }
    if matches!(v, Value::Object(_) | Value::Array(_)) {
        check_payload_size(v, step, report);
    }
}

/// §10.5 defense-in-depth scan: runs every default-enabled built-in rule
/// from `crate::secret_scan` against `text` and pushes one diagnostic per
/// matching rule. SPEC §3.3 / §4.3 / §10.5 require `tape verify` to be a
/// portable backstop against tapes whose producer didn't run redaction.
fn full_secret_scan(text: &str, code: DiagnosticCode, report: &mut VerifyReport) {
    for rule_id in crate::secret_scan::scan(text) {
        report.push(Diagnostic::error(
            code,
            format!("contains match for built-in rule {rule_id:?}"),
        ));
    }
}

#[cfg(test)]
mod payload_size_tests {
    use super::*;

    fn diags_for(payload: serde_json::Value) -> Vec<Diagnostic> {
        let mut report = VerifyReport::default();
        check_payload_size(&payload, 1, &mut report);
        report.diagnostics
    }

    #[test]
    fn flags_oversize_string() {
        let big = "x".repeat(crate::PAYLOAD_INLINE_MAX + 100);
        let diags = diags_for(serde_json::json!({"text": big}));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::OversizedInlinePayload);
        assert!(diags[0].message.contains("inline string"));
    }

    /// Regression test for issue #1: an array of small strings whose
    /// JSON-encoded form exceeds 4 KiB must be flagged.
    #[test]
    fn flags_oversize_array() {
        let arr: Vec<serde_json::Value> = (0..500)
            .map(|i| serde_json::Value::String(format!("item-with-id-{i:04}")))
            .collect();
        let payload = serde_json::json!({"choices": arr});
        let diags = diags_for(payload);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::OversizedInlinePayload);
        assert!(diags[0].message.contains("inline array"));
    }

    #[test]
    fn flags_oversize_object() {
        let mut map = serde_json::Map::new();
        for i in 0..500 {
            map.insert(
                format!("k{i:04}"),
                serde_json::Value::String(format!("v{i:04}")),
            );
        }
        let payload = serde_json::json!({"response": serde_json::Value::Object(map)});
        let diags = diags_for(payload);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::OversizedInlinePayload);
        assert!(diags[0].message.contains("inline object"));
    }

    #[test]
    fn ignores_ref_stubs_and_small_payloads() {
        let payload = serde_json::json!({
            "stub": {"ref": "sha:deadbeef"},
            "small": "ok",
            "nums": [1, 2, 3],
        });
        assert!(diags_for(payload).is_empty());
    }

    /// When a parent container is oversize, we flag the parent once — we
    /// don't also descend and flag every inner field.
    #[test]
    fn flags_parent_once_not_children() {
        let inner_big = "X".repeat(crate::PAYLOAD_INLINE_MAX + 100);
        let payload = serde_json::json!({
            "outer": {"inner": inner_big, "tag": "ok"},
        });
        let diags = diags_for(payload);
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
        assert!(diags[0].message.contains("inline object"));
    }
}

#[cfg(test)]
mod reserved_kind_tests {
    //! SPEC §5.4 / §11 / issue #60: `fork` and `splice` are RESERVED future
    //! kinds. v0 readers MUST reject them, and the verifier MUST surface a
    //! dedicated `RESERVED_KIND` diagnostic rather than the generic
    //! `INVALID_TRACKS_JSON` that serde's closed-enum failure would otherwise
    //! yield.

    use super::*;
    use crate::reader::RawTape;
    use std::collections::HashMap;

    fn raw_with_tracks(tracks: &str) -> RawTape {
        let meta = r#"tape_version: "tape/v0"
id: "01h8xy00-0000-7000-b8aa-000000000999"
created_at: "2026-05-06T10:00:00Z"
ejected_at: "2026-05-06T10:00:30Z"
task: "reserved-kind unit test"
recorder:
  agent: "claude-code/2.1.4"
outcome: success
"#;
        let liner = "## What I was asked to do
x

## What I found
x

## Suggested next step / fix
x

## What I'm uncertain about
x
";
        RawTape {
            meta_yaml: Some(meta.into()),
            liner_md: Some(liner.into()),
            tracks_jsonl: Some(tracks.into()),
            redactions_json: None,
            artifacts: HashMap::new(),
            unknown_entries: Vec::new(),
        }
    }

    fn error_codes(report: &VerifyReport) -> Vec<&'static str> {
        report.errors().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn fork_kind_emits_reserved_kind_not_invalid_tracks_json() {
        let tracks = concat!(
            r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
            r#"{"step":2,"kind":"fork","ts":"2026-05-06T10:00:05Z","payload":{}}"#, "\n",
            r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
        );
        let raw = raw_with_tracks(tracks);
        let report = verify(&raw);
        let codes = error_codes(&report);
        assert!(
            codes.contains(&"RESERVED_KIND"),
            "expected RESERVED_KIND, got {codes:?}"
        );
        assert!(
            !codes.contains(&"INVALID_TRACKS_JSON"),
            "INVALID_TRACKS_JSON must be suppressed when a reserved kind is the cause; got {codes:?}"
        );
        let msg = report
            .errors()
            .find(|d| d.code == DiagnosticCode::ReservedKind)
            .map(|d| d.message.clone())
            .unwrap_or_default();
        assert!(msg.contains("fork"), "message should name the kind: {msg}");
        assert!(msg.contains("step 2"), "message should name the step: {msg}");
    }

    #[test]
    fn splice_kind_emits_reserved_kind() {
        let tracks = concat!(
            r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
            r#"{"step":2,"kind":"splice","ts":"2026-05-06T10:00:05Z","payload":{}}"#, "\n",
            r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
        );
        let raw = raw_with_tracks(tracks);
        let report = verify(&raw);
        let codes = error_codes(&report);
        assert!(
            codes.contains(&"RESERVED_KIND"),
            "expected RESERVED_KIND, got {codes:?}"
        );
        assert!(!codes.contains(&"INVALID_TRACKS_JSON"));
    }

    /// A non-reserved unknown kind must still surface as `INVALID_TRACKS_JSON`
    /// — the salvage pass is reserved-kind-specific.
    #[test]
    fn unknown_kind_still_surfaces_as_invalid_tracks_json() {
        let tracks = concat!(
            r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
            r#"{"step":2,"kind":"sneeze","ts":"2026-05-06T10:00:05Z","payload":{}}"#, "\n",
            r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
        );
        let raw = raw_with_tracks(tracks);
        let report = verify(&raw);
        let codes = error_codes(&report);
        assert!(codes.contains(&"INVALID_TRACKS_JSON"), "got {codes:?}");
        assert!(!codes.contains(&"RESERVED_KIND"), "got {codes:?}");
    }
}
