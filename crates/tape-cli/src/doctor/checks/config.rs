//! `config.*` — `.taperc` parse + rule-id validity.
//!
//! `.taperc` is loaded twice in `tape record`: workspace `.taperc` (CWD walk
//! up to `$HOME`) takes precedence over `~/.taperc`. Doctor reports on each
//! independently so the user sees which file is at fault.

use std::path::PathBuf;

use tape_redact::config::TapeRcConfig;

use super::super::check::{Check, CheckOutcome, Env, Severity};

pub struct UserTaperc;
impl Check for UserTaperc {
    fn id(&self) -> &'static str {
        "config.user_taperc.parses"
    }
    fn category(&self) -> &'static str {
        "config"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "~/.taperc parses as valid YAML (or is absent)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        let Some(home) = env.home.as_ref() else {
            return CheckOutcome::na("$HOME is not set");
        };
        let path = home.join(".taperc");
        check_taperc_path(&path)
    }
}

pub struct WorkspaceTaperc;
impl Check for WorkspaceTaperc {
    fn id(&self) -> &'static str {
        "config.workspace_taperc.parses"
    }
    fn category(&self) -> &'static str {
        "config"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "./.taperc (and ancestors up to $HOME) parses as valid YAML (or is absent)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        let Some(path) = locate_workspace(&env.cwd, env.home.as_deref()) else {
            return CheckOutcome::pass("no workspace .taperc found (ok)");
        };
        check_taperc_path(&path)
    }
}

/// Every `enable_optional`, `disable_default`, and custom rule id in the
/// loaded `.taperc` files must resolve against `tape_redact::rules::built_in`.
/// We reuse the existing `TapeRcConfig::apply` plumbing rather than
/// reimplement the validation — it already surfaces the right errors.
pub struct RuleIdsValid;
impl Check for RuleIdsValid {
    fn id(&self) -> &'static str {
        "config.rule_ids.valid"
    }
    fn category(&self) -> &'static str {
        "config"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "every redaction rule id in .taperc resolves against the built-in catalog"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        // Workspace .taperc wins over user .taperc, mirroring
        // `tape_redact::config::engine_with_taperc`.
        let workspace = locate_workspace(&env.cwd, env.home.as_deref());
        let user = env
            .home
            .as_ref()
            .map(|h| h.join(".taperc"))
            .filter(|p| p.is_file());
        let path = workspace.or(user);
        let Some(path) = path else {
            return CheckOutcome::pass("no .taperc found (ok)");
        };

        let yaml = match std::fs::read_to_string(&path) {
            Ok(y) => y,
            Err(e) => {
                return CheckOutcome::na(format!(
                    "could not read {} ({e}); see config.*_taperc.parses",
                    path.display()
                ));
            }
        };
        let Ok(cfg) = TapeRcConfig::parse(&yaml) else {
            // The parse-failure check (`config.*_taperc.parses`) already
            // surfaces this as a `fail`. Don't double-fail.
            return CheckOutcome::na(format!(
                "{} did not parse; see config.*_taperc.parses",
                path.display()
            ));
        };
        let mut engine = tape_redact::Engine::with_default_rules();
        match cfg.apply(&mut engine) {
            Ok(()) => CheckOutcome::pass(format!(
                "all rule ids in {} resolve",
                path.display()
            )),
            Err(e) => CheckOutcome::fail(format!("{}: {e}", path.display())).with_fix(
                "check the spelling against `tape_redact::rules::built_in()` (e.g. `bearer_token`, not `bearer-token`)",
            ),
        }
    }
}

