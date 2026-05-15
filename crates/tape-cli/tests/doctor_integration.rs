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

    /// Install a `claude` shim alongside the tape shims. Step 2 of #81
    /// (issue #163) — the `claude-code.installed` check resolves
    /// against `$PATH` exactly the way the binary checks do.
    fn install_claude_shim(&self) {
        write_shim(&self.path_dir, "claude", "0.0.0-test");
    }

    /// Materialise the bundled-plugin directory the `claude-code.plugin.enabled`
    /// check looks for. Implies `enable_claude_dir` because the plugin
    /// path is nested under it.
    fn enable_tape_plugin(&self) {
        std::fs::create_dir_all(self.home.join(".claude").join("plugins").join("tape")).unwrap();
    }

    /// Provision `$HOME/.tape/keys/` at the given mode. Step-3 of #81
    /// (issue #166). Creates the parent `.tape` dir as well.
    fn provision_keystore(&self, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        let path = self.home.join(".tape").join("keys");
        std::fs::create_dir_all(&path).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(&path, perms).unwrap();
    }

    /// Drop a `*.key` file inside the keystore at the given mode.
    /// Implies `provision_keystore` was called.
    fn drop_key(&self, name: &str, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        let path = self.home.join(".tape").join("keys").join(name);
        std::fs::write(&path, b"fake key bytes").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(&path, perms).unwrap();
    }

    /// Materialise `<cache>/tape/index/` as an empty directory.
    /// `<cache>` is the macOS `Library/Caches` subtree under this
    /// fixture's `$HOME` (matches the production resolver — this
    /// crate ships on macOS in CI). Used by Step-5 of #81 (#183)
    /// to exercise the `index.exists` → `Pass` branch.
    fn provision_index_dir(&self) {
        let cache = self.cache_root();
        std::fs::create_dir_all(cache.join("tape").join("index")).unwrap();
    }

    fn cache_root(&self) -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            self.home.join("Library").join("Caches")
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.home.join(".cache")
        }
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
    // applicable phase-1 check. With #163 (claude-code Step 2) the
    // "fully-set-up" surface grows to include the `claude` binary on
    // $PATH and the `~/.claude/plugins/tape/` plugin directory. Signing
    // (#166) stays as 3 n/a since no keystore is provisioned.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.install_claude_shim();
    env.enable_claude_dir();
    env.enable_tape_plugin();

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert!(
        out.status.success(),
        "doctor should exit 0 on a clean install. stdout:\n{s}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        s.contains("12 pass"),
        "expected 12 passes (11 baseline + #177 pricing.table.fresh); got:\n{s}"
    );
    assert!(
        s.contains("[OK] pricing.table.fresh"),
        "pricing.table.fresh must render as [OK]:\n{s}"
    );
    // Step-5 of #81 (issue #183): the four `index.*` checks should
    // all surface as `[--]` on a fixture without `<cache>/tape/index/`.
    for id in [
        "index.exists",
        "index.sqlite.integrity",
        "index.lock.stale",
        "index.last_rescan.fresh",
    ] {
        assert!(
            s.contains(&format!("[--] {id}")),
            "{id} should render as [--] on a no-library fixture:\n{s}"
        );
    }
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
        19,
        "doctor catalog has 19 checks (phase 1 + #163 claude-code + #166 signing + #177 pricing + #183 index); got {}:\n{s}",
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
            "claude-code.installed",
            "claude-code.plugin.enabled",
            "signing.keystore.readable",
            "signing.keystore.perms",
            "signing.trust_store.readable",
            "pricing.table.fresh",
            "index.exists",
            "index.sqlite.integrity",
            "index.lock.stale",
            "index.last_rescan.fresh",
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
fn doctor_include_pricing_runs_only_that_check() {
    // Step-4 of #81 (issue #177): `--include pricing` narrows to the
    // single new check. The bundled pricing table is compiled in, so
    // this test does not depend on synthetic env state — just the
    // version of the binary under test. Exit 0 (the check passes on a
    // freshly-shipped binary, warns once it ages past 90 days; both
    // are exit 0 without --strict).
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    let out = env.doctor(&["--include", "pricing"]);
    let s = stdout(&out);
    assert!(out.status.success(), "stdout:\n{s}");
    assert!(s.contains("pricing.table.fresh"), "stdout:\n{s}");
    assert!(!s.contains("binary.tape.present"));
    assert!(!s.contains("config.user_taperc.parses"));
    assert!(!s.contains("signing.keystore.readable"));
}

