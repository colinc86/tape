//! Doctor check catalog.
//!
//! Adding a new check is a one-line edit here. The order in this vector is
//! the order checks are *executed* and *reported*; keep it stable so the
//! `--list-checks` snapshot is stable.

use super::check::Check;
use super::checks;

/// The stable doctor catalog. Order matters — `--list-checks` snapshots
/// against this exact sequence.
pub fn phase_1_checks() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(checks::binary::TapePresent),
        Box::new(checks::binary::TapeHookPresent),
        Box::new(checks::binary::TapeMcpWrapPresent),
        Box::new(checks::binary::TapeVersion),
        Box::new(checks::config::UserTaperc),
        Box::new(checks::config::WorkspaceTaperc),
        Box::new(checks::config::RuleIdsValid),
        // Issue #186 Acceptance #5: `.taperc::pricing.pricing_file`
        // surface. `Warn`-severity — a broken configured pricing file
        // does not block recording, only `tape stats --with-cost`.
        Box::new(checks::config::ConfiguredPricingFile),
        Box::new(checks::permissions::TmpdirWritable),
        Box::new(checks::permissions::ClaudeDirWritable),
        // Step-2 of #81 (issue #163): claude-code soft-dependency checks.
        // Warn-severity; never escalates the exit code without --strict
        // (which is also deferred).
        Box::new(checks::claude_code::ClaudeInstalled),
        Box::new(checks::claude_code::ClaudePluginEnabled),
        // Step-3 of #81 (issue #166): signing keystore + trust-store
        // checks. All three are `Warn`-severity soft-dependencies; the
        // exit code never escalates without `--strict` (deferred).
        // Each surfaces `Na` until #18 (`tape sign`) lands a real
        // keystore.
        Box::new(checks::signing::KeystoreReadable),
        Box::new(checks::signing::KeystorePerms),
        Box::new(checks::signing::TrustStoreReadable),
        // Step-4 of #81 (issue #177): bundled-pricing-table freshness.
        // One `Warn`-severity check, real-not-`Na` from day one since
        // the pricing table is compiled into the binary.
        Box::new(checks::pricing::TableFresh),
        // Step-5 of #81 (issue #183): local-library `index.*` checks.
        // Four soft-dependency checks; all surface `Na` until #2's
        // SQLite layer ships. Severity-on-fail mix is `Warn` /
        // `Fail` per §3.2 (integrity is `Fail`, the rest `Warn`).
        Box::new(checks::index::Exists),
        Box::new(checks::index::SqliteIntegrity),
        Box::new(checks::index::LockStale),
        Box::new(checks::index::LastRescanFresh),
    ]
}

/// Doctor category list, in display order. Distinct from the catalog
/// because the category-level header (`signing  ⊘ n/a`) needs to appear
/// even when no checks land in that category in this phase. Name is
/// grandfathered from Phase 1; functions as a phase-agnostic display
/// order today.
pub const PHASE_1_CATEGORIES: &[&str] = &[
    "binary",
    "config",
    "permissions",
    "claude-code",
    "signing",
    "pricing",
    "index",
];
