//! End-to-end coverage for `tape policy` Phase 1 (issue #227,
//! carved from #110). Builds cassettes via
//! `tape_format::writer::PendingTape::write_to` (the same pattern
//! used by `recap_integration.rs` / `relinernote_integration.rs` /
//! `compact_phase1.rs`), then shells out to the binary.
//!
//! Asserts:
//! - all-pass cassette + all three keys true → exit 0
//! - empty `[require]` → exit 0 with `0 rules checked`
//! - no `[require]` at all → exit 0 with `0 rules checked`
//! - each rule fails independently when meta is missing it → exit 2
//! - unknown key under `[require]` (`recpa`) → exit 2 with key
//! - unknown top-level table (`[forbid]`) → exit 2 with table
//! - malformed TOML → exit 2 with file path
//! - missing cassette → exit 2 with file path
//! - `--help` documents the format

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

const STD_LINER: &str = "## What I was asked to do\nx\n\n\
                         ## What I found\ny\n\n\
                         ## Suggested next step / fix\nz\n\n\
                         ## What I'm uncertain about\nnothing\n";

const STD_TRACKS: &str = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"go\"}}
{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"outcome\":\"success\"}}
";

fn meta_yaml_with(recap: Option<&str>, tags: &[&str]) -> String {
    let mut out = String::from(
        "tape_version: \"tape/v0\"\n\
         id: \"01h8xy00-0000-7000-b8aa-000000000227\"\n\
         created_at: \"2026-05-16T00:00:00Z\"\n\
         ejected_at: \"2026-05-16T00:00:01Z\"\n\
         task: \"policy test\"\n\
         recorder:\n  agent: \"test/0.0.1\"\n\
         outcome: success\n",
    );
    if let Some(r) = recap {
        out.push_str(&format!("recap: \"{r}\"\n"));
    }
    if !tags.is_empty() {
        out.push_str("tags:\n");
        for t in tags {
            out.push_str(&format!("  - {t}\n"));
        }
    }
    out
}

fn build_cassette(
    dir: &Path,
    name: &str,
    recap: Option<&str>,
    tags: &[&str],
    liner: Option<&str>,
) -> PathBuf {
    let path = dir.join(name);
    let pending = tape_format::writer::PendingTape {
        meta_yaml: meta_yaml_with(recap, tags),
        liner_md: liner.unwrap_or("").to_owned(),
        tracks_jsonl: STD_TRACKS.to_owned(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(&path).unwrap();
    path
}

fn write(p: &Path, body: &str) {
    std::fs::write(p, body).unwrap();
}

fn run(cassette: &Path, policy: &Path) -> std::process::Output {
    std::process::Command::new(binary_path())
        .args([
            "policy",
            cassette.to_str().unwrap(),
            "--policy",
            policy.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

#[test]
fn all_three_pass_when_meta_and_liner_populated() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = build_cassette(
        tmp.path(),
        "ok.tape",
        Some("ok"),
        &["billing"],
        Some(STD_LINER),
    );
    let policy = tmp.path().join("p.toml");
    write(
        &policy,
        "[require]\nrecap = true\ntags = true\nliner_notes = true\n",
    );

    let r = run(&cassette, &policy);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("recap: pass"), "stdout: {stdout}");
    assert!(stdout.contains("tags: pass"), "stdout: {stdout}");
    assert!(stdout.contains("liner_notes: pass"), "stdout: {stdout}");
    assert!(
        stdout.contains("3 rules checked: 3 passed, 0 failed"),
        "stdout: {stdout}"
    );
}

#[test]
fn empty_require_block_exits_zero_with_no_rules_checked() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = build_cassette(tmp.path(), "x.tape", None, &[], Some(STD_LINER));
    let policy = tmp.path().join("p.toml");
    write(&policy, "[require]\n");

    let r = run(&cassette, &policy);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("0 rules checked: 0 passed, 0 failed"),
        "stdout: {stdout}"
    );
}

#[test]
fn no_require_section_at_all_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = build_cassette(tmp.path(), "x.tape", None, &[], Some(STD_LINER));
    let policy = tmp.path().join("p.toml");
    write(&policy, "");

    let r = run(&cassette, &policy);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("0 rules checked"), "stdout: {stdout}");
}

