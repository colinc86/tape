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
