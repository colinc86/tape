//! Integration tests for `tape doctor` (issue #81 phase 1).
//!
//! These tests exercise the doctor through the real `tape` binary by
//! constructing a hermetic environment in a `TempDir`:
//!
//! * a synthetic `$HOME` (controls where `.taperc`, `.claude/` resolve to);
//! * a synthetic `$PATH` containing shim scripts for `tape`, `tape-hook`,
//!   `tape-mcp-wrap` (so the binary-presence checks see "the install");
//! * a synthetic `$TMPDIR` (so the writability check is hermetic);
//! * `$NO_COLOR=1` so output is deterministic.
//!
//! Together this gives the known-good / known-broken / list-checks coverage
//! the issue body §"Test plan" asks for.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

fn tape_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

const TAPE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A hermetic environment for `tape doctor` integration tests.
struct DoctorEnv {
    _root: TempDir,
    home: PathBuf,
    tmpdir: PathBuf,
    path_dir: PathBuf,
}

impl DoctorEnv {
    fn new() -> Self {
        let root = TempDir::new().expect("tempdir");
        let home = root.path().join("home");
        let tmpdir = root.path().join("tmp");
        let path_dir = root.path().join("bin");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::create_dir_all(&path_dir).unwrap();
        Self {
            _root: root,
            home,
            tmpdir,
            path_dir,
        }
    }

    fn install_binary_shim(&self, name: &str, version: &str) {
        write_shim(&self.path_dir, name, version);
    }

    fn install_all_shims(&self) {
        self.install_binary_shim("tape", TAPE_VERSION);
        self.install_binary_shim("tape-hook", TAPE_VERSION);
        self.install_binary_shim("tape-mcp-wrap", TAPE_VERSION);
    }

    fn write_user_taperc(&self, body: &str) {
        std::fs::write(self.home.join(".taperc"), body).unwrap();
    }

    fn enable_claude_dir(&self) {
        std::fs::create_dir_all(self.home.join(".claude")).unwrap();
    }

    fn doctor(&self, args: &[&str]) -> std::process::Output {
        let mut cmd = Command::new(tape_bin());
        cmd.arg("doctor");
        cmd.args(args);
        cmd.env_clear();
        cmd.env("HOME", &self.home);
        cmd.env("TMPDIR", &self.tmpdir);
        cmd.env("PATH", &self.path_dir);
        cmd.env("NO_COLOR", "1");
        cmd.output().expect("spawn tape doctor")
    }
}

/// Write a tiny shell-script "binary" that responds to `--version` with
/// `tape <version>`. Marked executable.
fn write_shim(dir: &Path, name: &str, version: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    let body = format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo '{name} {version}'\n  exit 0\nfi\nexit 0\n");
    std::fs::write(&path, body).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn known_good_environment_exits_zero() {
    // §"Test plan" item 2: a fully-set-up synthetic install passes every
    // applicable phase-1 check.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert!(
        out.status.success(),
        "doctor should exit 0 on a clean install. stdout:\n{s}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(s.contains("9 pass"), "expected 9 passes; got:\n{s}");
    assert!(s.contains("0 fail"));
    assert!(s.contains("exit 0"));
}

#[test]
fn missing_tape_hook_binary_is_a_failure() {
    // §"Test plan" item 3 piece A: removing one binary surfaces the
    // matching check id as fail.
    let env = DoctorEnv::new();
    env.install_binary_shim("tape", TAPE_VERSION);
    // Deliberately omit tape-hook.
    env.install_binary_shim("tape-mcp-wrap", TAPE_VERSION);
    env.enable_claude_dir();

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "expected exit 1; got:\n{s}");
    assert!(s.contains("[XX] binary.tape-hook.present"), "stdout:\n{s}");
    assert!(s.contains("not found on $PATH"));
    assert!(s.contains("fix:"));
}

#[test]
fn malformed_user_taperc_is_a_failure_and_rule_check_short_circuits_to_na() {
    // §"Test plan" item 3 piece B: a broken .taperc fails parse and the
    // rule-ids check correctly reports n/a rather than double-failing on
    // the same root cause.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.write_user_taperc(":\n  this is not valid: yaml: at all: -\n");

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "stdout:\n{s}");
    assert!(s.contains("[XX] config.user_taperc.parses"));
    // rule-ids reports n/a because the parse already failed.
    assert!(
        s.contains("config.rule_ids.valid"),
        "rule-ids check should still appear in output:\n{s}"
    );
}

#[test]
fn unknown_rule_id_in_taperc_fails_rule_check() {
    // §"Test plan" item 1 piece A: a misspelt `disable_default` rule id
    // (the canonical example in the issue body — `bearer-token` vs
    // `bearer_token`) is caught by `config.rule_ids.valid`.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.write_user_taperc("redact:\n  disable_default: [\"bearer-token\"]\n");

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "stdout:\n{s}");
    assert!(s.contains("[OK] config.user_taperc.parses"), "stdout:\n{s}");
    assert!(s.contains("[XX] config.rule_ids.valid"), "stdout:\n{s}");
    assert!(s.contains("bearer-token"));
}