#[test]
fn missing_recap_fails_when_required() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = build_cassette(
        tmp.path(),
        "no-recap.tape",
        None,
        &["billing"],
        Some(STD_LINER),
    );
    let policy = tmp.path().join("p.toml");
    write(&policy, "[require]\nrecap = true\n");

    let r = run(&cassette, &policy);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("recap: fail"), "stdout: {stdout}");
    assert!(stdout.contains("absent or empty"), "stdout: {stdout}");
}

#[test]
fn empty_tags_fail_when_required() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = build_cassette(tmp.path(), "no-tags.tape", Some("ok"), &[], Some(STD_LINER));
    let policy = tmp.path().join("p.toml");
    write(&policy, "[require]\ntags = true\n");

    let r = run(&cassette, &policy);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("tags: fail"), "stdout: {stdout}");
    assert!(stdout.contains("meta.tags is empty"), "stdout: {stdout}");
}

#[test]
fn empty_liner_notes_fail_when_required() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = build_cassette(
        tmp.path(),
        "no-liner.tape",
        Some("ok"),
        &["billing"],
        Some("   \n  "),
    );
    let policy = tmp.path().join("p.toml");
    write(&policy, "[require]\nliner_notes = true\n");

    let r = run(&cassette, &policy);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("liner_notes: fail"), "stdout: {stdout}");
}

#[test]
fn unknown_key_under_require_exits_two_with_key_named() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = build_cassette(tmp.path(), "x.tape", Some("ok"), &["t"], Some(STD_LINER));
    let policy = tmp.path().join("p.toml");
    write(&policy, "[require]\nrecpa = true\n");

    let r = run(&cassette, &policy);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("recpa"),
        "stderr should name the key: {stderr}"
    );
}

#[test]
fn unknown_top_level_table_exits_two_with_table_named() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = build_cassette(tmp.path(), "x.tape", Some("ok"), &["t"], Some(STD_LINER));
    let policy = tmp.path().join("p.toml");
    // `[forbid]` is a Phase-2 syntax-from-the-future; Phase 1 must reject.
    write(&policy, "[forbid]\nrecap = true\n");

    let r = run(&cassette, &policy);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("forbid"),
        "stderr should name the table: {stderr}"
    );
}

#[test]
fn malformed_toml_exits_two_with_file_path() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = build_cassette(tmp.path(), "x.tape", Some("ok"), &["t"], Some(STD_LINER));
    let policy = tmp.path().join("bad.toml");
    write(&policy, "[require\n");

    let r = run(&cassette, &policy);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("bad.toml"), "stderr: {stderr}");
}

#[test]
fn missing_cassette_exits_two_with_path() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("nope.tape");
    let policy = tmp.path().join("p.toml");
    write(&policy, "[require]\nrecap = true\n");

    let r = run(&cassette, &policy);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("nope.tape"), "stderr: {stderr}");
}

#[test]
fn key_set_to_false_is_equivalent_to_omitted() {
    let tmp = tempfile::tempdir().unwrap();
    // Cassette has no recap, but the policy explicitly sets
    // `recap = false` — the rule should not fire and exit is 0.
    let cassette = build_cassette(
        tmp.path(),
        "no-recap.tape",
        None,
        &["billing"],
        Some(STD_LINER),
    );
    let policy = tmp.path().join("p.toml");
    write(
        &policy,
        "[require]\nrecap = false\ntags = true\nliner_notes = true\n",
    );

    let r = run(&cassette, &policy);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        !stdout.contains("recap:"),
        "inactive rules should not be listed: {stdout}"
    );
    assert!(
        stdout.contains("2 rules checked: 2 passed, 0 failed"),
        "stdout: {stdout}"
    );
}

#[test]
fn help_documents_the_format() {
    let r = std::process::Command::new(binary_path())
        .args(["policy", "--help"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let lower = stdout.to_lowercase();
    assert!(lower.contains("toml"), "help: {stdout}");
    assert!(lower.contains("require"), "help: {stdout}");
    assert!(lower.contains("recap"), "help: {stdout}");
    assert!(lower.contains("tags"), "help: {stdout}");
    assert!(lower.contains("liner_notes"), "help: {stdout}");
}
