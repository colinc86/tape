//! Built-in anonymization rules. Phase 2 of issue #42 ships three:
//! `unix_home_path` (Phase 1), `unix_username_prompt`, and
//! `git_remote_user`. Phase 3+ slices append additional rule classes
//! (`windows_user_path`, `hostname_meta`, etc.) per the table in
//! issue #42 §3.2.

use regex::Regex;

/// An anonymization rule. Deliberately narrower than
/// `tape_redact::Rule` — anon replacements are per-match (derived from
/// the matched bytes via HMAC) rather than the static placeholder
/// strings the redact engine uses, so the redact `Rule.replacement`
/// field is meaningless here. Per the ticket's open Q1 the
/// implementation chooses option (b): a parallel walker with
/// its own rule type, keeping the blast radius inside `tape-anon`.
///
/// Phase 2 adds `capture: Option<u32>`. `None` (Phase-1 default) means
/// replace the whole match — that's what `unix_home_path` wants. `Some(g)`
/// means replace only capture group `g`'s span — used by the new
/// `unix_username_prompt` rule (replace only the username portion of a
/// shell-prompt prefix) and `git_remote_user` (replace only the
/// user/org segment of a git remote URL).
#[derive(Debug, Clone)]
pub struct AnonRule {
    pub id: &'static str,
    pub regex: Regex,
    /// Replacement target. `None` replaces the whole match (Phase-1
    /// shape, used by `unix_home_path`). `Some(g)` replaces only
    /// capture group `g`'s span, preserving surrounding context
    /// (Phase-2 shape, used by `unix_username_prompt` and
    /// `git_remote_user`). The pseudonym is derived from the
    /// replaced substring (the whole match or the captured slice
    /// respectively), so same captured text under the same rule_id
    /// yields the same token within a cassette (cache hit).
    pub capture: Option<u32>,
}

/// Phase-2 rule set, stable order: `unix_home_path`,
/// `unix_username_prompt`, `git_remote_user`. The CLI's stderr
/// success line enumerates per-rule counts in this order.
#[must_use]
pub fn built_in_rules() -> Vec<AnonRule> {
    vec![unix_home_path(), unix_username_prompt(), git_remote_user()]
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
        capture: None,
    }
}

/// `unix_username_prompt` (issue #42 §3.2 row 3, carved into Phase 2
/// per #242).
///
/// Matches shell-prompt prefixes of the form `<user>@<host>:<cwd>$ `
/// where `<user>` is `[a-z][a-z0-9._-]*` (same char class as
/// `unix_home_path`). The capture group is the username only;
/// `@<host>:<cwd>$ ` is preserved verbatim in the output so a line
/// like `colin@macbook ~/work $ ls` becomes
/// `<USER:a1b2c3d4>@macbook ~/work $ ls`.
///
/// The `\b` anchor at the start and the `:\S*\$\s` suffix combine to
/// filter out:
/// - email addresses (no `$ ` follows them — `user@example.com` is
///   safe);
/// - URL `user@host` segments (no `:cwd $ ` follows — `git@github.com:org/repo`
///   is left for `git_remote_user` to handle);
/// - CLI args that happen to use `@` (no `:` between `@` and `$ `).
///
/// Phase-2 deliberate non-coverage:
/// - Root prompts ending in `#` instead of `$` — table pattern uses
///   `\$\s` literally. A `unix_root_prompt` row could be added in a
///   later phase; treat as a separate identifier class because root's
///   username is often the literal `root` rather than the operator's
///   personal username.
/// - Zsh prompts with custom themes (no `host:cwd $ ` shape). Same
///   trade-off as Phase-1 false-negatives — preferred over
///   false-positives in command output.
/// - Windows-style prompts (`PS C:\>`). Out of #42 §3.2 entirely;
///   the path portion is handled by `windows_user_path` in a future
///   phase.
#[must_use]
pub fn unix_username_prompt() -> AnonRule {
    AnonRule {
        id: "unix_username_prompt",
        // \b username @ host : cwd $ <whitespace>. The host class
        // allows letters/digits/`-`/`.`; cwd is any non-whitespace
        // (so `~/work`, `/etc`, `.` all match).
        regex: Regex::new(r"\b([a-z][a-z0-9._-]*)@[a-zA-Z0-9.-]+:\S*\$\s").unwrap(),
        capture: Some(1),
    }
}

