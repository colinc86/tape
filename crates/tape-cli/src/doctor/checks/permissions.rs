//! `permissions.*` — filesystem writability checks.

use std::path::Path;

use super::super::check::{Check, CheckOutcome, Env, Severity};

pub struct TmpdirWritable;
impl Check for TmpdirWritable {
    fn id(&self) -> &'static str {
        "permissions.tmpdir.writable"
    }
    fn category(&self) -> &'static str {
        "permissions"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "$TMPDIR (or /tmp) is writable; tape record needs this for its per-run tempdir"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        if !env.tmpdir.is_dir() {
            return CheckOutcome::fail(format!(
                "{} does not exist or is not a directory",
                env.tmpdir.display()
            ))
            .with_fix("create the directory or unset $TMPDIR");
        }
        match probe_write(&env.tmpdir) {
            Ok(()) => CheckOutcome::pass(format!("{} writable", env.tmpdir.display())),
            Err(e) => CheckOutcome::fail(format!("{} not writable: {e}", env.tmpdir.display()))
                .with_fix(format!(
                    "ensure the current user can write to {}",
                    env.tmpdir.display()
                )),
        }
    }
}

/// `~/.claude/` must be writable for Claude Code to operate. If the directory
/// doesn't exist at all we report `n/a` rather than fail — `tape` is useful
/// even without Claude Code installed, and this check is the soft signal
/// rather than a hard dependency.
pub struct ClaudeDirWritable;
impl Check for ClaudeDirWritable {
    fn id(&self) -> &'static str {
        "permissions.claude_dir.writable"
    }
    fn category(&self) -> &'static str {
        "permissions"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "~/.claude/ exists and is writable (n/a if Claude Code isn't installed)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        let Some(home) = env.home.as_ref() else {
            return CheckOutcome::na("$HOME is not set");
        };
        let path = home.join(".claude");
        if !path.exists() {
            return CheckOutcome::na(format!(
                "{} not present (Claude Code not detected)",
                path.display()
            ));
        }
        if !path.is_dir() {
            return CheckOutcome::fail(format!("{} exists but is not a directory", path.display()));
        }
        match probe_write(&path) {
            Ok(()) => CheckOutcome::pass(format!("{} writable", path.display())),
            Err(e) => CheckOutcome::fail(format!("{} not writable: {e}", path.display())).with_fix(
                format!("ensure the current user can write to {}", path.display()),
            ),
        }
    }
}

/// Attempt a create-then-delete probe inside `dir`. Returns the underlying
/// I/O error string on failure so the user has a hint at the cause (e.g.
/// `Permission denied`, `Read-only file system`).
fn probe_write(dir: &Path) -> std::io::Result<()> {
    use std::io::Write;
    // Use a long-ish unique suffix so concurrent doctor runs don't collide.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let probe = dir.join(format!(".tape-doctor-probe-{nanos}-{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&probe)?;
        f.write_all(b"tape doctor probe")?;
        f.sync_all().ok();
    }
    let _ = std::fs::remove_file(&probe);
    Ok(())
}
