//! End-to-end coverage for `tape rewind --list` (Phase 1 of issue
//! #85, carved per #213). Hand-builds cassettes via
//! `PendingTape::write_to` and asserts sorted, deterministic output,
//! exit codes, and the `--step` boundary semantics.

use std::collections::BTreeMap;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

const STD_LINER: &str = "## What I was asked to do\nx\n\n\
                         ## What I found\ny\n\n\
                         ## Suggested next step / fix\nz\n\n\
                         ## What I'm uncertain about\nnothing\n";

const STD_META: &str = "tape_version: \"tape/v0\"\n\
                        id: \"01h8xy00-0000-7000-b8aa-000000000213\"\n\
                        created_at: \"2026-05-16T00:00:00Z\"\n\
                        ejected_at: \"2026-05-16T00:00:30Z\"\n\
                        task: \"rewind list test\"\n\
                        recorder:\n  agent: \"test/0.0.1\"\n\
                        outcome: success\n";

/// Build a cassette with the given tracks_jsonl in `dir/name.tape`.
fn build_cassette(dir: &std::path::Path, name: &str, tracks_jsonl: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let pending = tape_format::writer::PendingTape {
        meta_yaml: STD_META.to_owned(),
        liner_md: STD_LINER.to_owned(),
        tracks_jsonl: tracks_jsonl.to_owned(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(&path).unwrap();
    path
}

fn run_rewind(args: &[&str]) -> std::process::Output {
    std::process::Command::new(binary_path())
        .args(args)
        .output()
        .unwrap()
}

/// A cassette with: one task at step 1, one created file at step 2
/// (`/etc/new.conf`), one modified file at step 3 (`/etc/old.conf`),
/// one read-only file at step 4 (`/etc/readme.md`), one write-after-
/// read promotion at step 5 (`/var/log/app.log`), one second-write-
/// after-create promotion at step 6 (`/etc/new.conf` again, modify),
/// and an eject at step 7.
fn rich_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"rewind test\"}}
{\"step\":2,\"kind\":\"file_write\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"path\":\"/etc/new.conf\",\"before_hash\":null,\"after_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000001\"}}
{\"step\":3,\"kind\":\"file_write\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{\"path\":\"/etc/old.conf\",\"before_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000002\",\"after_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000003\"}}
{\"step\":4,\"kind\":\"file_read\",\"ts\":\"2026-05-16T00:00:03Z\",\"payload\":{\"path\":\"/etc/readme.md\",\"content_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000004\"}}
{\"step\":5,\"kind\":\"file_read\",\"ts\":\"2026-05-16T00:00:04Z\",\"payload\":{\"path\":\"/var/log/app.log\",\"content_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000005\"}}
{\"step\":6,\"kind\":\"file_write\",\"ts\":\"2026-05-16T00:00:05Z\",\"payload\":{\"path\":\"/var/log/app.log\",\"before_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000005\",\"after_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000006\"}}
{\"step\":7,\"kind\":\"file_write\",\"ts\":\"2026-05-16T00:00:06Z\",\"payload\":{\"path\":\"/etc/new.conf\",\"before_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000001\",\"after_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000007\"}}
{\"step\":8,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:07Z\",\"payload\":{\"outcome\":\"success\"}}
";
    build_cassette(dir, "rich.tape", tracks)
}

#[test]
fn step_zero_emits_empty_listing_exit_0() {
    // AC #1.
    let dir = tempfile::tempdir().unwrap();
    let fixture = rich_fixture(dir.path());
    let r = run_rewind(&["rewind", fixture.to_str().unwrap(), "--step", "0", "--list"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8_lossy(&r.stdout);
    assert!(stdout.is_empty(), "expected empty listing; got: {stdout}");
}

#[test]
fn full_walk_classifies_create_modify_read_and_promotions() {
    // AC #2 + the four ticket-flagged status transitions.
    let dir = tempfile::tempdir().unwrap();
    let fixture = rich_fixture(dir.path());
    let r = run_rewind(&["rewind", fixture.to_str().unwrap(), "--step", "8", "--list"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    // Sort: last_step asc, then path asc. Expected:
    //   3 — modified  /etc/old.conf
    //   4 — read      /etc/readme.md
    //   6 — modified  /var/log/app.log  (read at 5 → write at 6 → modified)
    //   7 — modified  /etc/new.conf     (created at 2 → write at 7 → modified)
    let expected = "\
modified\t/etc/old.conf\t3
read\t/etc/readme.md\t4
modified\t/var/log/app.log\t6
modified\t/etc/new.conf\t7
";
    assert_eq!(stdout, expected, "expected exact output match");
}

#[test]
fn step_boundary_excludes_events_past_step() {
    // AC: `--step N` includes events at step=N, excludes step=N+1.
    // With --step 2, only /etc/new.conf (created at step 2) appears.
    let dir = tempfile::tempdir().unwrap();
    let fixture = rich_fixture(dir.path());
    let r = run_rewind(&["rewind", fixture.to_str().unwrap(), "--step", "2", "--list"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert_eq!(stdout, "created\t/etc/new.conf\t2\n", "got: {stdout}");
}

#[test]
fn step_three_includes_modify() {
    // With --step 3, /etc/new.conf (created at 2) and /etc/old.conf
    // (modified at 3) both appear. /etc/readme.md (read at 4) is
    // excluded.
    let dir = tempfile::tempdir().unwrap();
    let fixture = rich_fixture(dir.path());
    let r = run_rewind(&["rewind", fixture.to_str().unwrap(), "--step", "3", "--list"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let expected = "\
created\t/etc/new.conf\t2
modified\t/etc/old.conf\t3
";
    assert_eq!(stdout, expected);
}

#[test]
fn out_of_range_step_exits_two() {
    // AC #3.
    let dir = tempfile::tempdir().unwrap();
    let fixture = rich_fixture(dir.path());
    let r = run_rewind(&[
        "rewind",
        fixture.to_str().unwrap(),
        "--step",
        "99999",
        "--list",
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("out of range"), "stderr: {stderr}");
    assert!(
        stderr.contains("99999"),
        "stderr should name the offending step: {stderr}"
    );
}

#[test]
fn missing_list_flag_exits_two_with_phase1_message() {
    // AC #4 — without `--list`, refuse to run (so we don't promise
    // file materialization that hasn't shipped).
    let dir = tempfile::tempdir().unwrap();
    let fixture = rich_fixture(dir.path());
    let r = run_rewind(&["rewind", fixture.to_str().unwrap(), "--step", "3"]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("Phase 1 only supports --list"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("#85"),
        "stderr should reference the parent issue: {stderr}"
    );
}

#[test]
fn cassette_with_only_task_and_eject_emits_empty_listing() {
    // No file events at all → empty listing, exit 0.
    let dir = tempfile::tempdir().unwrap();
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"no files\"}}
{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"outcome\":\"success\"}}
";
    let fixture = build_cassette(dir.path(), "no-files.tape", tracks);
    let r = run_rewind(&["rewind", fixture.to_str().unwrap(), "--step", "2", "--list"]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.is_empty(), "expected empty listing; got: {stdout}");
}