/// `git_remote_user` (issue #42 §3.2 row 4, carved into Phase 2 per
/// #242).
///
/// Matches the user/org segment of GitHub/GitLab/Bitbucket SSH and
/// HTTPS remote URLs, plus the org segment of Azure DevOps HTTPS
/// URLs. Capture group 1 is the user/org; the
/// `git@<host>:` / `https://<host>/` prefix and the `/<rest>` tail
/// are preserved verbatim. So `git@github.com:colinc86/tape`
/// becomes `git@github.com:<ORG:a1b2c3d4>/tape`.
///
/// Phase-2 deliberate non-coverage:
/// - Self-hosted GitLab / Gitea / Forgejo / Codeberg at arbitrary
///   domains. The host list is closed in Phase 2; per-`.taperc` host
///   extension is Phase-4 user-rules work.
/// - `git://` protocol — effectively dead, skip.
/// - `ssh://git@host/user/repo` — third URL form, not in the §3.2
///   table; deferred.
/// - Azure DevOps SSH (`git@ssh.dev.azure.com:v3/<org>/...`) — the
///   prefix structure differs from the other hosts. Azure HTTPS
///   (`https://dev.azure.com/<org>/...`) IS covered since the
///   user/org is still the first path segment.
///
/// Documented choice: GitHub usernames are case-insensitive at the
/// API layer but case-preserving in URLs; the `[^/]+` character class
/// preserves case, so `colinC86` and `colinc86` derive different
/// pseudonyms. This is the Phase-2 trade-off.
#[must_use]
pub fn git_remote_user() -> AnonRule {
    // Combined SSH + HTTPS regex. Non-capturing alternation on the
    // host list keeps capture group 1 pinned to the user/org slot
    // regardless of which branch matches. SSH uses the
    // `git@<host>:user/repo` form across all three hosts; HTTPS adds
    // dev.azure.com for the four-host coverage.
    // `[^/<>]+` excludes `<` and `>` from the user/org class so the
    // defense-in-depth post-anon re-scan doesn't re-fire on a
    // freshly-substituted `<ORG:8hex>` token (whose surrounding
    // `git@github.com:<ORG:…>/repo` still otherwise satisfies the
    // pattern). Real user/org slugs can't contain `<` or `>` on any
    // listed host.
    let pattern = r"(?:git@(?:github\.com|gitlab\.com|bitbucket\.org):|https://(?:github\.com|gitlab\.com|bitbucket\.org|dev\.azure\.com)/)([^/<>]+)/[^ \t\n]+";
    AnonRule {
        id: "git_remote_user",
        regex: Regex::new(pattern).unwrap(),
        capture: Some(1),
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

#[cfg(test)]
mod unix_username_prompt_tests {
    use super::*;

    fn captured(s: &str) -> Option<String> {
        let rule = unix_username_prompt();
        rule.regex
            .captures(s)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_owned()))
    }

    fn matches(s: &str) -> bool {
        unix_username_prompt().regex.is_match(s)
    }

    // --- Positives (≥5). All use the standard bash-style
    //     `<user>@<host>:<cwd>$ <cmd>` shape per the rule's regex.

    #[test]
    fn matches_simple_prompt() {
        // Standard bash PS1 (`\u@\h:\w\$ `) on Linux.
        assert_eq!(
            captured("colin@macbook:~/work$ ls").as_deref(),
            Some("colin")
        );
    }

    #[test]
    fn matches_with_etc_cwd() {
        assert_eq!(captured("alice@host:/etc$ pwd").as_deref(), Some("alice"));
    }

    #[test]
    fn matches_with_tilde_cwd_and_space_before_dollar() {
        // Some themes render a space before the `$` — `\S*\$` still
        // matches because `~` (one char) consumes the `\S*` greedily,
        // leaving the space to satisfy `\s` *after* the `$`.
        assert_eq!(captured("bob@dev-vm:~$ true").as_deref(), Some("bob"));
    }

    #[test]
    fn matches_one_char_username() {
        assert_eq!(captured("c@h:.$ cd").as_deref(), Some("c"));
    }

    #[test]
    fn matches_username_with_punctuation() {
        assert_eq!(
            captured("user.with-dots@host:cwd$ x").as_deref(),
            Some("user.with-dots")
        );
    }

    #[test]
    fn matches_with_tab_terminator() {
        // `\s` matches any whitespace incl. tabs.
        assert_eq!(captured("colin@m:~$\tls").as_deref(), Some("colin"));
    }

    // --- Negatives (≥5) ---

    #[test]
    fn rejects_email_address() {
        // No `$ ` suffix → email is safe from this rule.
        assert!(!matches("user@example.com"));
    }

    #[test]
    fn rejects_url_user_at_host() {
        // No `:cwd $ ` shape after the host.
        assert!(!matches("https://example.com/user@thing"));
    }

    #[test]
    fn rejects_git_remote_user_form() {
        // git@github.com:org/repo — has `@host:` but no `$ ` suffix.
        // (git_remote_user handles this shape.)
        assert!(!matches("git@github.com:colin/repo"));
    }

    #[test]
    fn rejects_plain_word_with_no_at() {
        assert!(!matches("not.an.email"));
    }

    #[test]
    fn rejects_empty_username_before_at() {
        assert!(!matches("@host:cwd$ "));
    }

    #[test]
    fn rejects_root_hash_prompt() {
        // Root `#` prompts are out of Phase-2 scope (would be a
        // separate unix_root_prompt rule).
        assert!(!matches("root@m:~# ls"));
    }
}