#[test]
fn version_skew_is_detected() {
    // §"Test plan" item 1 piece A: doctor was compiled against
    // CARGO_PKG_VERSION but the on-$PATH binary reports something else.
    let env = DoctorEnv::new();
    env.install_binary_shim("tape", "0.0.99-stale");
    env.install_binary_shim("tape-hook", TAPE_VERSION);
    env.install_binary_shim("tape-mcp-wrap", TAPE_VERSION);

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "stdout:\n{s}");
    assert!(s.contains("[XX] binary.tape.version"), "stdout:\n{s}");
    assert!(s.contains("0.0.99-stale"));
    assert!(s.contains(TAPE_VERSION));
    assert!(s.contains("version skew"));
}

#[test]
fn unwritable_tmpdir_is_a_failure() {
    use std::os::unix::fs::PermissionsExt;
    // §"Test plan" item 1 piece C analogue: a non-writable $TMPDIR.
    let env = DoctorEnv::new();
    env.install_all_shims();

    let mut perms = std::fs::metadata(&env.tmpdir).unwrap().permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(&env.tmpdir, perms).unwrap();

    let out = env.doctor(&[]);
    let s = stdout(&out);

    // Restore perms before any assert so a failing test doesn't leak a
    // 0555 tempdir that breaks teardown.
    let mut restore = std::fs::metadata(&env.tmpdir).unwrap().permissions();
    restore.set_mode(0o700);
    std::fs::set_permissions(&env.tmpdir, restore).unwrap();

    assert_eq!(out.status.code(), Some(1), "stdout:\n{s}");
    assert!(
        s.contains("[XX] permissions.tmpdir.writable"),
        "stdout:\n{s}"
    );
}

#[test]
fn missing_claude_dir_is_na_not_fail() {
    // §3.2: `permissions.claude_dir.writable` is n/a (not fail) when
    // ~/.claude/ is absent — tape is useful without Claude Code installed.
    let env = DoctorEnv::new();
    env.install_all_shims();
    // No claude dir.

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert!(out.status.success(), "exit 0 expected; got:\n{s}");
    assert!(
        s.contains("[--] permissions.claude_dir.writable"),
        "stdout:\n{s}"
    );
    assert!(s.contains("Claude Code not detected"));
}

#[test]
fn list_checks_is_stable() {
    // §"Test plan" item 7: catalog enumeration is the canonical surface
    // for "what does doctor check?", and PR review uses it as the test
    // of record that nothing was accidentally removed.
    let out = Command::new(tape_bin())
        .arg("doctor")
        .arg("--list-checks")
        .env("NO_COLOR", "1")
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();

    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(
        lines.len(),
        9,
        "phase-1 has 9 checks; got {}:\n{s}",
        lines.len()
    );

    // Every row is `id\tcategory\tseverity\tdescription`.
    for line in &lines {
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), 4, "expected 4 cols in {line:?}");
    }

    // Spot-check stable ids — the order is also the execution order.
    let ids: Vec<&str> = lines
        .iter()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![
            "binary.tape.present",
            "binary.tape-hook.present",
            "binary.tape-mcp-wrap.present",
            "binary.tape.version",
            "config.user_taperc.parses",
            "config.workspace_taperc.parses",
            "config.rule_ids.valid",
            "permissions.tmpdir.writable",
            "permissions.claude_dir.writable",
        ]
    );
}

#[test]
fn include_filter_narrows_to_category() {
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    let out = env.doctor(&["--include", "permissions"]);
    let s = stdout(&out);
    assert!(out.status.success(), "stdout:\n{s}");
    assert!(s.contains("permissions.tmpdir.writable"));
    assert!(!s.contains("binary.tape.present"));
    assert!(!s.contains("config.user_taperc.parses"));
}

#[test]
fn exclude_filter_drops_category() {
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    let out = env.doctor(&["--exclude", "binary"]);
    let s = stdout(&out);
    assert!(out.status.success(), "stdout:\n{s}");
    assert!(!s.contains("binary.tape.present"));
    assert!(s.contains("config.user_taperc.parses"));
    assert!(s.contains("permissions.tmpdir.writable"));
}

#[test]
fn select_check_runs_only_that_id() {
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    let out = env.doctor(&["--check", "permissions.tmpdir.writable"]);
    let s = stdout(&out);
    assert!(out.status.success(), "stdout:\n{s}");
    assert!(s.contains("permissions.tmpdir.writable"));
    assert!(!s.contains("binary.tape.present"));
    assert!(!s.contains("config.user_taperc.parses"));
    assert!(s.contains("1 pass"));
}

#[test]
fn quiet_omits_pass_lines() {
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    // Break exactly one check so there's something for `--quiet` to show.
    env.write_user_taperc(": bad: yaml:\n");
    let out = env.doctor(&["--quiet"]);
    let s = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "stdout:\n{s}");
    // No `[OK]` glyphs should appear — quiet suppresses them.
    assert!(!s.contains("[OK]"), "quiet should suppress [OK]; got:\n{s}");
    assert!(s.contains("[XX] config.user_taperc.parses"));
}

#[test]
fn no_color_strips_ansi_escapes() {
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    let out = env.doctor(&["--no-color"]);
    let s = stdout(&out);
    assert!(!s.contains('\u{1b}'), "no ANSI escapes expected; got:\n{s}");
}