/// Issue #186 Acceptance #5 — surface `.taperc::pricing.pricing_file`
/// and a load verdict.
///
/// Probes the same workspace-then-user `.taperc` chain `cmd_stats` uses
/// (`resolve_pricing_source` in `main.rs`). When the section is set,
/// resolves the path against the `.taperc`'s parent (matching the
/// runtime resolver) and routes it through `PricingTable::load_from_file`
/// for a green/red verdict. Severity-on-fail is `Warn` — a broken
/// configured pricing file does not block recording, only `tape stats
/// --with-cost` (which exits 2 on its own).
pub struct ConfiguredPricingFile;
impl Check for ConfiguredPricingFile {
    fn id(&self) -> &'static str {
        "config.pricing_file.loads"
    }
    fn category(&self) -> &'static str {
        "config"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "`.taperc::pricing.pricing_file` resolves and loads as a valid pricing table (or is unset)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        // Same precedence as `resolve_pricing_source`: workspace .taperc
        // wins over user .taperc. Stay in lockstep with the runtime so
        // doctor never reports a file the CLI wouldn't actually consult.
        let workspace = locate_workspace(&env.cwd, env.home.as_deref());
        let user = env
            .home
            .as_ref()
            .map(|h| h.join(".taperc"))
            .filter(|p| p.is_file());
        let Some(taperc_path) = workspace.or(user) else {
            return CheckOutcome::pass("no .taperc found (ok)");
        };
        let yaml = match std::fs::read_to_string(&taperc_path) {
            Ok(y) => y,
            Err(e) => {
                return CheckOutcome::na(format!(
                    "could not read {} ({e}); see config.*_taperc.parses",
                    taperc_path.display()
                ));
            }
        };
        let Ok(cfg) = TapeRcConfig::parse(&yaml) else {
            return CheckOutcome::na(format!(
                "{} did not parse; see config.*_taperc.parses",
                taperc_path.display()
            ));
        };
        let Some(configured) = cfg.pricing.pricing_file.as_deref() else {
            return CheckOutcome::pass(format!(
                "no pricing.pricing_file set in {} (ok)",
                taperc_path.display()
            ));
        };
        let resolved = {
            let configured = std::path::Path::new(configured);
            if configured.is_absolute() {
                configured.to_path_buf()
            } else {
                taperc_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(configured)
            }
        };
        match tape_play::pricing::PricingTable::load_from_file(&resolved) {
            Ok(_) => CheckOutcome::pass(format!(
                "{} loads (configured in {})",
                resolved.display(),
                taperc_path.display()
            )),
            Err(e) => CheckOutcome::fail(format!(
                "{} (configured in {}): {e}",
                resolved.display(),
                taperc_path.display()
            ))
            .with_fix(format!(
                "edit {} and fix the pricing.pricing_file path or its contents",
                taperc_path.display()
            )),
        }
    }
}

fn check_taperc_path(path: &PathBuf) -> CheckOutcome {
    if !path.exists() {
        return CheckOutcome::pass(format!("{} not present (ok)", path.display()));
    }
    if !path.is_file() {
        return CheckOutcome::fail(format!(
            "{} exists but is not a regular file",
            path.display()
        ));
    }
    let yaml = match std::fs::read_to_string(path) {
        Ok(y) => y,
        Err(e) => {
            return CheckOutcome::fail(format!("could not read {}: {e}", path.display())).with_fix(
                format!(
                    "check that you can `cat {}` and re-run doctor",
                    path.display()
                ),
            );
        }
    };
    match TapeRcConfig::parse(&yaml) {
        Ok(_) => CheckOutcome::pass(format!("{} parses as valid YAML", path.display())),
        Err(e) => CheckOutcome::fail(format!("{}: {e}", path.display())).with_fix(format!(
            "edit {} and fix the YAML error above",
            path.display()
        )),
    }
}

/// Walk from `cwd` up to (but not past) `$HOME`, returning the first
/// `.taperc` discovered. Distinct from `TapeRcConfig::locate_workspace` only
/// in that it accepts an explicit `home` override (so tests can run inside a
/// tempdir without touching the real `$HOME`).
fn locate_workspace(cwd: &std::path::Path, home: Option<&std::path::Path>) -> Option<PathBuf> {
    let mut current = Some(cwd.to_path_buf());
    while let Some(dir) = current {
        let candidate = dir.join(".taperc");
        if candidate.is_file() {
            return Some(candidate);
        }
        if home == Some(dir.as_path()) {
            return None;
        }
        current = dir.parent().map(std::path::Path::to_path_buf);
    }
    None
}

