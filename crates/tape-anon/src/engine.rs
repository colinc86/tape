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
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct ApplyReport {
    pub n_replacements: usize,
}

/// Per-rule replacement counts. Stable iteration order via `BTreeMap`
/// — the CLI's stderr success line enumerates rules in
/// `built_in_rules()` order, but the alphabetical BTreeMap order is
/// close enough and avoids passing the rule list through the
/// reporting path.
pub type RuleCounts = BTreeMap<&'static str, usize>;

/// In-place mutation of a single string. Each match of any rule's
/// regex is replaced — whole match for Phase-1 rules
/// (`rule.capture == None`) or only the captured group for Phase-2+
/// rules (`rule.capture == Some(g)`) — with the rule's token shape
/// (`<PATH:home:<8hex>>` / `<USER:<8hex>>` / `<ORG:<8hex>>`).
/// The pseudonym is derived from the **replaced** substring, so the
/// same captured text under the same `rule_id` yields the same
/// token within a cassette (cache hit).
///
/// Returns per-rule replacement counts (Phase 2 of #42, carved per
/// #242). Total is `.values().sum()`.
pub fn anonymize_string(rules: &[AnonRule], p: &mut Pseudonymizer, s: &mut String) -> RuleCounts {
    let mut counts: RuleCounts = BTreeMap::new();
    for rule in rules {
        // Collect (replace_start, replace_end, key_substring) spans.
        // `key_substring` is what we feed the pseudonymizer and is
        // also what gets written back into the string. For whole-
        // match rules the two are the entire match; for capture-
        // group rules they're the captured slice.
        let spans: Vec<(usize, usize, String)> = match rule.capture {
            None => rule
                .regex
                .find_iter(s)
                .map(|m| (m.start(), m.end(), m.as_str().to_owned()))
                .collect(),
            Some(group) => rule
                .regex
                .captures_iter(s)
                .filter_map(|c| {
                    c.get(group as usize)
                        .map(|m| (m.start(), m.end(), m.as_str().to_owned()))
                })
                .collect(),
        };
        if spans.is_empty() {
            continue;
        }
        // Walk right-to-left so byte offsets stay valid while we
        // splice. Multi-rule cascading is already supported (different
        // rules can match overlapping substrings; the right-to-left
        // pass per rule keeps each pass's offsets internally
        // consistent).
        let mut new_s = s.clone();
        let count_for_rule = counts.entry(rule.id).or_insert(0);
        for (start, end, key) in spans.into_iter().rev() {
            let pseudo = p.pseudonym(rule.id, &key);
            let token = render_token(rule.id, &pseudo);
            new_s.replace_range(start..end, &token);
            *count_for_rule += 1;
        }
        *s = new_s;
    }
    counts
}

/// Recursively walk a JSON Value, mutating every string field. Mirrors
/// the shape of `tape_redact::Engine::redact_value` (a parallel walker
/// per open Q1).
pub fn anonymize_value(rules: &[AnonRule], p: &mut Pseudonymizer, value: &mut Value) -> RuleCounts {
    let mut counts: RuleCounts = BTreeMap::new();
    walk(value, &mut |s| {
        for (k, v) in anonymize_string(rules, p, s) {
            *counts.entry(k).or_insert(0) += v;
        }
    });
    counts
}

/// Merge `b` into `a` in place, summing values per key.
pub fn merge_counts(a: &mut RuleCounts, b: RuleCounts) {
    for (k, v) in b {
        *a.entry(k).or_insert(0) += v;
    }
}

