//! End-to-end coverage for `tape playlist` Phase 1 (issue #221,
//! carved from #78). Builds `.tapelist` files at runtime in a
//! tempdir using copies of `tests/fixtures/minimal-success.tape`
//! (valid) and `tests/fixtures/malformed/outcome-mismatch.tape`
//! (invalid). Asserts:
//! - all-valid list → exit 0, three `[OK]` lines + summary
//! - mixed list (OK + missing + invalid) → exit 1, one of each + summary
//! - empty / comment-only list → exit 0
//! - relative paths resolve against the `.tapelist`'s parent, not CWD
//! - unreadable `.tapelist` → exit 2 with stderr mention of the path
//! - `--help` documents the format

use std::path::{Path, PathBuf};

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

fn repo_fixtures() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/tape-cli/; fixtures live at repo
    // root tests/fixtures/.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
}

fn copy_to(dir: &Path, src: &Path, dest_name: &str) -> PathBuf {
    let dest = dir.join(dest_name);
    std::fs::copy(src, &dest).expect("copy fixture");
    dest
}

#[test]
fn all_valid_entries_exit_zero_with_three_ok_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let valid = repo_fixtures().join("minimal-success.tape");
    copy_to(tmp.path(), &valid, "a.tape");
    copy_to(tmp.path(), &valid, "b.tape");
    copy_to(tmp.path(), &valid, "c.tape");
    let list = tmp.path().join("valid.tapelist");
    std::fs::write(&list, "# all good\n./a.tape\n./b.tape\n\n./c.tape\n").unwrap();

    let r = std::process::Command::new(binary_path())
        .args(["playlist", list.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert_eq!(stdout.matches("[OK]").count(), 3, "stdout: {stdout}");
    assert!(stdout.contains("3 OK, 0 missing, 0 invalid (3 total)"));
}

#[test]
fn mixed_entries_exit_one_with_one_of_each() {
    let tmp = tempfile::tempdir().unwrap();
    let valid = repo_fixtures().join("minimal-success.tape");
    let invalid = repo_fixtures()
        .join("malformed")
        .join("outcome-mismatch.tape");
    copy_to(tmp.path(), &valid, "good.tape");
    copy_to(tmp.path(), &invalid, "bad.tape");
    // Note: ./absent.tape is deliberately NOT created.
    let list = tmp.path().join("mixed.tapelist");
    std::fs::write(&list, "./good.tape\n./absent.tape\n./bad.tape\n").unwrap();

    let r = std::process::Command::new(binary_path())
        .args(["playlist", list.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(1), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert_eq!(stdout.matches("[OK]").count(), 1, "stdout: {stdout}");
    assert_eq!(stdout.matches("[MISSING]").count(), 1, "stdout: {stdout}");
    assert_eq!(stdout.matches("[INVALID]").count(), 1, "stdout: {stdout}");
    assert!(stdout.contains("1 OK, 1 missing, 1 invalid (3 total)"));
}

#[test]
fn comment_only_playlist_exits_zero_with_empty_summary() {
    let tmp = tempfile::tempdir().unwrap();
    let list = tmp.path().join("empty.tapelist");
    std::fs::write(&list, "# nothing here\n\n   # also nothing\n").unwrap();

    let r = std::process::Command::new(binary_path())
        .args(["playlist", list.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("0 OK, 0 missing, 0 invalid (0 total)"));
}

#[test]
fn relative_paths_resolve_against_tapelist_parent_not_cwd() {
    let tmp = tempfile::tempdir().unwrap();
    // Place the `.tapelist` and cassette in a subdir; run the
    // binary from a DIFFERENT cwd. If we resolved against cwd we'd
    // get [MISSING]; we expect [OK].
    let pl_dir = tmp.path().join("playlist-dir");
    std::fs::create_dir_all(&pl_dir).unwrap();
    let valid = repo_fixtures().join("minimal-success.tape");
    copy_to(&pl_dir, &valid, "local.tape");
    let list = pl_dir.join("rel.tapelist");
    std::fs::write(&list, "./local.tape\n").unwrap();

    // Run from a foreign CWD: tmp root, where ./local.tape does NOT exist.
    let r = std::process::Command::new(binary_path())
        .args(["playlist", list.to_str().unwrap()])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("[OK]"),
        "relative path should resolve against tapelist dir; stdout: {stdout}"
    );
}

#[test]
fn unreadable_playlist_file_exits_two() {
    let tmp = tempfile::tempdir().unwrap();
    let nope = tmp.path().join("nope.tapelist");
    let r = std::process::Command::new(binary_path())
        .args(["playlist", nope.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("nope.tapelist"),
        "stderr should mention the path: {stderr}"
    );
}

#[test]
fn duplicate_entries_are_validated_independently() {
    let tmp = tempfile::tempdir().unwrap();
    let valid = repo_fixtures().join("minimal-success.tape");
    copy_to(tmp.path(), &valid, "dup.tape");
    let list = tmp.path().join("dup.tapelist");
    std::fs::write(&list, "./dup.tape\n./dup.tape\n").unwrap();

    let r = std::process::Command::new(binary_path())
        .args(["playlist", list.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert_eq!(stdout.matches("[OK]").count(), 2, "stdout: {stdout}");
    assert!(stdout.contains("2 OK, 0 missing, 0 invalid (2 total)"));
}

#[test]
fn help_documents_the_format() {
    let r = std::process::Command::new(binary_path())
        .args(["playlist", "--help"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    // Format documentation: comment grammar, relative-resolution, exit codes.
    let lower = stdout.to_lowercase();
    assert!(lower.contains("comment"), "help: {stdout}");
    assert!(lower.contains("relative"), "help: {stdout}");
    assert!(lower.contains("exit code"), "help: {stdout}");
}
