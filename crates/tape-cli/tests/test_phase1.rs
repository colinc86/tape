//! End-to-end coverage for `tape test` Phase 1 (issue #252, carved
//! from #10). Uses tempdir copies of `tests/fixtures/minimal-success.tape`
//! for the happy-path and a hand-built second cassette for the
//! divergence cases.
//!
//! Asserts:
//! - identical cassettes → exit 0, `4/4 passed`
//! - different track counts → exit 2 with `track count: FAIL`
//! - different kind sequence → exit 2 with `kind sequence: FAIL`
//!   naming the first divergent index
//! - different task → exit 2 with `task prompt: FAIL`
//! - different outcome → exit 2 with `eject outcome: FAIL`
//! - missing cassette → non-zero exit (anyhow default = 1, not the
//!   exit-2 reserved for "comparison ran, a check failed")
//! - `--help` mentions Phase 1 + #10

use std::collections::BTreeMap;
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

fn copy_minimal_to(dest: &Path) {
    let src = repo_fixtures().join("minimal-success.tape");
    std::fs::copy(&src, dest).unwrap();
}

const STD_LINER: &str = "## What I was asked to do\nx\n\n\
                         ## What I found\ny\n\n\
                         ## Suggested next step / fix\nz\n\n\
                         ## What I'm uncertain about\nnothing\n";

const STD_META_TEMPLATE: &str = "tape_version: \"tape/v0\"\n\
                                 id: \"01h8xy00-0000-7000-b8aa-000000000252\"\n\
                                 created_at: \"2026-05-16T00:00:00Z\"\n\
                                 ejected_at: \"2026-05-16T00:00:01Z\"\n\
                                 task: \"TASK_PLACEHOLDER\"\n\
                                 recorder:\n  agent: \"test/0.0.1\"\n\
                                 outcome: OUTCOME_PLACEHOLDER\n";

/// Build a cassette with the given task, outcome, and track Kind
/// sequence (task is implicit step 1; eject is the final step).
fn build_cassette(
    dir: &Path,
    name: &str,
    task: &str,
    outcome: &str,
    middle_kinds: &[&str],
) -> PathBuf {
    let path = dir.join(name);
    let meta = STD_META_TEMPLATE
        .replace("TASK_PLACEHOLDER", task)
        .replace("OUTCOME_PLACEHOLDER", outcome);
    let mut tracks = format!(
        "{{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{{\"prompt\":\"{task}\"}}}}\n"
    );
    let mut step = 2u64;
    for k in middle_kinds {
        let payload = match *k {
            "shell" => "{\"cmd\":\"ls\"}",
            "model_call" => "{\"vendor\":\"anthropic\",\"model\":\"claude-haiku-4-5\"}",
            "mcp_call" => "{\"server\":\"x\",\"tool\":\"y\"}",
            _ => "{}",
        };
        tracks.push_str(&format!(
            "{{\"step\":{step},\"kind\":\"{k}\",\"ts\":\"2026-05-16T00:00:{step:02}Z\",\"payload\":{payload}}}\n"
        ));
        step += 1;
    }
    tracks.push_str(&format!(
        "{{\"step\":{step},\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:{step:02}Z\",\"payload\":{{\"outcome\":\"{outcome}\"}}}}\n"
    ));
    let pending = tape_format::writer::PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.to_owned(),
        tracks_jsonl: tracks,
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(&path).unwrap();
    path
}