/// Total replacements across all rules.
#[must_use]
pub fn total(counts: &RuleCounts) -> usize {
    counts.values().sum()
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
/// Phase 2 of #42 (carved per #242) adds `<USER:8hex>` for
/// `unix_username_prompt` and `<ORG:8hex>` for `git_remote_user`,
/// alongside Phase-1's `<PATH:home:8hex>` for `unix_home_path`.
/// The defensive `<ANON:rule:8hex>` default catches any future rule
/// the matcher knows about but the renderer hasn't gotten an arm for
/// yet — that should be a build-time fix when a Phase-3 rule lands.
fn render_token(rule_id: &str, pseudonym: &str) -> String {
    match rule_id {
        "unix_home_path" => format!("<PATH:home:{pseudonym}>"),
        "unix_username_prompt" => format!("<USER:{pseudonym}>"),
        "git_remote_user" => format!("<ORG:{pseudonym}>"),
        // Defensive default — Phase 3+ rules add explicit arms here.
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
        let counts = anonymize_string(&rules, &mut p, &mut s);
        assert_eq!(total(&counts), 2);
        assert_eq!(counts.get("unix_home_path").copied(), Some(2));
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
        let counts = anonymize_value(&rules, &mut p, &mut v);
        // 5 matches: path, cmd, tags[0], tags[1], nested.deep
        assert_eq!(total(&counts), 5);
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
        let counts = anonymize_string(&rules, &mut p, &mut s);
        assert_eq!(total(&counts), 0);
        assert_eq!(s, before);
    }

    #[test]
    fn render_token_shape() {
        assert_eq!(
            render_token("unix_home_path", "deadbeef"),
            "<PATH:home:deadbeef>"
        );
        assert_eq!(
            render_token("unix_username_prompt", "deadbeef"),
            "<USER:deadbeef>"
        );
        assert_eq!(
            render_token("git_remote_user", "deadbeef"),
            "<ORG:deadbeef>"
        );
    }

    // =====================================================================
    // Phase 2 (#242): capture-group narrowing + cross-rule cascade.
    // =====================================================================

    #[test]
    fn capture_group_narrows_to_username_span_only() {
        // The shell-prompt rule should replace ONLY the `colin`
        // span; the `@macbook:~/work$ ls` tail must be preserved
        // verbatim.
        let rules = built_in_rules();
        let mut p = fixed_p();
        let mut s = String::from("colin@macbook:~/work$ ls");
        let counts = anonymize_string(&rules, &mut p, &mut s);
        assert_eq!(counts.get("unix_username_prompt").copied(), Some(1));
        let token = render_token(
            "unix_username_prompt",
            &p.pseudonym("unix_username_prompt", "colin"),
        );
        assert_eq!(s, format!("{token}@macbook:~/work$ ls"));
    }

    #[test]
    fn capture_group_cache_hits_on_captured_slice() {
        // Two shell prompts with the same `colin` username should
        // derive the SAME `<USER:…>` token (cache key is the
        // captured substring, not the whole match).
        let rules = built_in_rules();
        let mut p = fixed_p();
        let mut s = String::from("colin@macbook:~/work$ ls\nthen later colin@laptop:/etc$ pwd");
        anonymize_string(&rules, &mut p, &mut s);
        let token = render_token(
            "unix_username_prompt",
            &p.pseudonym("unix_username_prompt", "colin"),
        );
        // Both occurrences should carry the same token.
        assert_eq!(s.matches(&token).count(), 2, "got: {s}");
    }

    #[test]
    fn two_rule_cascade_one_line_with_both_shapes() {
        // A single string with one shell-prompt match and one git-
        // remote match. After anon both shapes appear with distinct
        // tokens (different rule_ids → different cache keys → different
        // pseudonyms, even if the captured substrings happen to share
        // text).
        let rules = built_in_rules();
        let mut p = fixed_p();
        let mut s =
            String::from("colin@m:~$ git remote -v\norigin\tgit@github.com:colinc86/tape (fetch)");
        let counts = anonymize_string(&rules, &mut p, &mut s);
        assert_eq!(counts.get("unix_username_prompt").copied(), Some(1));
        assert_eq!(counts.get("git_remote_user").copied(), Some(1));
        assert!(!s.contains("colin@m:"), "shell-prompt user leaked: {s}");
        assert!(!s.contains("colinc86"), "git-remote user leaked: {s}");
        assert!(s.contains("<USER:"), "missing USER token: {s}");
        assert!(s.contains("<ORG:"), "missing ORG token: {s}");
    }

    #[test]
    fn git_remote_user_preserves_prefix_and_tail() {
        let rules = built_in_rules();
        let mut p = fixed_p();
        let mut s = String::from("git@github.com:colinc86/tape.git");
        anonymize_string(&rules, &mut p, &mut s);
        let token = render_token(
            "git_remote_user",
            &p.pseudonym("git_remote_user", "colinc86"),
        );
        // Prefix `git@github.com:` and tail `/tape.git` are
        // preserved; only the user/org slot is replaced.
        assert_eq!(s, format!("git@github.com:{token}/tape.git"));
    }
}
