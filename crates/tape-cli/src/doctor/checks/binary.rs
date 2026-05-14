//! `binary.*` — sibling-binary presence and version-skew checks.

use std::process::Command;

use super::super::check::{Check, CheckOutcome, Env, Severity};

pub struct TapePresent;
impl Check for TapePresent {
    fn id(&self) -> &'static str {
        "binary.tape.present"
    }
    fn category(&self) -> &'static str {
        "binary"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "tape on $PATH and executable"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        match env.find_on_path("tape") {
            Some(p) => CheckOutcome::pass(format!("tape on $PATH ({})", p.display())),
            None => CheckOutcome::fail("tape not found on $PATH").with_fix(
                "reinstall the tape plugin, or add the directory containing `tape` to $PATH",
            ),
        }
    }
}

pub struct TapeHookPresent;
impl Check for TapeHookPresent {
    fn id(&self) -> &'static str {
        "binary.tape-hook.present"
    }
    fn category(&self) -> &'static str {
        "binary"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "tape-hook on $PATH and executable"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        match env.find_on_path("tape-hook") {
            Some(p) => CheckOutcome::pass(format!("tape-hook on $PATH ({})", p.display())),
            None => CheckOutcome::fail("tape-hook not found on $PATH")
                .with_fix("reinstall the tape plugin, or copy `tape-hook` next to `tape` on $PATH"),
        }
    }
}

pub struct TapeMcpWrapPresent;
impl Check for TapeMcpWrapPresent {
    fn id(&self) -> &'static str {
        "binary.tape-mcp-wrap.present"
    }
    fn category(&self) -> &'static str {
        "binary"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "tape-mcp-wrap on $PATH and executable"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        match env.find_on_path("tape-mcp-wrap") {
            Some(p) => CheckOutcome::pass(format!("tape-mcp-wrap on $PATH ({})", p.display())),
            None => CheckOutcome::fail("tape-mcp-wrap not found on $PATH").with_fix(
                "reinstall the tape plugin, or copy `tape-mcp-wrap` next to `tape` on $PATH",
            ),
        }
    }
}

/// Verify the `tape` binary on `$PATH` reports the same version the doctor
/// was compiled against. This is the version-skew tripwire: the user's
/// shell-resolved `tape` is what their commands will actually invoke, and
/// if it disagrees with this binary's `CARGO_PKG_VERSION` we have a
/// confusing install on our hands.
pub struct TapeVersion;
impl Check for TapeVersion {
    fn id(&self) -> &'static str {
        "binary.tape.version"
    }
    fn category(&self) -> &'static str {
        "binary"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "$PATH's `tape --version` matches doctor's compile-time version"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        let Some(tape_path) = env.find_on_path("tape") else {
            // `binary.tape.present` already reports the failure; emit n/a
            // here so we don't double-fail on the same root cause.
            return CheckOutcome::na("tape not on $PATH — see binary.tape.present");
        };
        let out = match Command::new(&tape_path).arg("--version").output() {
            Ok(o) => o,
            Err(e) => {
                return CheckOutcome::harness(format!(
                    "could not exec {}: {e}",
                    tape_path.display()
                ));
            }
        };
        if !out.status.success() {
            return CheckOutcome::fail(format!(
                "`{} --version` exited {}",
                tape_path.display(),
                out.status.code().unwrap_or(-1)
            ));
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let reported = parse_version_line(&stdout);
        let expected = env.compile_time_version;
        match reported {
            Some(v) if v == expected => {
                CheckOutcome::pass(format!("version reported by `tape --version`: {v}"))
            }
            Some(v) => CheckOutcome::fail(format!(
                "$PATH `tape` reports {v}; doctor was compiled against {expected} (version skew)"
            ))
            .with_fix("ensure $PATH points at the matching tape binary, or reinstall the plugin"),
            None => CheckOutcome::fail(format!(
                "could not parse a version from `tape --version` output: {:?}",
                stdout.trim()
            )),
        }
    }
}

/// `tape --version` output looks like `tape 0.1.2`. Pull out the version
/// token after the first whitespace and strip a trailing newline. Returns
/// `None` if the format is unrecognisable.
fn parse_version_line(s: &str) -> Option<String> {
    let line = s.lines().next()?.trim();
    let token = line.split_whitespace().last()?;
    if token.is_empty() {
        None
    } else {
        Some(token.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clap_default_version_line() {
        assert_eq!(parse_version_line("tape 0.1.2\n").as_deref(), Some("0.1.2"));
        assert_eq!(parse_version_line("tape 0.2.0").as_deref(), Some("0.2.0"));
    }

    #[test]
    fn rejects_blank_output() {
        assert_eq!(parse_version_line(""), None);
        assert_eq!(parse_version_line("\n"), None);
    }
}
