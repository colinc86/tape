//! Defense-in-depth secret scan for `tape verify`.
//!
//! SPEC §3.3 (`meta.yaml`) and §4.3 (`liner-notes.md`) require that neither
//! file contain a string matching a built-in redaction rule. SPEC §10.5
//! makes this normative — `tape verify` MUST reject tapes that fail it.
//!
//! Why a separate copy of the rule patterns lives here and not in
//! `tape-redact`: `tape-redact` already depends on `tape-format`, so
//! `tape-format` can't pull the rule set in via the redaction crate
//! without creating a cycle. The patterns are duplicated rather than the
//! whole module moved because the in-flight #23 / #17 PRs are also
//! reshaping the redaction surface — a deeper consolidation belongs in a
//! follow-up once those land. (Issue #33.)
//!
//! Until then, **any change to a default-enabled rule in
//! `tape-redact::rules::built_in()` MUST be mirrored here**, and vice
//! versa. The pattern strings are kept identical to make a textual diff
//! between the two files trivial.

use regex::Regex;
use std::sync::OnceLock;

/// One default-enabled built-in rule. Mirrors `tape_redact::Rule` minus
/// the engine-specific fields (`replacement`, `target_capture`, etc.) —
/// here we only need to know whether a match exists and what to label it.
struct ScanRule {
    id: &'static str,
    regex: &'static OnceLock<Regex>,
    pattern: &'static str,
    /// Optional second-stage validator (e.g. Luhn for `credit_card`).
    /// Returning `false` rejects the regex match as a false positive.
    validator: Option<fn(&str) -> bool>,
}

impl ScanRule {
    fn compiled(&self) -> &'static Regex {
        self.regex.get_or_init(|| {
            Regex::new(self.pattern).expect("compile-time-valid regex")
        })
    }
}

// One `OnceLock<Regex>` per rule so the compiled regex is built once and
// shared across calls (the `Regex` type internally uses `Arc`s so cloning
// is cheap, but compilation is not — we don't want to pay it per scan).

macro_rules! rule_slot {
    () => {{
        static SLOT: OnceLock<Regex> = OnceLock::new();
        &SLOT
    }};
}

fn rules() -> &'static [ScanRule] {
    static RULES: OnceLock<[ScanRule; 9]> = OnceLock::new();
    RULES.get_or_init(|| {
        // Order mirrors `tape_redact::rules::built_in()` exactly. Each
        // entry's pattern is a byte-for-byte copy of the corresponding
        // rule there; cross-check on every edit.
        [
            ScanRule {
                id: "anthropic_api_key",
                regex: rule_slot!(),
                pattern: r"sk-ant-[A-Za-z0-9_-]{40,}",
                validator: None,
            },
            ScanRule {
                id: "openai_api_key",
                regex: rule_slot!(),
                pattern: r"sk-[A-Za-z0-9]{20,}",
                validator: None,
            },
            ScanRule {
                id: "aws_access_key",
                regex: rule_slot!(),
                pattern: r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
                validator: None,
            },
            ScanRule {
                id: "aws_secret_key",
                regex: rule_slot!(),
                pattern: r"(?i)aws[_\-]?secret(?:[_\-](?:access[_\-])?key)?[^\n]{0,50}?([A-Za-z0-9/+=]{40})",
                validator: None,
            },
            ScanRule {
                id: "jwt",
                regex: rule_slot!(),
                pattern: r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
                validator: None,
            },
            ScanRule {
                id: "bearer_token",
                regex: rule_slot!(),
                pattern: r"Bearer\s+[A-Za-z0-9._-]{20,}",
                validator: None,
            },
            ScanRule {
                id: "ssn",
                regex: rule_slot!(),
                pattern: r"\b\d{3}-\d{2}-\d{4}\b",
                validator: None,
            },
            ScanRule {
                id: "email",
                regex: rule_slot!(),
                pattern: r"[A-Za-z0-9._%+\-]+@(?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,}",
                validator: None,
            },
            ScanRule {
                id: "credit_card",
                regex: rule_slot!(),
                pattern: r"\b(?:\d[ -]?){13,19}\b",
                validator: Some(luhn_valid),
            },
        ]
    })
}