#[cfg(test)]
mod git_remote_user_tests {
    use super::*;

    fn captured(s: &str) -> Option<String> {
        let rule = git_remote_user();
        rule.regex
            .captures(s)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_owned()))
    }

    fn matches(s: &str) -> bool {
        git_remote_user().regex.is_match(s)
    }

    // --- Positives (≥5) ---

    #[test]
    fn matches_github_ssh() {
        assert_eq!(
            captured("git@github.com:colinc86/tape").as_deref(),
            Some("colinc86")
        );
    }

    #[test]
    fn matches_gitlab_ssh_with_dot_git() {
        assert_eq!(
            captured("git@gitlab.com:acme/widget.git").as_deref(),
            Some("acme")
        );
    }

    #[test]
    fn matches_github_https() {
        assert_eq!(
            captured("https://github.com/torvalds/linux").as_deref(),
            Some("torvalds")
        );
    }

    #[test]
    fn matches_bitbucket_https() {
        assert_eq!(
            captured("https://bitbucket.org/atlassian/something").as_deref(),
            Some("atlassian")
        );
    }

    #[test]
    fn matches_azure_https() {
        assert_eq!(
            captured("https://dev.azure.com/contoso/project/_git/repo").as_deref(),
            Some("contoso")
        );
    }

    #[test]
    fn matches_dot_git_suffix_preserved_in_tail() {
        // The tail captures the rest of the URL via `[^ \t\n]+`,
        // so `.git` ends up in the un-replaced tail.
        let s = "git@github.com:user/repo.git";
        assert_eq!(captured(s).as_deref(), Some("user"));
    }

    // --- Negatives (≥5) ---

    #[test]
    fn rejects_https_github_without_user() {
        // `https://github.com/` — no `<user>/<repo>` after the host.
        assert!(!matches("https://github.com/"));
    }

    #[test]
    fn rejects_ssh_without_user_repo_tail() {
        // `git@github.com` — no `:user/repo`.
        assert!(!matches("git@github.com"));
    }

    #[test]
    fn rejects_non_listed_host() {
        // Self-hosted GitLab / unknown forge.
        assert!(!matches("https://git.example.com/foo/bar"));
    }

    #[test]
    fn rejects_git_protocol() {
        // `git://` — deprecated, explicitly out of scope.
        assert!(!matches("git://github.com/user/repo"));
    }

    #[test]
    fn rejects_ssh_url_protocol_form() {
        // `ssh://git@github.com/user/repo` — third URL form, not in
        // §3.2; deferred.
        assert!(!matches("ssh://git@github.com/user/repo"));
    }

    #[test]
    fn case_sensitivity_documented() {
        // Documented choice: case is preserved, so `User` and `user`
        // derive different pseudonyms. The match still succeeds for
        // mixed-case usernames.
        assert_eq!(
            captured("git@github.com:ColinC86/tape").as_deref(),
            Some("ColinC86")
        );
    }
}
