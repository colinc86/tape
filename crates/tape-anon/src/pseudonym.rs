//! Per-cassette pseudonym derivation. See issue #42 §3.3.
//!
//! On engine construction, draws a 32-byte salt via `getrandom`. For
//! each match `s`, computes
//! `HMAC-SHA256(salt, rule_id || 0x1F || s)`, takes the first 4 bytes,
//! and hex-encodes them as 8 lowercase characters. Identical
//! `(rule_id, s)` triples within a single run return the cached value
//! so the substitution is stable within the cassette; the cache is
//! discarded at end-of-run. The salt is explicitly zeroed on `Drop`
//! per the ticket's open Q2 recommendation (no `zeroize` dep in Phase
//! 1; pulled in Phase 3 when the secret-handling surface grows).

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

const SALT_LEN: usize = 32;

/// Salt-bearing pseudonym derivation cache. Construct once per `run_anon`
/// invocation. Cross-run salts are not stable (each construction draws
/// fresh random bytes), so cross-cassette correlation is broken by
/// construction; within-cassette consistency comes from the cache.
pub struct Pseudonymizer {
    salt: [u8; SALT_LEN],
    cache: HashMap<(String, String), String>,
}

impl std::fmt::Debug for Pseudonymizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Do NOT leak salt bytes through Debug.
        f.debug_struct("Pseudonymizer")
            .field("salt", &"[redacted]")
            .field("cache_entries", &self.cache.len())
            .finish()
    }
}

impl Pseudonymizer {
    /// Construct with a fresh 32-byte random salt. Returns an error if
    /// the OS RNG fails — vanishingly rare; surfaced rather than panic'd
    /// because the CLI layer wants to exit 2 with a structured error.
    pub fn new() -> anyhow::Result<Self> {
        let mut salt = [0u8; SALT_LEN];
        getrandom::getrandom(&mut salt)
            .map_err(|e| anyhow::anyhow!("tape anon: OS RNG failure deriving salt: {e}"))?;
        Ok(Self {
            salt,
            cache: HashMap::new(),
        })
    }

    /// Construct with a caller-supplied salt. Only used by tests today;
    /// `--salt` CLI flag for deterministic salts is Phase 2+ work.
    #[must_use]
    pub fn with_salt(salt: [u8; SALT_LEN]) -> Self {
        Self {
            salt,
            cache: HashMap::new(),
        }
    }

    /// Derive (or return cached) 8-hex pseudonym for `(rule_id, matched)`.
    pub fn pseudonym(&mut self, rule_id: &str, matched: &str) -> String {
        let key = (rule_id.to_owned(), matched.to_owned());
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let derived = derive_pseudonym(&self.salt, rule_id, matched);
        self.cache.insert(key, derived.clone());
        derived
    }
}

impl Drop for Pseudonymizer {
    fn drop(&mut self) {
        // Explicit zero-fill per ticket open Q2 — Phase 1 avoids
        // pulling in the `zeroize` crate; this is functionally
        // equivalent for the in-memory window between use and drop.
        self.salt.fill(0);
    }
}

/// Stateless HMAC derivation. Exposed for the cache-hit test which
/// needs to verify the cache returns the cached value rather than
/// re-deriving.
fn derive_pseudonym(salt: &[u8; SALT_LEN], rule_id: &str, matched: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(salt).expect("HMAC accepts any key length");
    mac.update(rule_id.as_bytes());
    mac.update(&[0x1F]); // unit separator per #42 §3.3
    mac.update(matched.as_bytes());
    let digest = mac.finalize().into_bytes();
    // First 4 bytes → 8 lowercase hex chars.
    let mut out = String::with_capacity(8);
    for b in &digest[..4] {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const FIXED_SALT: [u8; 32] = [0x42; 32];

    #[test]
    fn same_triple_produces_identical_output() {
        // Property: derivation is deterministic for a fixed
        // (salt, rule_id, matched).
        let a = derive_pseudonym(&FIXED_SALT, "unix_home_path", "/Users/colin");
        let b = derive_pseudonym(&FIXED_SALT, "unix_home_path", "/Users/colin");
        assert_eq!(a, b);
        assert_eq!(a.len(), 8);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn distinct_matched_strings_produce_distinct_outputs_at_scale() {
        // Property: 1000 distinct inputs should all map to distinct
        // 8-hex outputs (no collisions in this sample size, since
        // 16^8 = 4.3B > 1000 * 1000 birthday-pairs).
        let mut seen = HashSet::new();
        for i in 0..1000 {
            let s = format!("/Users/u{i}");
            let out = derive_pseudonym(&FIXED_SALT, "unix_home_path", &s);
            assert!(seen.insert(out.clone()), "collision at {s} → {out}");
        }
    }

    #[test]
    fn distinct_salts_produce_distinct_outputs_for_same_input() {
        let a = derive_pseudonym(&[0x01; 32], "unix_home_path", "/Users/colin");
        let b = derive_pseudonym(&[0x02; 32], "unix_home_path", "/Users/colin");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_returns_cached_value_not_re_derivation() {
        // Inject a sentinel value into the cache and verify
        // `pseudonym()` returns it instead of computing fresh.
        let mut p = Pseudonymizer::with_salt(FIXED_SALT);
        let key = ("unix_home_path".to_owned(), "/Users/colin".to_owned());
        p.cache.insert(key.clone(), "deadbeef".to_owned());
        assert_eq!(p.pseudonym("unix_home_path", "/Users/colin"), "deadbeef");
    }

    #[test]
    fn pseudonymizer_with_random_salt_is_constructible() {
        let _ = Pseudonymizer::new().unwrap();
    }

    #[test]
    fn unit_separator_byte_prevents_concat_collision() {
        // Without the 0x1F separator, rule_id="ab" + matched="cd"
        // would HMAC-hash the same bytes as rule_id="a" + matched="bcd".
        // The 0x1F breaks that ambiguity.
        let with_split_1 = derive_pseudonym(&FIXED_SALT, "ab", "cd");
        let with_split_2 = derive_pseudonym(&FIXED_SALT, "a", "bcd");
        assert_ne!(with_split_1, with_split_2);
    }

    #[test]
    fn cross_rule_same_substring_yields_distinct_pseudonyms() {
        // Phase 2 of #42 (carved per #242) regression guard for the
        // open-question resolution in the ticket: the cache key
        // remains `(rule_id, matched)`, so `colin` under
        // `unix_username_prompt` and `colin` under `git_remote_user`
        // derive DIFFERENT pseudonyms. The visible token shapes are
        // different anyway (`<USER:…>` vs `<ORG:…>`), but pinning
        // this here prevents a future refactor from silently
        // changing the cross-rule correlation property.
        let mut p = Pseudonymizer::with_salt([0x42; 32]);
        let a = p.pseudonym("unix_username_prompt", "colin");
        let b = p.pseudonym("git_remote_user", "colin");
        assert_ne!(a, b);
    }
}
