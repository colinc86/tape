//! End-to-end coverage for `tape view` Phase 1 (issue #254, carved
//! from #67). Uses `tests/fixtures/killer-scenario-a.tape` for the
//! rich happy-path coverage (task + model_call + mcp_call +
//! annotation + eject), and `tests/fixtures/minimal-success.tape`
//! for the minimal index-summary case. Hand-builds extra cassettes
//! via `PendingTape::write_to` for the redaction-applied,
//! parent_step, and empty-cassette coverage that no on-disk fixture
//! covers today.
//!
//! Asserts:
//! - detail page (`--track N`) on a known step: exit 0, header,
//!   every applicable field, payload divider
//! - detail page surfaces `parent_step` when present, omits when None
//! - detail page renders all three `RedactionStatus` variants
//! - detail page renders the annotation list when non-empty
//! - `--track N` for absent N exits 1 with stderr naming N
//! - index summary on a known fixture: exit 0, all 8 kinds in
//!   declaration order with zero-count rows
//! - index summary on an empty cassette: exit 0, no first/last ts
//! - missing cassette exits 2 with the path named
//! - malformed cassette exits 2
//! - `--help` documents the subcommand and `--track`

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

fn run(args: &[&str]) -> std::process::Output {
    let mut cmd = std::process::Command::new(binary_path());
    cmd.arg("view");
    for a in args {
        cmd.arg(a);
    }
    cmd.output().unwrap()
}

const STD_META: &str = "tape_version: \"tape/v0\"\n\
                        id: \"01h8xy00-0000-7000-b8aa-000000000254\"\n\
                        created_at: \"2026-05-16T00:00:00Z\"\n\
                        ejected_at: \"2026-05-16T00:00:30Z\"\n\
                        task: \"view test\"\n\
                        recorder:\n  agent: \"test/0.0.1\"\n\
                        outcome: success\n";

const STD_LINER: &str = "## What I was asked to do\nx\n\n\
                         ## What I found\ny\n\n\
                         ## Suggested next step / fix\nz\n\n\
                         ## What I'm uncertain about\nnothing\n";