#[cfg(test)]
mod tests {
    //! Coverage for `ConfiguredPricingFile` (issue #186 AC #5). The
    //! other three `config.*` checks predate the test module; their
    //! behaviour is covered indirectly via the `tape doctor` end-to-end
    //! tests in `tape-cli/tests/`.

    use super::*;
    use crate::doctor::check::Status;
    use std::path::Path;

    fn env_with(home: &Path, cwd: &Path) -> Env {
        Env {
            home: Some(home.to_path_buf()),
            cache_dir: None,
            tmpdir: std::env::temp_dir(),
            path_dirs: vec![],
            cwd: cwd.to_path_buf(),
            compile_time_version: "0.0.0-test",
        }
    }

    fn write_valid_pricing(path: &Path) {
        // Minimal valid pricing TOML — single row. Schema mirrors
        // `tape-play::pricing` (`[[model]]` with `vendor`/`model`/
        // `*_per_mtok` keys), not the doctor's own naming.
        std::fs::write(
            path,
            r#"
last_updated = "2026-01-01"

[[model]]
vendor = "anthropic"
model = "claude-test"
input_per_mtok = 1.0
output_per_mtok = 1.0
"#,
        )
        .unwrap();
    }

    #[test]
    fn passes_when_no_taperc_present() {
        let home = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let env = env_with(home.path(), cwd.path());
        let out = ConfiguredPricingFile.run(&env);
        assert_eq!(out.status, Status::Pass);
        assert!(out.message.contains("no .taperc"), "{}", out.message);
    }

    #[test]
    fn passes_when_pricing_section_absent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".taperc"),
            "redact:\n  disable_default: []\n",
        )
        .unwrap();
        let env = env_with(dir.path(), dir.path());
        let out = ConfiguredPricingFile.run(&env);
        assert_eq!(out.status, Status::Pass);
        assert!(
            out.message.contains("no pricing.pricing_file"),
            "{}",
            out.message
        );
    }

    #[test]
    fn passes_when_configured_file_loads() {
        let dir = tempfile::tempdir().unwrap();
        let pricing = dir.path().join("prices.toml");
        write_valid_pricing(&pricing);
        // Relative path — resolver must anchor on .taperc's parent.
        std::fs::write(
            dir.path().join(".taperc"),
            "pricing:\n  pricing_file: ./prices.toml\n",
        )
        .unwrap();
        let env = env_with(dir.path(), dir.path());
        let out = ConfiguredPricingFile.run(&env);
        assert_eq!(out.status, Status::Pass, "{out:?}");
        assert!(out.message.contains("loads"), "{}", out.message);
    }

    #[test]
    fn fails_when_configured_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".taperc"),
            "pricing:\n  pricing_file: ./does-not-exist.toml\n",
        )
        .unwrap();
        let env = env_with(dir.path(), dir.path());
        let out = ConfiguredPricingFile.run(&env);
        assert_eq!(out.status, Status::Fail, "{out:?}");
        // Both paths named in the diagnostic per AC.
        assert!(
            out.message.contains("does-not-exist.toml"),
            "{}",
            out.message
        );
        assert!(out.message.contains(".taperc"), "{}", out.message);
        assert!(out.suggested_fix.is_some());
    }

    #[test]
    fn na_when_taperc_does_not_parse() {
        // Doesn't double-fail with `config.*_taperc.parses`.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".taperc"), "redact: : :\n").unwrap();
        let env = env_with(dir.path(), dir.path());
        let out = ConfiguredPricingFile.run(&env);
        assert_eq!(out.status, Status::Na);
        assert!(out.message.contains("did not parse"), "{}", out.message);
    }
}
