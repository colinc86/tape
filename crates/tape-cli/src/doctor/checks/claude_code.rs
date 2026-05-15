//! `claude-code.*` — soft-dependency checks for the Claude Code install
//! and the bundled `tape` plugin. Issue #163 / Step-2 of #81.
//!
//! Both checks are `Warn`-severity: `tape` is useful without Claude Code
//! (the recording proxy, the `tape-cli`, and the format crate all stand
//! alone), so absence is a heads-up rather than a hard failure. Severity
//! never escalates the exit code in this phase — `--strict` is deferred.

use super::super::check::{Check, CheckOutcome, Env, Severity};

/// Does the `claude` binary exist on `$PATH`?
///
/// `Pass` when found, `Warn` when not. Never `Na` — `$PATH` is always
/// defined in the process environment, and absence of `claude` is the
/// actionable warning state (the user might want to install Claude
/// Code to unlock the recording-by-hooks flow).
pub struct ClaudeInstalled;
impl Check for ClaudeInstalled {
    fn id(&self) -> &'static str {
        "claude-code.installed"
    }
    fn category(&self) -> &'static str {
        "claude-code"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "claude on $PATH (Claude Code optional but recommended)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        match env.find_on_path("claude") {
            Some(p) => CheckOutcome::pass(format!("claude on $PATH — {}", p.display())),
            None => CheckOutcome::warn(
                "claude not found on $PATH (Claude Code optional but recommended)",
            )
            .with_fix("install Claude Code from https://claude.com/claude-code"),
        }
    }
}

/// Is the bundled `tape` plugin registered with Claude Code?
///
/// The canonical signal is the presence of `$HOME/.claude/plugins/tape/`
/// (a directory). Returns `Na` rather than `Warn` when the directory is
/// absent: a user who never installed the plugin should not get a
/// yellow line for a feature they're not using. Same goes for the
/// `$HOME`-unset and `claude not installed` short-circuits.
pub struct ClaudePluginEnabled;
impl Check for ClaudePluginEnabled {
    fn id(&self) -> &'static str {
        "claude-code.plugin.enabled"
    }
    fn category(&self) -> &'static str {
        "claude-code"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "~/.claude/plugins/tape/ registered with Claude Code (n/a when not in use)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        let Some(home) = env.home.as_ref() else {
            return CheckOutcome::na("$HOME not set");
        };
        // Short-circuit if `claude` is missing on $PATH — the parent
        // `claude-code.installed` check already surfaced the missing
        // install. Double-warning on the same root cause is noise.
        if env.find_on_path("claude").is_none() {
            return CheckOutcome::na("claude not on $PATH — see claude-code.installed");
        }
        let path = home.join(".claude").join("plugins").join("tape");
        if !path.exists() {
            return CheckOutcome::na(format!(
                "{} not present (tape plugin not installed)",
                path.display()
            ));
        }
        if !path.is_dir() {
            return CheckOutcome::warn(format!("{} exists but is not a directory", path.display()))
                .with_fix(format!(
                    "remove the file at {} and reinstall the tape plugin",
                    path.display()
                ));
        }
        CheckOutcome::pass(format!("tape plugin registered at {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn env_with(home: Option<PathBuf>, path_dirs: Vec<PathBuf>) -> Env {
        Env {
            home,
            tmpdir: PathBuf::from("/tmp"),
            path_dirs,
            cwd: PathBuf::from("."),
            compile_time_version: env!("CARGO_PKG_VERSION"),
        }
    }

    #[test]
    fn installed_passes_when_claude_on_path() {
        // Use a known-good binary as the `claude` shim. Any executable
        // file on $PATH satisfies `find_on_path`; on every Unix system
        // `/bin/sh` is present and executable.
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("claude");
        std::fs::write(&shim, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(&shim).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(&shim, p).unwrap();
        }
        let env = env_with(None, vec![dir.path().to_path_buf()]);
        let out = ClaudeInstalled.run(&env);
        assert_eq!(out.status, super::super::super::check::Status::Pass);
        assert!(out.message.contains("claude on $PATH"));
    }

    #[test]
    fn installed_warns_when_claude_absent() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_with(None, vec![dir.path().to_path_buf()]);
        let out = ClaudeInstalled.run(&env);
        assert_eq!(out.status, super::super::super::check::Status::Warn);
        assert!(out.message.contains("not found"));
        assert!(out.suggested_fix.is_some(), "warn must carry a fix hint");
    }

    #[test]
    fn plugin_enabled_returns_na_when_home_unset() {
        let env = env_with(None, vec![]);
        let out = ClaudePluginEnabled.run(&env);
        assert_eq!(out.status, super::super::super::check::Status::Na);
        assert!(out.message.contains("$HOME"));
    }

    #[test]
    fn plugin_enabled_returns_na_when_claude_not_on_path() {
        // $HOME is set + plugin dir might not exist, but claude itself
        // is missing — short-circuit to `Na` so we don't double-warn on
        // the same root cause as claude-code.installed.
        let home = tempfile::tempdir().unwrap();
        let path = tempfile::tempdir().unwrap();
        let env = env_with(
            Some(home.path().to_path_buf()),
            vec![path.path().to_path_buf()],
        );
        let out = ClaudePluginEnabled.run(&env);
        assert_eq!(out.status, super::super::super::check::Status::Na);
        assert!(out.message.contains("claude-code.installed"), "{out:?}");
    }

    #[test]
    fn plugin_enabled_returns_na_when_plugin_dir_absent() {
        // claude on $PATH, $HOME set, but no `plugins/tape` directory.
        // Treat as "feature not in use" → Na, not Warn.
        let home = tempfile::tempdir().unwrap();
        let path = tempfile::tempdir().unwrap();
        let shim = path.path().join("claude");
        std::fs::write(&shim, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(&shim).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(&shim, p).unwrap();
        }
        let env = env_with(
            Some(home.path().to_path_buf()),
            vec![path.path().to_path_buf()],
        );
        let out = ClaudePluginEnabled.run(&env);
        assert_eq!(out.status, super::super::super::check::Status::Na);
        assert!(out.message.contains("plugins/tape"));
        assert!(out.message.contains("not installed"));
    }

    #[test]
    fn plugin_enabled_passes_when_plugin_dir_present() {
        let home = tempfile::tempdir().unwrap();
        let path = tempfile::tempdir().unwrap();
        let shim = path.path().join("claude");
        std::fs::write(&shim, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(&shim).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(&shim, p).unwrap();
        }
        std::fs::create_dir_all(home.path().join(".claude").join("plugins").join("tape")).unwrap();
        let env = env_with(
            Some(home.path().to_path_buf()),
            vec![path.path().to_path_buf()],
        );
        let out = ClaudePluginEnabled.run(&env);
        assert_eq!(out.status, super::super::super::check::Status::Pass);
        assert!(out.message.contains("tape plugin registered"));
    }
}
