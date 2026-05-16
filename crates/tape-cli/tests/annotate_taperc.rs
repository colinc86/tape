//! `tape annotate` + `.taperc::annotate` integration coverage.
//! Step-4a of #74 / issue #192. Drives a hermetic `$HOME` so each
//! test owns its `.taperc`.

use std::process::Command;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Copy `minimal-success.tape` into the given dir so each test's
/// input is isolated. Returns the input path.
fn isolated_minimal(dir: &std::path::Path) -> std::path::PathBuf {
    let dst = dir.join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &dst).unwrap();
    dst
}

/// Run `tape <args>` with `$HOME=<home>` and `current_dir=<home>`
/// so the workspace `.taperc` walk lands inside the caller's
/// tempdir. Other env vars are preserved.
fn run_in_home(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(binary_path());
    cmd.args(args)
        .env_remove("HOME")
        .env("HOME", home)
        .current_dir(home);
    cmd.output().unwrap()
}

fn json_field(stdout: &[u8], key: &str) -> String {
    let v: serde_json::Value = serde_json::from_slice(stdout).unwrap();
    v[key].as_str().unwrap().to_owned()
}

#[test]
fn default_actor_consumed_when_flag_absent() {
    // AC #5 / test plan #7.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "annotate:\n  default_actor: alice\n",
    )
    .unwrap();
    let input = isolated_minimal(dir.path());
    let out = dir.path().join("annotated.tape");
    let r = run_in_home(
        dir.path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "hi",
            "-o",
            out.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(r.status.success(), "{r:?}");
    assert_eq!(json_field(&r.stdout, "actor"), "alice");
}

#[test]
fn cli_actor_overrides_default_actor() {
    // AC #5 / test plan #8.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "annotate:\n  default_actor: alice\n",
    )
    .unwrap();
    let input = isolated_minimal(dir.path());
    let out = dir.path().join("annotated.tape");
    let r = run_in_home(
        dir.path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "hi",
            "--actor",
            "bob",
            "-o",
            out.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(r.status.success(), "{r:?}");
    assert_eq!(json_field(&r.stdout, "actor"), "bob");
}

#[test]
fn builtin_actor_default_falls_back_to_user_env() {
    // AC #5 / test plan #9 — no `.taperc`, USER is set, builtin
    // default fires.
    let dir = tempfile::tempdir().unwrap();
    let input = isolated_minimal(dir.path());
    let out = dir.path().join("annotated.tape");
    let mut cmd = Command::new(binary_path());
    cmd.args([
        "annotate",
        input.to_str().unwrap(),
        "--note",
        "hi",
        "-o",
        out.to_str().unwrap(),
        "--json",
    ])
    .env_remove("HOME")
    .env("HOME", dir.path())
    .env("USER", "carol")
    .current_dir(dir.path());
    let r = cmd.output().unwrap();
    assert!(r.status.success(), "{r:?}");
    assert_eq!(json_field(&r.stdout, "actor"), "carol");
}

#[test]
fn default_by_consumed_when_flag_absent() {
    // AC #6 / test plan #10. Verifies the clap default_value = "human"
    // was actually removed — otherwise clap would force "human"
    // before our resolver runs.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "annotate:\n  default_by: agent\n",
    )
    .unwrap();
    let input = isolated_minimal(dir.path());
    let out = dir.path().join("annotated.tape");
    let r = run_in_home(
        dir.path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "hi",
            "-o",
            out.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(r.status.success(), "{r:?}");
    assert_eq!(json_field(&r.stdout, "by"), "agent");
}

#[test]
fn cli_by_overrides_default_by() {
    // AC #6 / test plan #11.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "annotate:\n  default_by: agent\n",
    )
    .unwrap();
    let input = isolated_minimal(dir.path());
    let out = dir.path().join("annotated.tape");
    let r = run_in_home(
        dir.path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "hi",
            "--by",
            "human",
            "-o",
            out.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(r.status.success(), "{r:?}");
    assert_eq!(json_field(&r.stdout, "by"), "human");
}

