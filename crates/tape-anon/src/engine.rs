//! The Phase-1 anonymization walker. Reuses the surfaces the redaction
//! engine walks at eject time (track payloads, meta.yaml text fields,
//! liner-notes.md), reapplied post-hoc on an already-ejected cassette.
//!
//! Per the ticket's open Q1 the Phase-1 implementation is a parallel
//! walker rather than wedging per-match replacement into
//! `tape_redact::Engine`. The walker keeps the blast radius inside
//! `tape-anon`.

use crate::pseudonym::Pseudonymizer;
use crate::rules::AnonRule;
use serde_json::Value;

#[derive(Debug, Default)]
pub struct ApplyReport {
    pub n_replacements: usize,
}

/// In-place mutation of a single string. Each match of any rule's
/// regex is replaced by `<PATH:home:<8hex>>` where the 8hex pseudonym
/// is derived from the matched substring.
pub fn anonymize_string(rules: &[AnonRule], p: &mut Pseudonymizer, s: &mut String) -> usize {
    let mut n = 0;
    for rule in rules {
        // Collect matches first so we don't iterate-and-mutate.
        let spans: Vec<(usize, usize, String)> = rule
            .regex
            .find_iter(s)
            .map(|m| (m.start(), m.end(), m.as_str().to_owned()))
            .collect();
        if spans.is_empty() {
            continue;
        }
        // Walk right-to-left so byte offsets stay valid while we
        // splice. (For a 1-rule Phase 1 this is over-cautious — the
        // replacement length differs from the match length only
        // marginally — but it's the simplest correct shape and Phase
        // 2 will add multi-rule cascading where ordering matters.)
        let mut new_s = s.clone();
        for (start, end, matched) in spans.into_iter().rev() {
            let pseudo = p.pseudonym(rule.id, &matched);
            let token = render_token(rule.id, &pseudo);
            new_s.replace_range(start..end, &token);
            n += 1;
        }
        *s = new_s;
    }
    n
}

/// Recursively walk a JSON Value, mutating every string field. Mirrors
/// the shape of `tape_redact::Engine::redact_value` (a parallel walker
/// per open Q1).
pub fn anonymize_value(rules: &[AnonRule], p: &mut Pseudonymizer, value: &mut Value) -> usize {
    let mut n = 0;
    walk(value, &mut |s| n += anonymize_string(rules, p, s));
    n
}

fn walk<F: FnMut(&mut String)>(value: &mut Value, f: &mut F) {
    match value {
        Value::String(s) => f(s),
        Value::Array(a) => {
            for v in a.iter_mut() {
                walk(v, f);
            }
        }
        Value::Object(o) => {
            for (_k, v) in o.iter_mut() {
                walk(v, f);
            }
        }
        _ => {}
    }
}

/// Render the replacement token for a `(rule_id, pseudonym)` pair.
/// Phase 1 hardcodes the `<PATH:home:8hex>` shape since the one rule's
/// canonical token shape is `PATH:home`. Phase 2+ generalizes per the
/// `<KIND:tag:8hex>` table in #42 §3.2.
fn render_token(rule_id: &str, pseudonym: &str) -> String {
    match rule_id {
        "unix_home_path" => format!("<PATH:home:{pseudonym}>"),
        // Defensive default — Phase 2+ rules add explicit arms here.
        other => format!("<ANON:{other}:{pseudonym}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::built_in_rules;
    use serde_json::json;

    fn fixed_p() -> Pseudonymizer {
        Pseudonymizer::with_salt([0x42; 32])
    }

    #[test]
    fn within_string_repeated_prefix_gets_same_token() {
        let rules = built_in_rules();
        let mut p = fixed_p();
        let mut s = String::from("first /Users/colin/a then /Users/colin/b");
        let n = anonymize_string(&rules, &mut p, &mut s);
        assert_eq!(n, 2);
        // Both occurrences scrub to the SAME pseudonym (cache hit).
        let token = render_token(
            "unix_home_path",
            &p.pseudonym("unix_home_path", "/Users/colin"),
        );
        assert!(
            s.contains(&format!("first {token}/a then {token}/b")),
            "got: {s}"
        );
    }

    #[test]
    fn distinct_prefixes_get_distinct_tokens() {
        let rules = built_in_rules();
        let mut p = fixed_p();
        let mut s = String::from("a /Users/colin/x b /Users/alice/y");
        anonymize_string(&rules, &mut p, &mut s);
        let t_colin = render_token(
            "unix_home_path",
            &p.pseudonym("unix_home_path", "/Users/colin"),
        );
        let t_alice = render_token(
            "unix_home_path",
            &p.pseudonym("unix_home_path", "/Users/alice"),
        );
        assert_ne!(t_colin, t_alice);
        assert!(s.contains(&t_colin) && s.contains(&t_alice));
    }

    #[test]
    fn value_walker_mutates_nested_strings() {
        let rules = built_in_rules();
        let mut p = fixed_p();
        let mut v = json!({
            "path": "/Users/colin/x",
            "cmd": "cat /Users/colin/.bashrc",
            "tags": ["/home/bob", "/root"],
            "nested": {"deep": "/Users/colin"}
        });
        let n = anonymize_value(&rules, &mut p, &mut v);
        // 5 matches: path, cmd, tags[0], tags[1], nested.deep
        assert_eq!(n, 5);
        let s = serde_json::to_string(&v).unwrap();
        assert!(!s.contains("/Users/colin"), "unscrubbed home in {s}");
        assert!(!s.contains("/home/bob"), "unscrubbed home in {s}");
        assert!(!s.contains("/root"), "unscrubbed home in {s}");
    }

    #[test]
    fn no_matches_returns_zero_and_does_not_mutate() {
        let rules = built_in_rules();
        let mut p = fixed_p();
        let mut s = String::from("/usr/local/bin/cargo");
        let before = s.clone();
        let n = anonymize_string(&rules, &mut p, &mut s);
        assert_eq!(n, 0);
        assert_eq!(s, before);
    }

    #[test]
    fn render_token_shape() {
        assert_eq!(
            render_token("unix_home_path", "deadbeef"),
            "<PATH:home:deadbeef>"
        );
    }
}
