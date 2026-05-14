//! Audit-trail row consumers persist after every judge call.
//!
//! Each downstream feature (`tape diff --judge`, `tape recap --auto`,
//! `tape relinernote`, `tape tag --auto`) owns its own `meta.X[]`
//! array on the cassette; the row shape is the same so a future
//! cross-feature analysis can union them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanOutcome {
    /// Defense-in-depth scanner returned no hits.
    Clean,
    /// Scanner fired — the call returned an error and this row is
    /// here so consumers can attribute the rejection. The judge
    /// client itself only emits `Clean` rows (it returns
    /// `JudgeError::Rejected` instead of persisting), but consumers
    /// re-using the type for their own scanning passes use this.
    Rejected { rule_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JudgeCallRecord {
    /// Wall-clock time at the moment the call returned. ISO-8601 UTC
    /// `%Y-%m-%dT%H:%M:%S%.3fZ`, matching the format every other tape
    /// timestamp uses (SPEC §3.1).
    pub ts: String,
    /// Model id from `JudgeConfig::model`. Lets a future analysis
    /// pin output quality to provider version.
    pub model: String,
    /// `blake3` of the prompt (post-truncation, pre-network). Hex.
    /// Pins what was *asked*; an upstream that returns different
    /// outputs on retry still attributes them to the same prompt.
    pub prompt_hash: String,
    /// `blake3` of the model's response text. Hex.
    pub output_hash: String,
    /// Defense-in-depth scan verdict. Always `Clean` on a successful
    /// call (`JudgeError::Rejected` short-circuits before the
    /// record is built).
    pub scan_result: ScanOutcome,
    /// Number of *retries* (not total attempts). A first-try success
    /// has `retry_count == 0`.
    pub retry_count: u32,
    /// Whether the prompt was head-truncated to fit
    /// `JudgeConfig::max_input_chars`. Surfaces a soft warning so the
    /// consumer can decide to widen the budget for the cassette.
    pub truncated: bool,
}

/// Lowercase-hex `blake3` digest of a string. Public so consumers can
/// reproduce the same hashing strategy if they fold the row into a
/// larger audit trail.
pub fn hash_blake3(s: &str) -> String {
    blake3::hash(s.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trips_through_serde() {
        let r = JudgeCallRecord {
            ts: "2026-05-14T10:00:00.000Z".into(),
            model: "gpt-4o".into(),
            prompt_hash: "deadbeef".into(),
            output_hash: "cafebabe".into(),
            scan_result: ScanOutcome::Clean,
            retry_count: 1,
            truncated: false,
        };
        let json = serde_json::to_string(&r).unwrap();
        let parsed: JudgeCallRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, r);
    }

    #[test]
    fn rejected_scan_outcome_serializes_with_rule_id() {
        let outcome = ScanOutcome::Rejected {
            rule_id: "instruction_override_ignore".into(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("rejected"), "{json}");
        assert!(json.contains("instruction_override_ignore"), "{json}");
    }

    #[test]
    fn hash_is_deterministic_and_lowercase_hex() {
        let h = hash_blake3("hello");
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(hash_blake3("hello"), h);
        assert_ne!(hash_blake3("hello"), hash_blake3("Hello"));
    }
}
