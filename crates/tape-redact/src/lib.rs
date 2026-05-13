//! Redaction engine. See SPEC.md §7 (rules) and §8 (eject pipeline).
//!
//! The engine walks JSON values (and plain text — meta.yaml, liner-notes.md)
//! and applies an ordered list of rules. Each match is replaced by a typed
//! placeholder; an audit record (`Redaction`) is emitted per match.
//!
//! Public surface:
//!   - [`Engine::with_default_rules`] — engine seeded with all defaults-on built-ins.
//!   - [`Engine::redact_value`] — walks a JSON Value, mutates strings, returns records.
//!   - [`Engine::redact_text`] — operates on a plain string (for meta/liner).
//!   - [`Engine::scan`] — defense-in-depth scan over the engine's own rules.
//!   - [`scan_for_secrets`] — defense-in-depth scan over ALL built-in rules.
//!   - [`config::TapeRcConfig`] — loader for `.taperc`.

pub mod config;
pub mod rules;

pub use config::engine_with_taperc;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Redaction {
    pub step: u64,
    pub field_path: String,
    pub rule_id: String,
    pub replacement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_range: Option<[u64; 2]>,
}

/// A single redaction rule.
#[derive(Clone)]
pub struct Rule {
    pub id: String,
    pub regex: Regex,
    pub replacement: String,
    /// Optional secondary validator (e.g. Luhn for credit cards). Receives
    /// the full match (group 0). Returns true to keep, false to skip.
    pub validator: Option<fn(&str) -> bool>,
    /// Default state: enabled or opt-in.
    pub default_enabled: bool,
    /// If set, replace only this capture group (1-indexed) rather than the
    /// whole match. Used by `aws_secret_key` so that the leading
    /// `aws_secret = ` context label survives the redaction.
    pub target_capture: Option<usize>,
}

impl std::fmt::Debug for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rule")
            .field("id", &self.id)
            .field("replacement", &self.replacement)
            .field("default_enabled", &self.default_enabled)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct Engine {
    rules: Vec<Rule>,
}

impl Engine {
    /// Engine seeded with all `default_enabled = true` built-ins, in the
    /// canonical order (anthropic_api_key MUST precede openai_api_key).
    pub fn with_default_rules() -> Self {
        Self {
            rules: rules::built_in()
                .into_iter()
                .filter(|r| r.default_enabled)
                .collect(),
        }
    }

    /// Engine seeded with NO rules. Add via `add_rule`.
    pub fn empty() -> Self {
        Self { rules: Vec::new() }
    }

    /// Append a custom rule (or re-enable an opt-in built-in).
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    /// Disable a rule by id.
    pub fn remove_rule(&mut self, id: &str) {
        self.rules.retain(|r| r.id != id);
    }

    pub fn rule_ids(&self) -> Vec<String> {
        self.rules.iter().map(|r| r.id.clone()).collect()
    }

    /// Redact a single string. Mutates `s` in place. Returns a list of
    /// (rule_id, replacement) tuples for each match. Rules are applied in
    /// order; later rules see the output of earlier rules.
    pub fn redact_string(&self, s: &mut String) -> Vec<(String, String)> {
        let mut records = Vec::new();
        for rule in &self.rules {
            // Collect non-overlapping match spans first; we apply replacements
            // right-to-left so byte offsets in the source string remain valid.
            let mut spans: Vec<(usize, usize)> = if let Some(group) = rule.target_capture {
                rule.regex
                    .captures_iter(s)
                    .filter_map(|caps| {
                        let whole = caps.get(0)?;
                        if let Some(v) = rule.validator {
                            if !v(whole.as_str()) {
                                return None;
                            }
                        }
                        let target = caps.get(group)?;
                        Some((target.start(), target.end()))
                    })
                    .collect()
            } else {
                rule.regex
                    .find_iter(s)
                    .filter(|m| match rule.validator {
                        Some(v) => v(m.as_str()),
                        None => true,
                    })
                    .map(|m| (m.start(), m.end()))
                    .collect()
            };

            if spans.is_empty() {
                continue;
            }

            spans.sort_unstable();
            // Drop overlaps left-to-right (keep first occurrence).
            spans.dedup_by(|b, a| a.1 > b.0);
            for (start, end) in spans.iter().rev() {
                s.replace_range(*start..*end, &rule.replacement);
                records.push((rule.id.clone(), rule.replacement.clone()));
            }
        }
        records
    }