fn build_cassette(
    dir: &Path,
    name: &str,
    tracks_jsonl: &str,
    redactions_json: Option<String>,
) -> PathBuf {
    let path = dir.join(name);
    let pending = tape_format::writer::PendingTape {
        meta_yaml: STD_META.to_owned(),
        liner_md: STD_LINER.to_owned(),
        tracks_jsonl: tracks_jsonl.to_owned(),
        redactions_json,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(&path).unwrap();
    path
}

// ---------- detail page (`--track N`) ----------

#[test]
fn detail_page_known_step_exits_zero_with_header_and_payload() {
    let cassette = repo_fixtures().join("killer-scenario-a.tape");
    let r = run(&[cassette.to_str().unwrap(), "--track", "3"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("══ track step 3 ══"), "stdout: {stdout}");
    assert!(
        stdout.contains("kind:           mcp_call"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("step:           3"), "stdout: {stdout}");
    assert!(
        stdout.contains("ts:             2026-05-06T10:00:25Z"),
        "stdout: {stdout}"
    );
    // No parent_step / refs on this track → those lines are omitted.
    assert!(!stdout.contains("parent_step:"), "stdout: {stdout}");
    assert!(!stdout.contains("refs:"), "stdout: {stdout}");
    // The cassette has no redactions.json → "not processed".
    assert!(
        stdout.contains("redaction:      not processed"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("annotations:    0"), "stdout: {stdout}");
    assert!(stdout.contains("── payload ──"), "stdout: {stdout}");
    // Pretty-printed payload surfaces the full structure.
    assert!(stdout.contains("\"server\": \"db\""), "stdout: {stdout}");
    assert!(stdout.contains("\"tool\": \"query\""), "stdout: {stdout}");
    assert!(stdout.contains("customer_id=4471"), "stdout: {stdout}");
}

#[test]
fn detail_page_surfaces_parent_step_when_present() {
    let dir = tempfile::tempdir().unwrap();
    // Hand-build a track with parent_step set; not a valid SPEC cassette
    // (parent must point to an earlier step), but view is read-only and
    // verify isn't invoked here.
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"x\"}}
{\"step\":2,\"kind\":\"model_call\",\"ts\":\"2026-05-16T00:00:01Z\",\"parent_step\":1,\"payload\":{\"vendor\":\"x\",\"model\":\"y\"}}
{\"step\":3,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{\"outcome\":\"success\"}}
";
    let cassette = build_cassette(dir.path(), "parent.tape", tracks, None);
    let r = run(&[cassette.to_str().unwrap(), "--track", "2"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("parent_step:    1"), "stdout: {stdout}");
}

#[test]
fn detail_page_renders_annotation_list_when_non_empty() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"x\"},\"annotations\":[{\"by\":\"colin\",\"note\":\"wrong tool choice\"}]}
{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"outcome\":\"success\"}}
";
    let cassette = build_cassette(dir.path(), "ann.tape", tracks, None);
    let r = run(&[cassette.to_str().unwrap(), "--track", "1"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("annotations:    1"), "stdout: {stdout}");
    assert!(
        stdout.contains(r#"- by: colin    note: "wrong tool choice""#),
        "stdout: {stdout}"
    );
}

#[test]
fn detail_page_renders_redaction_applied_status() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"x\"}}
{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"outcome\":\"success\"}}
";
    // Two-element redactions.json — the `cmd_stats` reader counts the array
    // length, so this exercises the `Applied(2)` branch.
    let redactions = "[{\"rule\":\"email\"},{\"rule\":\"api_key\"}]".to_owned();
    let cassette = build_cassette(dir.path(), "red.tape", tracks, Some(redactions));
    let r = run(&[cassette.to_str().unwrap(), "--track", "1"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("redaction:      2 replacement(s) applied (cassette-wide)"),
        "stdout: {stdout}"
    );
}

#[test]
fn detail_page_renders_redaction_none_applied_status() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"x\"}}
{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"outcome\":\"success\"}}
";
    // Empty array — engine ran, zero hits.
    let cassette = build_cassette(
        dir.path(),
        "noneapplied.tape",
        tracks,
        Some("[]".to_owned()),
    );
    let r = run(&[cassette.to_str().unwrap(), "--track", "1"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("redaction:      none applied"),
        "stdout: {stdout}"
    );
}

#[test]
fn detail_page_missing_step_exits_one_with_stderr_naming_n() {
    let cassette = repo_fixtures().join("killer-scenario-a.tape");
    let r = run(&[cassette.to_str().unwrap(), "--track", "999"]);
    assert!(!r.status.success(), "expected non-zero: {r:?}");
    assert_eq!(r.status.code(), Some(1), "{r:?}");
    let stderr = String::from_utf8(r.stderr).unwrap();
    assert!(stderr.contains("999"), "stderr: {stderr}");
    assert!(stderr.contains("no track"), "stderr: {stderr}");
}

// ---------- index summary (no flag) ----------

#[test]
fn index_summary_known_fixture_exits_zero_with_header_and_histogram() {
    let cassette = repo_fixtures().join("killer-scenario-a.tape");
    let r = run(&[cassette.to_str().unwrap()]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("══ tape index: killer-scenario-a.tape ══"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("tracks:         6"), "stdout: {stdout}");
    assert!(
        stdout.contains("first_ts:       2026-05-06T10:00:00Z"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("last_ts:        2026-05-06T10:01:00Z"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("redactions:     not processed"),
        "stdout: {stdout}"
    );
    // Histogram lists all eight kinds, with zero-count rows for the absent ones.
    assert!(stdout.contains("── kind histogram ──"), "stdout: {stdout}");
    for kind in [
        "task",
        "model_call",
        "mcp_call",
        "shell",
        "file_read",
        "file_write",
        "annotation",
        "eject",
    ] {
        assert!(stdout.contains(kind), "missing kind {kind}: {stdout}");
    }
    // killer-scenario-a has: 1 task + 2 model_call + 1 mcp_call + 1 annotation + 1 eject.
    assert!(stdout.contains("task          1"), "stdout: {stdout}");
    assert!(stdout.contains("model_call    2"), "stdout: {stdout}");
    assert!(stdout.contains("mcp_call      1"), "stdout: {stdout}");
    assert!(stdout.contains("shell         0"), "stdout: {stdout}");
    assert!(stdout.contains("file_read     0"), "stdout: {stdout}");
    assert!(stdout.contains("file_write    0"), "stdout: {stdout}");
    assert!(stdout.contains("annotation    1"), "stdout: {stdout}");
    assert!(stdout.contains("eject         1"), "stdout: {stdout}");
}

#[test]
fn index_summary_minimal_fixture() {
    let cassette = repo_fixtures().join("minimal-success.tape");
    let r = run(&[cassette.to_str().unwrap()]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("tracks:         3"), "stdout: {stdout}");
    // minimal-success: 1 task + 1 model_call + 1 eject; mcp_call etc. show 0.
    assert!(stdout.contains("task          1"), "stdout: {stdout}");
    assert!(stdout.contains("model_call    1"), "stdout: {stdout}");
    assert!(stdout.contains("eject         1"), "stdout: {stdout}");
    assert!(stdout.contains("mcp_call      0"), "stdout: {stdout}");
}

#[test]
fn index_summary_renders_redaction_applied_count() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"x\"}}
{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"outcome\":\"success\"}}
";
    let redactions = "[{\"rule\":\"email\"},{\"rule\":\"jwt\"},{\"rule\":\"key\"}]".to_owned();
    let cassette = build_cassette(dir.path(), "red.tape", tracks, Some(redactions));
    let r = run(&[cassette.to_str().unwrap()]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("redactions:     3 (cassette-wide)"),
        "stdout: {stdout}"
    );
}

#[test]
fn index_summary_empty_cassette_omits_ts_lines() {
    let dir = tempfile::tempdir().unwrap();
    // Zero-track cassette: SPEC-illegal (no task / no eject) but
    // view is strictly read-side and must not panic.
    let cassette = build_cassette(dir.path(), "empty.tape", "", None);
    let r = run(&[cassette.to_str().unwrap()]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("tracks:         0"), "stdout: {stdout}");
    assert!(!stdout.contains("first_ts:"), "stdout: {stdout}");
    assert!(!stdout.contains("last_ts:"), "stdout: {stdout}");
    // Histogram still printed with all-zero rows.
    assert!(stdout.contains("task          0"), "stdout: {stdout}");
    assert!(stdout.contains("eject         0"), "stdout: {stdout}");
}

// ---------- error paths ----------

#[test]
fn missing_cassette_exits_two() {
    let r = run(&["/nonexistent/path/to/no-such.tape"]);
    assert!(!r.status.success(), "expected non-zero: {r:?}");
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8(r.stderr).unwrap();
    assert!(
        stderr.contains("/nonexistent/path/to/no-such.tape"),
        "stderr: {stderr}"
    );
}

#[test]
fn malformed_cassette_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.tape");
    std::fs::write(&bad, b"not a zip file").unwrap();
    let r = run(&[bad.to_str().unwrap()]);
    assert!(!r.status.success(), "expected non-zero: {r:?}");
    assert_eq!(r.status.code(), Some(2), "{r:?}");
}

#[test]
fn help_documents_subcommand_and_track_flag() {
    let r = run(&["--help"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("--track"), "stdout: {stdout}");
    // The doc-comment differentiates from sibling read-side verbs.
    assert!(
        stdout.contains("inspector") || stdout.contains("detail"),
        "stdout: {stdout}"
    );
}
