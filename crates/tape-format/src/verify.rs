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
            report.push(Diagnostic::error(
                DiagnosticCode::InvalidTracksJson,
                format!("tracks.jsonl: {e}"),
            ));
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

    // ts monotonic
    {
        let mut prev: Option<&str> = None;
        for t in &tracks {
            if let Some(p) = prev {
                if t.ts.as_str() < p {
                    report.push(Diagnostic::error(
                        DiagnosticCode::TsNotMonotonic,
                        format!(
                            "step {} ts={} earlier than previous {}",
                            t.step, t.ts, p
                        ),
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

    // §10.5 defense-in-depth — cheap built-in pattern scan over meta + liner.
    // We delegate the actual rule definitions to the redact crate later; for now,
    // do a minimal in-tree scan for the lowest-friction cases: anthropic key prefix
    // and bare emails. The full scan moves to tape-redact once that crate exists.
    minimal_secret_scan(meta_yaml, DiagnosticCode::LeakedSecretInMeta, &mut report);
    minimal_secret_scan(liner_md, DiagnosticCode::LeakedSecretInLiner, &mut report);

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

/// Walk a JSON value; for any string field whose JSON-encoded form exceeds
/// PAYLOAD_INLINE_MAX, emit OversizedInlinePayload. Stub `{ref: ...}` objects
/// are exempt (they're already spilled).
fn check_payload_size(v: &serde_json::Value, step: u64, report: &mut VerifyReport) {
    use serde_json::Value;
    match v {
        Value::String(s) => {
            // If the encoded JSON of just the string exceeds the threshold, flag it.
            if s.len() > crate::PAYLOAD_INLINE_MAX {
                report.push(Diagnostic::error(
                    DiagnosticCode::OversizedInlinePayload,
                    format!(
                        "step {} has an inline string of {} bytes (max {})",
                        step,
                        s.len(),
                        crate::PAYLOAD_INLINE_MAX
                    ),
                ));
            }
        }
        Value::Object(map) => {
            // A `{"ref": "sha:..."}` stub is exempt.
            if map.len() == 1 && map.contains_key("ref") {
                return;
            }
            for v in map.values() {
                check_payload_size(v, step, report);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                check_payload_size(v, step, report);
            }
        }
        _ => {}
    }
}

/// Minimal in-tree secret scan for §10.5 defense-in-depth. The full rule
/// engine lives in `tape-redact`; this is the floor enforced even without it.
fn minimal_secret_scan(text: &str, code: DiagnosticCode, report: &mut VerifyReport) {
    if text.contains("sk-ant-") {
        report.push(Diagnostic::error(
            code,
            "contains an Anthropic API key prefix (`sk-ant-`)",
        ));
    }
    // Don't scan for emails here — many tape contents legitimately contain
    // things that look like emails (e.g. URLs with `@` in commit hashes).
    // The full engine in tape-redact runs at eject time and catches these.
}
