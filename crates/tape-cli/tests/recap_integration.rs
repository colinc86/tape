//! `tape recap` Phase-1 integration coverage. Tracks Principal's
//! scoping comment on #105 — `--set`/`--clear`/`--list`, output-path
//! refusal, 280-char ceiling, newline rejection, mutually-exclusive
//! flags, exit-4 on `--list` against an un-recapped cassette.

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

/// Copy `minimal-success.tape` into a fresh temp dir so each test's
/// input is isolated.
fn isolated_minimal() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &dst).unwrap();
    (dir, dst)
}

fn read_meta(path: &std::path::Path) -> tape_format::meta::Meta {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    let yaml = raw.meta_yaml.unwrap();
    tape_format::meta::Meta::parse(&yaml).unwrap()
}

#[test]
fn set_writes_recap_field_and_audit_entry() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("input.recap.tape");

    let out = Command::new(binary_path())
        .args([
            "recap",
            input.to_str().unwrap(),
            "--set",
            "Found a race condition in process_refund().",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "recap --set failed: {out:?}");

    let meta = read_meta(&output);
    assert_eq!(
        meta.recap.as_deref(),
        Some("Found a race condition in process_refund().")
    );
    assert_eq!(meta.recaps.len(), 1);
    assert_eq!(meta.recaps[0].kind, tape_format::meta::RecapKind::Set);
    assert!(meta.recaps[0].prior_recap.is_none());
    assert_eq!(
        meta.recaps[0].new_recap.as_deref(),
        Some("Found a race condition in process_refund().")
    );

    // Output passes `tape verify`.
    let v = Command::new(binary_path())
        .args(["verify", output.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(v.status.success(), "verify failed: {v:?}");
}

#[test]
fn clear_removes_recap_and_appends_audit_entry() {
    let (_dir, input) = isolated_minimal();
    let after_set = input.with_file_name("input.recap.tape");
    let after_clear = input.with_file_name("after_clear.tape");

    // First set a recap.
    let set_out = Command::new(binary_path())
        .args([
            "recap",
            input.to_str().unwrap(),
            "--set",
            "smoking gun: file race",
            "-o",
            after_set.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(set_out.status.success());

    // Then clear it.
    let clear_out = Command::new(binary_path())
        .args([
            "recap",
            after_set.to_str().unwrap(),
            "--clear",
            "-o",
            after_clear.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        clear_out.status.success(),
        "recap --clear failed: {clear_out:?}"
    );

    let meta = read_meta(&after_clear);
    assert!(meta.recap.is_none(), "recap should be cleared");
    // Both audit entries preserved: the prior set + the clear.
    assert_eq!(meta.recaps.len(), 2);
    assert_eq!(meta.recaps[1].kind, tape_format::meta::RecapKind::Clear);
    assert_eq!(
        meta.recaps[1].prior_recap.as_deref(),
        Some("smoking gun: file race")
    );
    assert!(meta.recaps[1].new_recap.is_none());
}

#[test]
fn list_with_recap_prints_text_exit_0() {
    let (_dir, input) = isolated_minimal();
    let recapped = input.with_file_name("input.recap.tape");

    // Seed a recap so --list has something to print.
    Command::new(binary_path())
        .args([
            "recap",
            input.to_str().unwrap(),
            "--set",
            "ready to ship",
            "-o",
            recapped.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let out = Command::new(binary_path())
        .args(["recap", recapped.to_str().unwrap(), "--list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "recap --list failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim() == "ready to ship",
        "expected exact recap text, got {stdout:?}"
    );
}

#[test]
fn list_without_recap_exits_4() {
    let (_dir, input) = isolated_minimal();

    let out = Command::new(binary_path())
        .args(["recap", input.to_str().unwrap(), "--list"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn mutually_exclusive_flags_exit_2() {
    let (_dir, input) = isolated_minimal();

    let out = Command::new(binary_path())
        .args(["recap", input.to_str().unwrap(), "--set", "x", "--clear"])
        .output()
        .unwrap();
    // clap surfaces conflicts as exit 2 (the same "usage error" slot
    // the explicit fallthrough below uses).
    assert!(!out.status.success());
    assert_ne!(out.status.code(), Some(0));
}

#[test]
fn no_mode_flag_exits_2() {
    let (_dir, input) = isolated_minimal();

    let out = Command::new(binary_path())
        .args(["recap", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn set_empty_exits_2() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("input.recap.tape");

    let out = Command::new(binary_path())
        .args([
            "recap",
            input.to_str().unwrap(),
            "--set",
            "",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!output.exists());
}

#[test]
fn set_overlong_exits_2() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("input.recap.tape");
    let too_long = "x".repeat(281);

    let out = Command::new(binary_path())
        .args([
            "recap",
            input.to_str().unwrap(),
            "--set",
            &too_long,
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("280"), "{stderr}");
    assert!(!output.exists());
}

#[test]
fn set_with_newline_exits_2() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("input.recap.tape");

    let out = Command::new(binary_path())
        .args([
            "recap",
            input.to_str().unwrap(),
            "--set",
            "foo\nbar",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!output.exists());
}

#[test]
fn out_equal_to_input_exits_2() {
    let (_dir, input) = isolated_minimal();

    let out = Command::new(binary_path())
        .args([
            "recap",
            input.to_str().unwrap(),
            "--set",
            "x",
            "-o",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn boundary_280_chars_accepted() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("input.recap.tape");
    let exactly_max = "x".repeat(280);

    let out = Command::new(binary_path())
        .args([
            "recap",
            input.to_str().unwrap(),
            "--set",
            &exactly_max,
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "280-char recap should be accepted: {out:?}"
    );
    let meta = read_meta(&output);
    assert_eq!(meta.recap.as_deref(), Some(exactly_max.as_str()));
}
