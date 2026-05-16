//! Built-in anonymization rules. Phase 1 of issue #42 ships exactly one
//! (`unix_home_path`). Phase 2+ slices append additional rule classes
//! (`windows_user_path`, `unix_username_prompt`, etc.) per the table in
//! issue #42 §3.2.

use regex::Regex;

/// A Phase-1 anonymization rule. Deliberately narrower than
/// `tape_redact::Rule` — anon replacements are per-match (derived from
/// the matched bytes via HMAC) rather than the static placeholder
/// strings the redact engine uses, so the redact `Rule.replacement`
/// field is meaningless here. Per the ticket's open Q1 the
/// Phase-1 implementation chooses option (b): a parallel walker with
/// its own rule type, keeping the blast radius inside `tape-anon`.
#[derive(Debug, Clone)]
pub struct AnonRule {
    pub id: &'static str,
    pub regex: Regex,
}

/// Phase-1 rule set. Length 1. Phase 2+ extends this list.
#[must_use]
pub fn built_in_rules() -> Vec<AnonRule> {
    vec![unix_home_path()]
}

/// `unix_home_path` (issue #42 §3.2 first row).
///
/// Matches `/Users/<u>` and `/home/<u>` where `<u>` is
/// `[a-z][a-z0-9._-]*` (per #42 §3.2 char class — usernames starting
/// with a letter, then any of `a-z0-9._-`). The bare `/root` literal
/// is matched separately since root's home is just `/root`.
///
/// Whole-match replacement covers ONLY the home-dir prefix (e.g.
/// `/Users/colin`), not the path tail. The engine slices the
/// replacement into the original string so a path like
/// `/Users/colin/work/billing/x.rs` becomes
/// `<PATH:home:a1b2c3d4>/work/billing/x.rs` with the `/work/billing/x.rs`
/// suffix preserved.
///
/// Phase-1 deliberate non-coverage:
/// - Uppercase usernames (`/Users/Colin`) — char class is lowercase
///   per the #42 table; macOS Home Directories defaults to lowercase.
/// - Digit-leading usernames (`/Users/123colin`) — same.
/// - Embedded matches (`xyz/Users/colin`) — no word-boundary anchor
///   in Phase 1; this WILL match `/Users/colin`. False-positive cost
///   on this is lower than the false-negative cost of missing a real
///   home path embedded in a longer string (e.g. a shell history line
///   containing `cd /Users/colin/...`).
#[must_use]
pub fn unix_home_path() -> AnonRule {
    AnonRule {
        id: "unix_home_path",
        regex: Regex::new(r"(?:/Users/[a-z][a-z0-9._-]*|/home/[a-z][a-z0-9._-]*|/root)").unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(s: &str) -> bool {
        unix_home_path().regex.is_match(s)
    }

    fn first_match(s: &str) -> Option<String> {
        unix_home_path()
            .regex
            .find(s)
            .map(|m| m.as_str().to_owned())
    }

    // --- Positives (≥5 per test plan) ---

    #[test]
    fn matches_users_with_simple_username() {
        assert_eq!(
            first_match("/Users/colin/work/x.rs").as_deref(),
            Some("/Users/colin")
        );
    }

    #[test]
    fn matches_users_with_dotfile_continuation() {
        assert_eq!(
            first_match("/Users/alice/.bashrc").as_deref(),
            Some("/Users/alice")
        );
    }

    #[test]
    fn matches_home_with_simple_username() {
        assert_eq!(first_match("/home/bob/repo").as_deref(), Some("/home/bob"));
    }

    #[test]
    fn matches_home_with_one_char_username() {
        assert_eq!(first_match("/home/c/d").as_deref(), Some("/home/c"));
    }

    #[test]
    fn matches_bare_root() {
        assert_eq!(first_match("/root").as_deref(), Some("/root"));
    }

    #[test]
    fn matches_root_with_continuation() {
        assert_eq!(first_match("/root/.ssh/config").as_deref(), Some("/root"));
    }

    #[test]
    fn matches_username_with_punctuation_chars() {
        assert_eq!(
            first_match("/Users/a.b-c_d/x").as_deref(),
            Some("/Users/a.b-c_d")
        );
    }

    #[test]
    fn matches_at_string_end_no_continuation() {
        assert_eq!(first_match("/Users/colin").as_deref(), Some("/Users/colin"));
    }

    // --- Negatives (≥5 per test plan) ---

    #[test]
    fn rejects_usr_local() {
        assert!(!matches("/usr/local/bin"));
    }

    #[test]
    fn rejects_var_log() {
        assert!(!matches("/var/log/foo"));
    }

    #[test]
    fn rejects_opt_homebrew() {
        assert!(!matches("/opt/homebrew/bin"));
    }

    #[test]
    fn rejects_lowercase_users_path() {
        // `/users/colin` — lowercase `/users` is not in the Phase-1
        // rule (only `/Users` and `/home` for unix-home and the bare
        // `/root` literal).
        assert!(!matches("/users/colin"));
    }

    #[test]
    fn rejects_users_with_no_username() {
        assert!(!matches("/Users/"));
    }

    #[test]
    fn rejects_users_with_uppercase_username() {
        // `/Users/Colin/x` — char class is `[a-z][a-z0-9._-]*` per the
        // ticket; uppercase usernames are out of scope for Phase 1.
        assert!(!matches("/Users/Colin/x"));
    }

    #[test]
    fn rejects_users_with_leading_digit_username() {
        // `/Users/123colin` — char class requires `[a-z]` first.
        assert!(!matches("/Users/123colin"));
    }

    // --- Documented choice: embedded matches DO scrub ---

    #[test]
    fn embedded_users_path_does_match() {
        // `xyz/Users/colin` — no word-boundary in Phase 1, so this
        // matches. Documented in the rule's doc comment as the
        // deliberate Phase-1 trade-off (false-positive on `xyz` cost
        // < false-negative on a real shell-history line containing a
        // home path).
        assert_eq!(
            first_match("xyz/Users/colin/work").as_deref(),
            Some("/Users/colin")
        );
    }
}