#[test]
fn invalid_default_by_in_taperc_exits_two() {
    // AC #6 / test plan #12.
    let dir = tempfile::tempdir().unwrap();
    let taperc = dir.path().join(".taperc");
    std::fs::write(&taperc, "annotate:\n  default_by: humans\n").unwrap();
    let input = isolated_minimal(dir.path());
    let out = dir.path().join("annotated.tape");
    let r = run_in_home(
        dir.path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "hi",
            "-o",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains(taperc.to_string_lossy().as_ref()),
        ".taperc path must appear in diagnostic: {stderr}"
    );
    assert!(
        stderr.contains("humans"),
        "rejected value must appear in diagnostic: {stderr}"
    );
}

#[test]
fn taperc_editor_invoked_for_editor_flag() {
    use std::os::unix::fs::PermissionsExt;
    // AC #7 / test plan #13. Provide a shell script as the
    // `.taperc::annotate.editor` value; the script writes a known
    // body into its argv[1] (the temp file path).
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fake-editor.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf 'taperc-editor wrote this\\n' > \"$1\"\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        format!("annotate:\n  editor: {}\n", script.to_string_lossy(),),
    )
    .unwrap();
    let input = isolated_minimal(dir.path());
    let out = dir.path().join("annotated.tape");
    // Note: the env-vars VISUAL / EDITOR are deliberately cleared to
    // confirm the `.taperc` value takes precedence over the empty
    // env-chain as well as the populated one.
    let mut cmd = Command::new(binary_path());
    cmd.args([
        "annotate",
        input.to_str().unwrap(),
        "--editor",
        "-o",
        out.to_str().unwrap(),
    ])
    .env_remove("HOME")
    .env("HOME", dir.path())
    .env_remove("VISUAL")
    .env_remove("EDITOR")
    .current_dir(dir.path());
    let r = cmd.output().unwrap();
    assert!(
        r.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&r.stderr)
    );

    // Pull the rendered note from the new annotation track.
    let raw = tape_format::reader::RawTape::open(&out).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(&raw.tracks_jsonl.unwrap()).unwrap();
    let annot = tracks
        .iter()
        .rev()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation)
        .unwrap();
    assert_eq!(annot.payload["note"], "taperc-editor wrote this");
}

#[test]
fn taperc_editor_precedence_over_visual_env() {
    use std::os::unix::fs::PermissionsExt;
    // AC #7 / test plan #14. With both `.taperc::annotate.editor`
    // and `$VISUAL` set, the `.taperc` value wins. `$VISUAL` is
    // pointed at /usr/bin/false (would exit non-zero), so the test
    // only succeeds when the script-based `.taperc` editor is the
    // one actually invoked.
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fake-editor.sh");
    std::fs::write(&script, "#!/bin/sh\nprintf 'taperc wins\\n' > \"$1\"\n").unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        format!("annotate:\n  editor: {}\n", script.to_string_lossy(),),
    )
    .unwrap();
    let input = isolated_minimal(dir.path());
    let out = dir.path().join("annotated.tape");
    let mut cmd = Command::new(binary_path());
    cmd.args([
        "annotate",
        input.to_str().unwrap(),
        "--editor",
        "-o",
        out.to_str().unwrap(),
    ])
    .env_remove("HOME")
    .env("HOME", dir.path())
    .env("VISUAL", "/usr/bin/false")
    .env_remove("EDITOR")
    .current_dir(dir.path());
    let r = cmd.output().unwrap();
    assert!(
        r.status.success(),
        ".taperc editor should override $VISUAL — stderr: {}",
        String::from_utf8_lossy(&r.stderr)
    );
}

#[test]
fn taperc_editor_dormant_when_editor_flag_absent() {
    // AC #7 / test plan #15. The `.taperc::annotate.editor` value is
    // set to /usr/bin/false so any accidental invocation would fail.
    // Without `--editor`, the editor field stays dormant; the
    // `--note` path produces a normal annotation.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "annotate:\n  editor: /usr/bin/false\n",
    )
    .unwrap();
    let input = isolated_minimal(dir.path());
    let out = dir.path().join("annotated.tape");
    let r = run_in_home(
        dir.path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "x",
            "-o",
            out.to_str().unwrap(),
        ],
    );
    assert!(
        r.status.success(),
        "editor field should be dormant without --editor — stderr: {}",
        String::from_utf8_lossy(&r.stderr)
    );
}
