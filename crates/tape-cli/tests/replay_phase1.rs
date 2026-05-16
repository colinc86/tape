//! End-to-end coverage for `tape replay` Phase 1 (issue #232,
//! carved from #101). Uses `tests/fixtures/minimal-success.tape`
//! for the happy-path cases (no fixture mutation — replay is
//! strictly read-only). Asserts:
//! - default replay (no flags) exits 0 with one header line per
//!   track in source order
//! - `--step N` (existing) exits 0 with exactly one matching header
//! - `--step N` (non-existent) exits 1 with stderr naming N
//! - missing cassette → exit 2 with stderr naming the path
//! - malformed cassette → exit 2
//! - `--help` documents the subcommand and `--step`
//!
//! Deliberately NO wall-clock timing assertion — flaky in CI per
//! the ticket. The 500 ms pause is a UX choice, not a contract.

use std::path::{Path, PathBuf};

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

fn repo_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
}

fn run(args: &[&str]) -> std::process::Output {
    let mut cmd = std::process::Command::new(binary_path());
    cmd.arg("replay");
    for a in args {
        cmd.arg(a);
    }
    cmd.output().unwrap()
}

#[test]
fn default_replay_prints_every_track_in_source_order() {
    let cassette = repo_fixtures().join("minimal-success.tape");
    let r = run(&[cassette.to_str().unwrap()]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    // Each track produces a header line. Count them and verify
    // they're in step order (step 1 appears before step 2, etc.).
    let header_lines: Vec<&str> = stdout
        .lines()
        .filter(|l| l.starts_with("── step "))
        .collect();
    assert!(!header_lines.is_empty(), "stdout: {stdout}");
    // First header should be step 1 (the task event per SPEC §5.4).
    assert!(
        header_lines[0].starts_with("── step 1 · task ·"),
        "first header: {}",
        header_lines[0]
    );
    // Last header should be an eject (also SPEC §5.4).
    let last = header_lines.last().unwrap();
    assert!(
        last.contains(" · eject · "),
        "last header should be eject: {last}"
    );
    // Step numbers should be strictly increasing.
    let steps: Vec<u64> = header_lines
        .iter()
        .map(|l| {
            l.strip_prefix("── step ")
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect();
    for w in steps.windows(2) {
        assert!(w[0] < w[1], "steps not increasing: {steps:?}");
    }
}

#[test]
fn step_flag_with_existing_step_prints_exactly_one_block() {
    let cassette = repo_fixtures().join("minimal-success.tape");
    let r = run(&[cassette.to_str().unwrap(), "--step", "1"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let header_lines: Vec<&str> = stdout
        .lines()
        .filter(|l| l.starts_with("── step "))
        .collect();
    assert_eq!(header_lines.len(), 1, "stdout: {stdout}");
    assert!(
        header_lines[0].starts_with("── step 1 ·"),
        "stdout: {stdout}"
    );
}

#[test]
fn step_flag_with_missing_step_exits_one_with_stderr_naming_n() {
    let cassette = repo_fixtures().join("minimal-success.tape");
    let r = run(&[cassette.to_str().unwrap(), "--step", "9999"]);
    assert_eq!(r.status.code(), Some(1), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("9999"), "stderr should name 9999: {stderr}");
    assert!(stderr.contains("no track with step"), "stderr: {stderr}");
}

#[test]
fn missing_cassette_exits_two() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("nope.tape");
    let r = run(&[missing.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("nope.tape"),
        "stderr should name path: {stderr}"
    );
}

#[test]
fn malformed_cassette_exits_two() {
    // Pick any malformed fixture; the parse-or-open failure will
    // surface as exit 2.
    let malformed = repo_fixtures()
        .join("malformed")
        .join("outcome-mismatch.tape");
    // outcome-mismatch is a valid zip with bad meta — replay's
    // open succeeds, but parse_jsonl would still work too. To
    // actually hit the exit-2 path, use a non-zip file.
    let tmp = tempfile::tempdir().unwrap();
    let not_a_zip = tmp.path().join("garbage.tape");
    std::fs::write(&not_a_zip, b"this is not a zip").unwrap();
    let r = run(&[not_a_zip.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    // outcome-mismatch should NOT itself trigger exit 2 (it's a
    // valid zip with valid tracks.jsonl — the mismatch is in meta).
    let r2 = run(&[malformed.to_str().unwrap()]);
    // Replay doesn't run verify; it just walks tracks. Exit 0 is
    // expected even for a verify-failing-but-parseable cassette.
    assert!(r2.status.success(), "{r2:?}");
}

#[test]
fn help_documents_subcommand_and_step_flag() {
    let r = std::process::Command::new(binary_path())
        .args(["replay", "--help"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let lower = stdout.to_lowercase();
    assert!(lower.contains("--step"), "help: {stdout}");
    assert!(
        lower.contains("phase 1") || lower.contains("chronological") || lower.contains("walk"),
        "help: {stdout}"
    );
}