    /// Redact every string field within `value`. `field_path` is the
    /// JSONPath of `value` itself; child paths are produced for matches.
    pub fn redact_value(&self, value: &mut Value, step: u64, field_path: &str) -> Vec<Redaction> {
        let mut out = Vec::new();
        self.redact_value_inner(value, step, field_path, &mut out);
        out
    }

    fn redact_value_inner(
        &self,
        value: &mut Value,
        step: u64,
        path: &str,
        out: &mut Vec<Redaction>,
    ) {
        match value {
            Value::String(s) => {
                let records = self.redact_string(s);
                for (rule_id, replacement) in records {
                    out.push(Redaction {
                        step,
                        field_path: path.to_string(),
                        rule_id,
                        replacement,
                        byte_range: None,
                    });
                }
            }
            Value::Object(map) => {
                for (k, v) in map.iter_mut() {
                    let child = if is_simple_ident(k) {
                        format!("{path}.{k}")
                    } else {
                        format!("{path}[{:?}]", k)
                    };
                    self.redact_value_inner(v, step, &child, out);
                }
            }
            Value::Array(arr) => {
                for (i, v) in arr.iter_mut().enumerate() {
                    let child = format!("{path}[{i}]");
                    self.redact_value_inner(v, step, &child, out);
                }
            }
            _ => {}
        }
    }

    /// Defense-in-depth scan: returns the rule_ids in this engine's configured
    /// rule set that would match in `text`, without mutating. Symmetric with
    /// `redact_*`: only the rules that *could have redacted* get to enforce.
    ///
    /// Used by the eject pipeline to verify that meta.yaml, liner-notes.md,
    /// and spilled artifacts don't carry secrets the engine would have caught.
    /// Rules the user did NOT opt into are NOT enforced here — see issue #23.
    pub fn scan(&self, text: &str) -> Vec<String> {
        let mut hits = Vec::new();
        for rule in &self.rules {
            if rule
                .regex
                .find_iter(text)
                .any(|m| rule.validator.is_none_or(|v| v(m.as_str())))
            {
                hits.push(rule.id.clone());
            }
        }
        hits
    }

    /// Redact a plain text document (e.g. liner-notes.md, meta.yaml). Returns
    /// the redacted text plus per-match records. `step=0` is conventional for
    /// non-track redactions (per spec §6.1).
    pub fn redact_text(&self, text: &str, step: u64, label: &str) -> (String, Vec<Redaction>) {
        let mut s = text.to_string();
        let records = self.redact_string(&mut s);
        let out = records
            .into_iter()
            .map(|(rule_id, replacement)| Redaction {
                step,
                field_path: label.to_string(),
                rule_id,
                replacement,
                byte_range: None,
            })
            .collect();
        (s, out)
    }
}

/// Defense-in-depth scan: returns the rule_ids that would match in `text`,
/// without mutating. Used to verify meta.yaml and liner-notes.md don't leak
/// secrets even after redaction.
///
/// Runs ALL built-in rules (including opt-in ones), since this is a hard
/// safety check, not a configurable redaction.
pub fn scan_for_secrets(text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for rule in rules::built_in() {
        if rule
            .regex
            .find_iter(text)
            .any(|m| rule.validator.is_none_or(|v| v(m.as_str())))
        {
            hits.push(rule.id);
        }
    }
    hits
}

fn is_simple_ident(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