#[test]
fn doctor_include_index_runs_only_those_checks() {
    // Step-5 of #81 (issue #183): `--include index` narrows to the four
    // new checks. With no `<cache>/tape/index/` provisioned, every one
    // surfaces `[--]` (library not in use). Exit 0 either way.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    let out = env.doctor(&["--include", "index"]);
    let s = stdout(&out);
    assert!(out.status.success(), "stdout:\n{s}");
    for id in [
        "index.exists",
        "index.sqlite.integrity",
        "index.lock.stale",
        "index.last_rescan.fresh",
    ] {
        assert!(
            s.contains(&format!("[--] {id}")),
            "{id} should render as [--] without an index dir:\n{s}"
        );
    }
    // Adjacent categories should not have rendered.
    assert!(!s.contains("binary.tape.present"));
    assert!(!s.contains("signing.keystore.readable"));
    assert!(!s.contains("pricing.table.fresh"));
}

#[test]
fn index_exists_passes_when_dir_present() {
    // AC #2 of #183: provisioning `<cache>/tape/index/` as an empty
    // directory flips `index.exists` to `[OK]` while the other three
    // index checks stay `[--]` with the "not present" wording (not the
    // "deferred to #2 follow-up" wording — that's a separate AC #3
    // branch).
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    env.provision_index_dir();
    let out = env.doctor(&["--include", "index"]);
    let s = stdout(&out);
    assert!(out.status.success(), "stdout:\n{s}");
    assert!(s.contains("[OK] index.exists"), "stdout:\n{s}");
    for id in [
        "index.sqlite.integrity",
        "index.lock.stale",
        "index.last_rescan.fresh",
    ] {
        assert!(
            s.contains(&format!("[--] {id}")),
            "{id} should still render as [--] (no catalog file):\n{s}"
        );
    }
    assert!(
        !s.contains("deferred to the #2 follow-up"),
        "no `deferred` wording on a dir-only fixture:\n{s}"
    );
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

// --- Step 2 of #81 (issue #163): claude-code category --------------

#[test]
fn claude_code_installed_warns_when_claude_missing() {
    // AC #2: no `claude` on $PATH → `claude-code.installed` is `warn`,
    // `claude-code.plugin.enabled` is `n/a`. Exit code stays 0 because
    // warns don't escalate without `--strict` (which is deferred).
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    // No install_claude_shim → claude is missing.

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert!(out.status.success(), "warn-only output is exit 0: {out:?}");
    assert!(s.contains("[!!] claude-code.installed"), "stdout:\n{s}");
    assert!(s.contains("not found on $PATH"));
    assert!(
        s.contains("install Claude Code"),
        "fix string surfaces: {s}"
    );
    // Plugin check short-circuits to n/a because the prerequisite check
    // failed; we should not see a second warn line on the same root.
    assert!(
        s.contains("[--] claude-code.plugin.enabled"),
        "stdout:\n{s}"
    );
    assert!(s.contains("see claude-code.installed"));
}

#[test]
fn claude_code_plugin_na_when_directory_absent() {
    // claude on $PATH, $HOME set, but no plugin directory → plugin
    // check returns n/a. The "feature not in use" branch per §3.2.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.install_claude_shim();
    env.enable_claude_dir();
    // No enable_tape_plugin → plugins/tape/ is absent.

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert!(out.status.success(), "{out:?}");
    assert!(s.contains("[OK] claude-code.installed"), "stdout:\n{s}");
    assert!(
        s.contains("[--] claude-code.plugin.enabled"),
        "stdout:\n{s}"
    );
    assert!(s.contains("plugins/tape"));
    assert!(s.contains("tape plugin not installed"));
}

#[test]
fn doctor_include_claude_code_runs_only_those_checks() {
    // AC #3: `tape doctor --include claude-code` runs only the two new
    // checks. Healthy environment → exit 0, both pass.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.install_claude_shim();
    env.enable_claude_dir();
    env.enable_tape_plugin();

    let out = env.doctor(&["--include", "claude-code"]);
    let s = stdout(&out);
    assert!(out.status.success(), "{out:?}");
    assert!(s.contains("[OK] claude-code.installed"), "stdout:\n{s}");
    assert!(
        s.contains("[OK] claude-code.plugin.enabled"),
        "stdout:\n{s}"
    );
    // None of the other categories' checks run under --include.
    assert!(!s.contains("binary.tape.present"), "stdout:\n{s}");
    assert!(!s.contains("config.user_taperc.parses"), "stdout:\n{s}");
    assert!(!s.contains("permissions.tmpdir.writable"), "stdout:\n{s}");
}

// --- Step 3 of #81 (issue #166): signing category -------------------

#[test]
fn signing_no_keystore_reports_all_na_exit_zero() {
    // AC #1: a machine without `~/.tape/keys/` reports all three
    // signing checks as `n/a` with the "not in use" message. The
    // category header still renders so the user can see "what
    // exists" without ambiguity.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    // No provision_keystore → signing surfaces n/a × 3.

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert!(out.status.success(), "n/a only is exit 0: {out:?}");
    assert!(s.contains("[--] signing.keystore.readable"), "stdout:\n{s}");
    assert!(s.contains("[--] signing.keystore.perms"), "stdout:\n{s}");
    assert!(
        s.contains("[--] signing.trust_store.readable"),
        "stdout:\n{s}"
    );
    assert!(s.contains("signing not in use"), "stdout:\n{s}");
}

#[test]
fn signing_keystore_warns_on_bad_perms() {
    // AC #3: `~/.tape/keys/` at mode 0755 trips
    // `signing.keystore.readable` as warn with the chmod-0700 fix
    // string. Exit 0 (no --strict).
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    env.provision_keystore(0o755);

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert!(out.status.success(), "warn-only is exit 0: {out:?}");
    assert!(s.contains("[!!] signing.keystore.readable"), "stdout:\n{s}");
    assert!(s.contains("0755"), "stdout:\n{s}");
    assert!(s.contains("0700"), "stdout:\n{s}");
    assert!(s.contains("chmod 0700"), "fix string surfaces: {s}");
}

#[test]
fn signing_keys_warn_on_bad_file_mode() {
    // AC #4: an over-permissive `*.key` trips `signing.keystore.perms`
    // as warn naming the bad file. Exit 0.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();
    env.provision_keystore(0o700);
    env.drop_key("default.key", 0o644);

    let out = env.doctor(&[]);
    let s = stdout(&out);
    assert!(out.status.success(), "warn-only is exit 0: {out:?}");
    assert!(s.contains("[!!] signing.keystore.perms"), "stdout:\n{s}");
    assert!(s.contains("default.key"), "stdout:\n{s}");
    assert!(s.contains("0644"), "stdout:\n{s}");
    assert!(s.contains("chmod 0600"), "fix string surfaces: {s}");
}

#[test]
fn doctor_include_signing_runs_only_those_checks() {
    // AC #5: `--include signing` runs the three new checks only.
    // No keystore on the fixture → all three surface n/a.
    let env = DoctorEnv::new();
    env.install_all_shims();
    env.enable_claude_dir();

    let out = env.doctor(&["--include", "signing"]);
    let s = stdout(&out);
    assert!(out.status.success(), "{out:?}");
    assert!(s.contains("[--] signing.keystore.readable"), "stdout:\n{s}");
    assert!(s.contains("[--] signing.keystore.perms"), "stdout:\n{s}");
    assert!(
        s.contains("[--] signing.trust_store.readable"),
        "stdout:\n{s}"
    );
    // None of the other categories' checks should run under
    // --include signing.
    assert!(!s.contains("binary.tape.present"), "stdout:\n{s}");
    assert!(!s.contains("config.user_taperc.parses"), "stdout:\n{s}");
    assert!(!s.contains("permissions.tmpdir.writable"), "stdout:\n{s}");
}
