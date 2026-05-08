//! Built-in redaction rules. See SPEC.md §7.
//!
//! Order matters: `anthropic_api_key` runs before `openai_api_key` so an
//! `sk-ant-…` value isn't first matched as an OpenAI key. `aws_secret_key`
//! runs before `generic_high_entropy` so a context-tagged AWS secret gets
//! a typed placeholder rather than the generic `<SECRET>`.

use regex::Regex;

use crate::Rule;

/// Build the canonical built-in rule set in priority order.
#[allow(clippy::too_many_lines)]
pub fn built_in() -> Vec<Rule> {
    vec![
        Rule {
            id: "anthropic_api_key".into(),
            regex: Regex::new(r"sk-ant-[A-Za-z0-9_-]{40,}").unwrap(),
            replacement: "<API_KEY:anthropic>".into(),
            validator: None,
            default_enabled: true,
            target_capture: None,
        },
        Rule {
            id: "openai_api_key".into(),
            // After anthropic runs, sk-ant-... matches are gone, so a simple
            // `sk-[A-Za-z0-9]{20,}` is enough.
            regex: Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap(),
            replacement: "<API_KEY:openai>".into(),
            validator: None,
            default_enabled: true,
            target_capture: None,
        },
        Rule {
            id: "aws_access_key".into(),
            regex: Regex::new(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b").unwrap(),
            replacement: "<API_KEY:aws_access>".into(),
            validator: None,
            default_enabled: true,
            target_capture: None,
        },
        Rule {
            // SPEC §7: 40-char base64 within 50 bytes of `aws_secret`/`AWS_SECRET`
            // context. We match the full window (case-insensitive label + up to
            // 50 chars of separator + 40 base64 chars) but redact only capture
            // group 1 (the secret) so the label survives — leaks contained,
            // breadcrumbs intact.
            id: "aws_secret_key".into(),
            regex: Regex::new(
                // Context label, then up to 50 chars (lazy) of *anything*
                // non-newline, then 40 base64 chars. Lazy quantifier finds
                // the shortest gap; the {0,50} ceiling keeps the rule from
                // pairing a label with a secret far away in the document.
                r#"(?i)aws[_\-]?secret(?:[_\-](?:access[_\-])?key)?[^\n]{0,50}?([A-Za-z0-9/+=]{40})"#,
            )
            .unwrap(),
            replacement: "<API_KEY:aws_secret>".into(),
            validator: None,
            default_enabled: true,
            target_capture: Some(1),
        },
        Rule {
            id: "jwt".into(),
            regex: Regex::new(r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").unwrap(),
            replacement: "<JWT>".into(),
            validator: None,
            default_enabled: true,
            target_capture: None,
        },
        Rule {
            id: "bearer_token".into(),
            regex: Regex::new(r"Bearer\s+[A-Za-z0-9._-]{20,}").unwrap(),
            replacement: "<BEARER>".into(),
            validator: None,
            default_enabled: true,
            target_capture: None,
        },
        Rule {
            id: "ssn".into(),
            regex: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
            replacement: "<SSN>".into(),
            validator: None,
            default_enabled: true,
            target_capture: None,
        },
        Rule {
            // Tightened: domain part requires alphanumeric+dash before each dot;
            // rejects `..` runs.
            id: "email".into(),
            regex: Regex::new(r"[A-Za-z0-9._%+\-]+@(?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,}").unwrap(),
            replacement: "<EMAIL>".into(),
            validator: None,
            default_enabled: true,
            target_capture: None,
        },
        Rule {
            id: "credit_card".into(),
            regex: Regex::new(r"\b(?:\d[ -]?){13,19}\b").unwrap(),
            replacement: "<CC>".into(),
            validator: Some(luhn_valid),
            default_enabled: true,
            target_capture: None,
        },
        Rule {
            id: "ipv4_private".into(),
            regex: Regex::new(
                r"\b(?:10(?:\.\d{1,3}){3}|172\.(?:1[6-9]|2\d|3[01])(?:\.\d{1,3}){2}|192\.168(?:\.\d{1,3}){2})\b",
            )
            .unwrap(),
            replacement: "<IP:private>".into(),
            validator: None,
            default_enabled: false,
            target_capture: None,
        },
        Rule {
            id: "generic_high_entropy".into(),
            regex: Regex::new(r"[A-Za-z0-9+/=_-]{32,}").unwrap(),
            replacement: "<SECRET>".into(),
            validator: Some(high_entropy_validator),
            default_enabled: false,
            target_capture: None,
        },
    ]
}

/// Luhn check for credit-card validation. Strips spaces and hyphens.
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

/// Shannon-entropy validator for high-entropy secrets. Conservative:
/// requires ≥4.5 bits/char and that the string isn't dominated by a single
/// character class.
fn high_entropy_validator(s: &str) -> bool {
    if s.len() < 32 {
        return false;
    }
    let mut counts = [0u32; 256];
    for b in s.bytes() {
        counts[b as usize] += 1;
    }
    let len = s.len() as f64;
    let mut entropy = 0.0_f64;
    for c in counts.iter() {
        if *c == 0 {
            continue;
        }
        let p = (*c as f64) / len;
        entropy -= p * p.log2();
    }
    entropy >= 4.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule_by_id(id: &str) -> Rule {
        built_in().into_iter().find(|r| r.id == id).expect("rule exists")
    }

    fn matches(rule: &Rule, s: &str) -> bool {
        rule.regex
            .find_iter(s)
            .any(|m| rule.validator.is_none_or(|v| v(m.as_str())))
    }

    // -------- email --------
    #[test]
    fn email_positives() {
        let r = rule_by_id("email");
        for s in [
            "alice@example.com",
            "Contact me: alice@example.com tomorrow",
            "alice@example.com starts the line",
            "ends with bob@example.org",
            "a@b.co and c@d.io",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn email_negatives() {
        let r = rule_by_id("email");
        for s in [
            "alice@example", // no TLD
            "@example.com",   // no local
            "alice@.com",     // empty domain label
            "not.an.email.address",
            "no at sign here",
            "alice@example..com", // P3 #18: consecutive dots in domain
            "alice@.example.com", // leading dot in domain
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    // -------- anthropic_api_key --------
    #[test]
    fn anthropic_positives() {
        let r = rule_by_id("anthropic_api_key");
        for s in [
            "sk-ant-api03-AbCdEf1234567890abcdef1234567890aBcDeF12_-",
            "header: sk-ant-api03-AbCdEf1234567890abcdef1234567890aBcDeF12_-",
            "sk-ant-api03-AbCdEf1234567890abcdef1234567890aBcDeF12_- start",
            "ends with sk-ant-api03-AbCdEf1234567890abcdef1234567890aBcDeF12_-",
            "two: sk-ant-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa and sk-ant-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn anthropic_negatives() {
        let r = rule_by_id("anthropic_api_key");
        for s in [
            "sk-ant-",                     // just prefix
            "sk-ant-tooshort",              // <40 chars after prefix
            "sk-AbCdEfGhIjKlMnOpQrStUvWxYz12", // OpenAI shape
            "not-an-sk-ant-...",
            "ANT-sk-AbCdEfGhIjKlMnOpQrStUvWxYz1234567",
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    // -------- openai_api_key --------
    #[test]
    fn openai_positives() {
        let r = rule_by_id("openai_api_key");
        for s in [
            "sk-AbCdEfGhIjKlMnOpQrStUvWxYz12",
            "API: sk-AbCdEfGhIjKlMnOpQrStUvWxYz12",
            "sk-AbCdEfGhIjKlMnOpQrStUvWxYz12 starts",
            "ends with sk-AbCdEfGhIjKlMnOpQrStUvWxYz12",
            "two: sk-AbCdEfGhIjKlMnOpQrStUvWxYz12 sk-1234567890123456789012345",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn openai_negatives() {
        let r = rule_by_id("openai_api_key");
        for s in [
            "sk-",
            "sk-tooshort",
            "SK-uppercase01234567890",
            "no-prefix-AbCdEfGhIjKlMnOpQr",
            "sk-with-dashes-and-not-enough",
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    // -------- aws_access_key --------
    #[test]
    fn aws_access_positives() {
        let r = rule_by_id("aws_access_key");
        for s in [
            "AKIA1234567890ABCDEF",
            "use AKIA1234567890ABCDEF here",
            "ASIAQRSTUVWXYZ012345",
            "AKIAABCDEFGHIJKLMNOP starts",
            "ends ASIA1234567890ABCDEF",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn aws_access_negatives() {
        let r = rule_by_id("aws_access_key");
        for s in [
            "AKIA1234",
            "akia1234567890ABCDEF",   // lowercase prefix
            "BKIA1234567890ABCDEF",   // wrong prefix
            "AKIA12345678901234567890", // too long
            "embeddedAKIA1234567890ABCDEFinside",
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    // -------- aws_secret_key (P1 #1) --------
    // Capture-group rule: full match includes the context label, but
    // replacement targets only group 1 (the 40-char secret). End-to-end
    // behavior tested via Engine::redact_string in the engine tests below.
    #[test]
    fn aws_secret_positives_match() {
        let r = rule_by_id("aws_secret_key");
        for s in [
            // Synthetic 40-char base64-style secrets. Real ones look the same shape.
            "aws_secret_access_key = abcdEFGH1234ijklMNOP5678qrstUVWX9012yz==",
            "AWS_SECRET_ACCESS_KEY=abcdEFGH1234ijklMNOP5678qrstUVWX9012yz==",
            "aws_secret_key: abcdEFGH1234ijklMNOP5678qrstUVWX9012yz==",
            "aws-secret-key=\"abcdEFGH1234ijklMNOP5678qrstUVWX9012yz==\"",
            "config:\n  aws_secret_access_key = abcdEFGH1234ijklMNOP5678qrstUVWX9012yz==\n",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn aws_secret_negatives() {
        let r = rule_by_id("aws_secret_key");
        for s in [
            // Bare 40-char base64 with no aws_secret context — no match.
            "abcdEFGH1234ijklMNOP5678qrstUVWX9012yz==",
            // Context but secret is too short.
            "aws_secret_access_key = tooshort",
            // No "secret" in the label.
            "aws_access_key = abcdEFGH1234ijklMNOP5678qrstUVWX9012yz==",
            // Context too far from secret (>50 chars between).
            "aws_secret_access_key was rotated last week — see ticket. The new value is abcdEFGH1234ijklMNOP5678qrstUVWX9012yz==",
            // 40 chars but contains a non-base64 character.
            "aws_secret_access_key = abcd!FGH1234ijklMNOP5678qrstUVWX9012yz==",
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }
    #[test]
    fn aws_secret_redacts_only_secret_not_label() {
        // End-to-end: ensure the label survives, only the secret is replaced.
        let mut s = "aws_secret_access_key = abcdEFGH1234ijklMNOP5678qrstUVWX9012yz==".to_string();
        let engine = crate::Engine::with_default_rules();
        engine.redact_string(&mut s);
        assert!(s.contains("aws_secret_access_key"));
        assert!(s.contains("<API_KEY:aws_secret>"));
        assert!(!s.contains("abcdEFGH1234ijklMNOP5678qrstUVWX9012yz=="));
    }

    // -------- jwt --------
    #[test]
    fn jwt_positives() {
        let r = rule_by_id("jwt");
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        for s in [
            jwt,
            &format!("Authorization: Bearer {jwt}"),
            &format!("starts {jwt}"),
            &format!("{jwt} ends"),
            &format!("two: {jwt} {jwt}"),
        ] {
            assert!(matches(&r, s), "should match");
        }
    }
    #[test]
    fn jwt_negatives() {
        let r = rule_by_id("jwt");
        for s in [
            "eyJhbGci", // single segment
            "eyJ.eyJ",  // missing third segment
            "abc.def.ghi", // doesn't start with eyJ
            "no jwt here",
            "eyJ.eyJ.", // empty third segment
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    // -------- bearer_token --------
    #[test]
    fn bearer_positives() {
        let r = rule_by_id("bearer_token");
        for s in [
            "Bearer abcdefghijklmnopqrstuvwxyz",
            "Authorization: Bearer abc123def456ghi789jkl",
            "Bearer AAAAAAAAAAAAAAAAAAAA prefix",
            "x: Bearer abc-def-ghi-jkl-mno-pqr",
            "Bearer 0123456789012345678901234567890",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn bearer_negatives() {
        let r = rule_by_id("bearer_token");
        for s in [
            "Bearer short", // <20 chars
            "BearerNoSpace01234567890123",
            "TokenAbCdEfGhIjKlMnOpQrStUvWx",
            "no auth here",
            "Bearer", // no token at all
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    // -------- ssn --------
    #[test]
    fn ssn_positives() {
        let r = rule_by_id("ssn");
        for s in [
            "123-45-6789",
            "SSN: 123-45-6789 here",
            "123-45-6789 leading",
            "trailing 999-99-9999",
            "two 123-45-6789 and 234-56-7890",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn ssn_negatives() {
        let r = rule_by_id("ssn");
        for s in [
            "12-345-6789",
            "1234-5-6789",
            "123 45 6789",
            "123-45-678",
            "no digits here",
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    // -------- credit_card --------
    #[test]
    fn cc_positives() {
        let r = rule_by_id("credit_card");
        for s in [
            "4532015112830366",                   // Visa test
            "5555555555554444",                   // Mastercard test
            "Card: 4111 1111 1111 1111 today",    // spaces
            "with-dashes 4111-1111-1111-1111",
            "two 4532015112830366 and 5555555555554444",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn cc_negatives() {
        let r = rule_by_id("credit_card");
        for s in [
            "4532015112830367",         // bad Luhn
            "1234567890123456",         // bad Luhn
            "12345",                    // too short
            "abcdefghijklmnop",         // not digits
            "no card here",
            "4532-0151-1283-0367",      // bad Luhn with separators
            "12 34 56 78",              // too short
            "phone 555-1234",           // too short
            "id 1234",                  // too short
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    // -------- ipv4_private (opt-in) --------
    #[test]
    fn ipv4_positives() {
        let r = rule_by_id("ipv4_private");
        for s in [
            "10.0.0.1",
            "host 192.168.1.1 here",
            "172.16.0.1 prefix",
            "trailing 172.31.255.255",
            "two 10.1.2.3 and 192.168.0.1",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn ipv4_negatives() {
        let r = rule_by_id("ipv4_private");
        for s in [
            "8.8.8.8",
            "172.15.0.1",
            "172.32.0.1",
            "192.169.0.1",
            "1.2.3.4.5",
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    // -------- generic_high_entropy (opt-in) --------
    #[test]
    fn high_entropy_positives() {
        let r = rule_by_id("generic_high_entropy");
        for s in [
            "QkpaUVlGYWNYTm5hVU5JT1JzN1F0V01HVEpoUWVtR3o",
            "prefix Mk7PqRsTuVwXyZ12AbCdE3FgHiJkLmNoPqRsTuV",
            "a1B2c3D4e5F6g7H8i9J0kLmNoPqRsTuVwXyZ_aB",
            "ZyrQv9kF3pSx7TWmJ8Lh2NbA1cV6oKgEdU0Pn4iH5R",
            "0K9j8H7g6F5e4D3c2B1aZyXwVuTsRqPoNmLkJiHgFeDcBa",
        ] {
            assert!(matches(&r, s), "should match: {s}");
        }
    }
    #[test]
    fn high_entropy_negatives() {
        let r = rule_by_id("generic_high_entropy");
        for s in [
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "12345678",
            "the quick brown fox jumps",
            "abc",
            "----------------------------------------",
        ] {
            assert!(!matches(&r, s), "should NOT match: {s}");
        }
    }

    #[test]
    fn luhn_check_works() {
        assert!(luhn_valid("4532015112830366"));
        assert!(!luhn_valid("4532015112830367"));
    }

    #[test]
    fn high_entropy_validator_rejects_low_entropy() {
        let s = "a".repeat(40);
        assert!(!high_entropy_validator(&s));
        let mixed = "QkpaUVlGYWNYTm5hVU5JT1JzN1F0V01HVEpoUWVtR3o";
        assert!(high_entropy_validator(mixed));
    }
}