fn run(args: &[&str]) -> std::process::Output {
    std::process::Command::new(binary_path())
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn identical_cassettes_exit_zero_with_four_of_four_passed() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.tape");
    let b = tmp.path().join("b.tape");
    copy_minimal_to(&a);
    copy_minimal_to(&b);
    let r = run(&["test", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("4/4 passed"), "stdout: {stdout}");
    assert!(stdout.contains("track count: PASS"), "stdout: {stdout}");
    assert!(stdout.contains("kind sequence: PASS"), "stdout: {stdout}");
    assert!(stdout.contains("task prompt: PASS"), "stdout: {stdout}");
    assert!(stdout.contains("eject outcome: PASS"), "stdout: {stdout}");
}

#[test]
fn different_track_count_exits_two_with_count_detail() {
    let tmp = tempfile::tempdir().unwrap();
    let a = build_cassette(tmp.path(), "a.tape", "investigate", "success", &["shell"]);
    let b = build_cassette(
        tmp.path(),
        "b.tape",
        "investigate",
        "success",
        &["shell", "shell"],
    );
    let r = run(&["test", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("track count: FAIL"), "stdout: {stdout}");
    assert!(stdout.contains("a=3"), "stdout: {stdout}");
    assert!(stdout.contains("b=4"), "stdout: {stdout}");
    // Task and outcome are independent of track count → both still
    // pass. Kind sequence necessarily fails too because b's extra
    // track shifts the trailing eject's position.
    assert!(stdout.contains("task prompt: PASS"), "stdout: {stdout}");
    assert!(stdout.contains("eject outcome: PASS"), "stdout: {stdout}");
    assert!(stdout.contains("2/4 passed"), "stdout: {stdout}");
}

#[test]
fn different_kind_sequence_exits_two_with_index_and_kinds() {
    let tmp = tempfile::tempdir().unwrap();
    let a = build_cassette(
        tmp.path(),
        "a.tape",
        "go",
        "success",
        &["shell", "model_call"],
    );
    let b = build_cassette(
        tmp.path(),
        "b.tape",
        "go",
        "success",
        &["model_call", "model_call"],
    );
    let r = run(&["test", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("kind sequence: FAIL"), "stdout: {stdout}");
    assert!(stdout.contains("index 1"), "stdout: {stdout}");
    assert!(stdout.contains("a=shell"), "stdout: {stdout}");
    assert!(stdout.contains("b=model_call"), "stdout: {stdout}");
}

#[test]
fn different_task_exits_two_with_truncated_diff() {
    let tmp = tempfile::tempdir().unwrap();
    let a = build_cassette(
        tmp.path(),
        "a.tape",
        "billing investigation",
        "success",
        &[],
    );
    let b = build_cassette(
        tmp.path(),
        "b.tape",
        "inventory investigation",
        "success",
        &[],
    );
    let r = run(&["test", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("task prompt: FAIL"), "stdout: {stdout}");
    assert!(stdout.contains("billing"), "stdout: {stdout}");
    assert!(stdout.contains("inventory"), "stdout: {stdout}");
}

#[test]
fn different_outcome_exits_two_with_lowercase_strings() {
    let tmp = tempfile::tempdir().unwrap();
    let a = build_cassette(tmp.path(), "a.tape", "go", "success", &[]);
    let b = build_cassette(tmp.path(), "b.tape", "go", "failure", &[]);
    let r = run(&["test", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("eject outcome: FAIL (a=success, b=failure)"),
        "stdout: {stdout}"
    );
}

#[test]
fn missing_cassette_exits_one_not_two() {
    // Phase 1 reserves exit 2 for "comparison ran and at least one
    // check failed". Loader errors (file not found, parse error)
    // propagate through anyhow and land at the default exit 1.
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("real.tape");
    copy_minimal_to(&a);
    let nope = tmp.path().join("absent.tape");
    let r = run(&["test", a.to_str().unwrap(), nope.to_str().unwrap()]);
    assert!(!r.status.success(), "{r:?}");
    assert_ne!(
        r.status.code(),
        Some(2),
        "missing file should NOT be exit 2 (that's reserved for failed checks): {r:?}"
    );
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("absent.tape"), "stderr: {stderr}");
}

#[test]
fn help_documents_phase_1_and_links_umbrella_issue() {
    let r = run(&["test", "--help"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let lower = stdout.to_lowercase();
    assert!(lower.contains("phase 1"), "help: {stdout}");
    assert!(lower.contains("#10"), "help: {stdout}");
}
