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
        Box::new(checks::permissions::TmpdirWritable),
        Box::new(checks::permissions::ClaudeDirWritable),
        // Step-2 of #81 (issue #163): claude-code soft-dependency checks.
        // Warn-severity; never escalates the exit code without --strict
        // (which is also deferred).
        Box::new(checks::claude_code::ClaudeInstalled),
        Box::new(checks::claude_code::ClaudePluginEnabled),
    ]
}

/// Doctor category list, in display order. Distinct from the catalog
/// because the category-level header (`signing  ⊘ n/a`) needs to appear
/// even when no checks land in that category in this phase. Name is
/// grandfathered from Phase 1; functions as a phase-agnostic display
/// order today.
pub const PHASE_1_CATEGORIES: &[&str] = &["binary", "config", "permissions", "claude-code"];