/// Return the rule ids that match somewhere in `text`. Empty result means
/// the text is clean of every default-enabled built-in pattern.
pub fn scan(text: &str) -> Vec<&'static str> {
    let mut hits = Vec::new();
    for rule in rules() {
        let re = rule.compiled();
        let matched = if let Some(validator) = rule.validator {
            re.find_iter(text).any(|m| validator(m.as_str()))
        } else {
            re.is_match(text)
        };
        if matched {
            hits.push(rule.id);
        }
    }
    hits
}

/// Luhn check for credit-card validation. Strips spaces and hyphens.
/// Mirrors `tape_redact::rules::luhn_valid`.
fn luhn_valid(s: &str) -> bool {
    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    let mut sum = 0u32;
    for (i, d) in digits.iter().rev().enumerate() {
        if i % 2 == 1 {
            let doubled = d * 2;
            sum += if doubled > 9 { doubled - 9 } else { doubled };
        } else {
            sum += d;
        }
    }
    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_has_no_hits() {
        assert!(scan("").is_empty());
        assert!(scan("ordinary prose with no secrets in it.").is_empty());
    }

    #[test]
    fn anthropic_key_prefix_hits() {
        let s = "auth: sk-ant-api03-AbCdEf1234567890abcdef1234567890aBcDeF12_-XX";
        assert!(scan(s).contains(&"anthropic_api_key"));
    }

    #[test]
    fn email_hits() {
        let hits = scan("Email me at alice@example.com please.");
        assert!(hits.contains(&"email"), "expected email; got {hits:?}");
    }

    /// The pre-fix comment in verify.rs claimed emails could false-positive
    /// on URLs with `@` in commit hashes. The current pattern requires a
    /// TLD-shaped suffix, so this case correctly does NOT match.
    #[test]
    fn commit_hash_email_lookalike_does_not_hit() {
        assert!(!scan("https://example.com/repo/foo@1234abcdef").contains(&"email"));
    }

    #[test]
    fn jwt_hits() {
        let jwt = "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let hits = scan(jwt);
        assert!(hits.contains(&"jwt"), "expected jwt; got {hits:?}");
    }

    #[test]
    fn aws_access_key_hits() {
        assert!(scan("AKIA1234567890ABCDEF").contains(&"aws_access_key"));
        assert!(scan("ASIA1234567890ABCDEF").contains(&"aws_access_key"));
    }

    #[test]
    fn bearer_token_hits() {
        assert!(scan("Bearer abcdefghijklmnopqrstuvwxyz0123456789").contains(&"bearer_token"));
    }

    #[test]
    fn ssn_hits() {
        assert!(scan("contact ssn 123-45-6789 on file").contains(&"ssn"));
    }

    #[test]
    fn credit_card_luhn_validated() {
        // Valid Luhn: 4111 1111 1111 1111.
        assert!(scan("card 4111-1111-1111-1111").contains(&"credit_card"));
        // Random 16 digits that fail Luhn — must not hit.
        assert!(!scan("card 4111-1111-1111-1112").contains(&"credit_card"));
    }

    /// `tape.snapshot`-produced tapes leave `tape-mcp/X.Y.Z+transcript` in
    /// `meta.recorder.agent`. The `+` is allowed in the local email RFC
    /// but the rest of the string isn't a TLD-shaped domain, so it must
    /// not trigger the email rule.
    #[test]
    fn recorder_agent_string_does_not_false_positive() {
        let meta = "recorder:\n  agent: tape-mcp/0.1.1+transcript\n";
        assert!(scan(meta).is_empty(), "got hits: {:?}", scan(meta));
    }
}
