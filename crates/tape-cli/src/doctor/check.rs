//! Check trait and supporting types for `tape doctor`.
//!
//! Phase-1 only exercises `Fail`-severity checks via the trait, but the
//! architecture is built for `Warn` too (e.g. `signing.keystore.perms` in
//! phase 2). The dead-code allow keeps the trait surface coherent across
//! phases without sprinkling per-symbol annotations.
#![allow(dead_code)]
//!
//! A `Check` is the architectural unit of the doctor framework: a small,
//! isolated function that inspects one slice of the install surface and
//! returns a [`CheckOutcome`]. The runner gathers all configured checks,
//! invokes each one against a shared [`Env`], and emits a report.
//!
//! Phase 1 (issue #81) intentionally ships only `binary.*`, `config.*`, and
//! `permissions.*` checks. The Check trait is the seam phase 2+ will extend.

use std::path::{Path, PathBuf};

/// Severity level a failing check raises by default. Currently the project
/// has two levels — `Warn` (advisory) and `Fail` (must-fix). Phase 1 doesn't
/// expose `--strict`, so warnings never escalate the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warn,
    Fail,
}

impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Severity::Warn => "warn",
            Severity::Fail => "fail",
        }
    }
}

/// Result status of a single check after execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Warn,
    Fail,
    /// Not applicable — the check is gated on a feature that isn't present.
    /// Does not contribute to any exit code. Carries a short reason.
    Na,
    /// The check itself couldn't run (timeout, panic, permission denied).
    /// Contributes to exit code 3.
    Harness,
}

impl Status {
    pub const fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Warn => "warn",
            Status::Fail => "fail",
            Status::Na => "n/a",
            Status::Harness => "harness",
        }
    }

    /// ASCII fallback glyph used when colour is disabled. The principal's
    /// scoping comment locks these strings in (§3.3 of the issue body).
    pub const fn glyph(self) -> &'static str {
        match self {
            Status::Pass => "[OK]",
            Status::Warn => "[!!]",
            Status::Fail => "[XX]",
            Status::Na | Status::Harness => "[--]",
        }
    }
}

/// The full result of running one check, ready to be rendered.
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    pub status: Status,
    /// Short, single-line human-readable message. Required for every status
    /// (including `Pass`, where it doubles as a "what was verified" caption).
    pub message: String,
    /// Optional one-line remediation hint. Rendered as `fix: <suggested_fix>`
    /// under non-pass lines.
    pub suggested_fix: Option<String>,
}

impl CheckOutcome {
    pub fn pass<S: Into<String>>(message: S) -> Self {
        Self {
            status: Status::Pass,
            message: message.into(),
            suggested_fix: None,
        }
    }

    pub fn warn<S: Into<String>>(message: S) -> Self {
        Self {
            status: Status::Warn,
            message: message.into(),
            suggested_fix: None,
        }
    }

    pub fn fail<S: Into<String>>(message: S) -> Self {
        Self {
            status: Status::Fail,
            message: message.into(),
            suggested_fix: None,
        }
    }

    pub fn na<S: Into<String>>(message: S) -> Self {
        Self {
            status: Status::Na,
            message: message.into(),
            suggested_fix: None,
        }
    }

    pub fn harness<S: Into<String>>(message: S) -> Self {
        Self {
            status: Status::Harness,
            message: message.into(),
            suggested_fix: None,
        }
    }

    #[must_use]
    pub fn with_fix<S: Into<String>>(mut self, fix: S) -> Self {
        self.suggested_fix = Some(fix.into());
        self
    }
}

/// The environment passed to every check. In production all fields are
/// resolved from the real process; in tests fields are overridden to point
/// at synthetic directories so the check sees a hermetic surface.
#[derive(Debug, Clone)]
pub struct Env {
    /// `$HOME` (or its test-time override). Used to resolve `~/.taperc`,
    /// `~/.claude/`, and similar paths.
    pub home: Option<PathBuf>,
    /// `$TMPDIR` (or `/tmp` fallback / test override). Used by tempdir
    /// writability checks.
    pub tmpdir: PathBuf,
    /// Directories to search for binary presence checks. Mirrors `$PATH`.
    pub path_dirs: Vec<PathBuf>,
    /// Current working directory. Used by the workspace `.taperc` walk.
    pub cwd: PathBuf,
    /// The compile-time `tape` version (`env!("CARGO_PKG_VERSION")`). Used
    /// as the source of truth for the `binary.tape.version` check.
    pub compile_time_version: &'static str,
}

impl Env {
    /// Resolve a process-real environment.
    pub fn from_process() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let tmpdir = std::env::var_os("TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let path_dirs = std::env::var_os("PATH")
            .map(|raw| std::env::split_paths(&raw).collect())
            .unwrap_or_default();
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            home,
            tmpdir,
            path_dirs,
            cwd,
            compile_time_version: env!("CARGO_PKG_VERSION"),
        }
    }

    /// Helper for binary-presence checks. Returns the first executable
    /// `<dir>/<name>` discovered in `path_dirs`, or `None`.
    pub fn find_on_path(&self, name: &str) -> Option<PathBuf> {
        for dir in &self.path_dirs {
            let candidate = dir.join(name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
        None
    }
}

/// The Check trait. Every phase-1 check implements this. Keep impls tiny:
/// one function each, fully testable in isolation.
pub trait Check: Send + Sync {
    /// Stable, dotted, category-prefixed identifier. e.g. `binary.tape.present`.
    fn id(&self) -> &'static str;

    /// Category. One of: `binary | config | permissions` in phase 1.
    /// Phase 2 will add `plugin | mcp | claude-code | recording`.
    fn category(&self) -> &'static str;

    /// Severity to report when the check fails.
    fn severity_on_fail(&self) -> Severity;

    /// One-line description for `--list-checks` output.
    fn description(&self) -> &'static str;

    /// Run the check against the given environment.
    fn run(&self, env: &Env) -> CheckOutcome;
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        (meta.permissions().mode() & 0o111) != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
