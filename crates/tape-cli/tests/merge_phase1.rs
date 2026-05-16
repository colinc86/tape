//! End-to-end coverage for `tape merge` Phase 1 (issue #61, carved
//! per #219). Hand-builds two cassettes via `PendingTape::write_to`
//! and asserts the merged output:
//! - passes `tape verify`
//! - has the right step count (len(a) + len(b) - 2)
//! - preserves cassette1's meta + liner verbatim
//! - artifacts union'd
//! - `--output == input` refused
//! - verify-invalid input aborts pre-write
//! - stdout mode emits a valid zip

use std::collections::BTreeMap;
use std::path::Path;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

const STD_LINER: &str = "## What I was asked to do\nx\n\n\
                         ## What I found\ny\n\n\
                         ## Suggested next step / fix\nz\n\n\
                         ## What I'm uncertain about\nnothing\n";

fn meta_yaml(label: &str, id_suffix: &str) -> String {
    format!(
        "tape_version: \"tape/v0\"\n\
         id: \"01h8xy00-0000-7000-b8aa-{id_suffix:0>12}\"\n\
         created_at: \"2026-05-16T00:00:00Z\"\n\
         ejected_at: \"2026-05-16T00:00:30Z\"\n\
         task: \"merge test {label}\"\n\
         recorder:\n  agent: \"test/0.0.1\"\n\
         outcome: success\n"
    )
}

fn build_cassette(
    dir: &Path,
    name: &str,
    tracks_jsonl: &str,
    label: &str,
    id_suffix: &str,
    artifacts: BTreeMap<String, Vec<u8>>,
) -> std::path::PathBuf {
    let path = dir.join(name);
    let pending = tape_format::writer::PendingTape {
        meta_yaml: meta_yaml(label, id_suffix),
        liner_md: STD_LINER.to_owned(),
        tracks_jsonl: tracks_jsonl.to_owned(),
        redactions_json: None,
        artifacts,
    };
    pending.write_to(&path).unwrap();
    path
}

/// A 5-track cassette: task, 2 middle tracks, eject. The middle
/// shell event has no parent_step.
fn cassette_a(dir: &Path) -> std::path::PathBuf {
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"first cassette\"}}
{\"step\":2,\"kind\":\"shell\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"cmd\":\"ls\"}}
{\"step\":3,\"kind\":\"file_read\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{\"path\":\"/etc/hosts\",\"content_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000001\"}}
{\"step\":4,\"kind\":\"annotation\",\"ts\":\"2026-05-16T00:00:03Z\",\"payload\":{\"by\":\"agent\",\"note\":\"first\"}}
{\"step\":5,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:04Z\",\"payload\":{\"outcome\":\"success\"}}
"
    .to_owned();
    build_cassette(dir, "a.tape", &tracks, "A", "1", BTreeMap::new())
}

/// A 5-track cassette where step 4 (annotation) has parent_step=2,
/// so we can verify the parent_step rewrite path.
fn cassette_b(dir: &Path) -> std::path::PathBuf {
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:05Z\",\"payload\":{\"prompt\":\"second cassette\"}}
{\"step\":2,\"kind\":\"shell\",\"ts\":\"2026-05-16T00:00:06Z\",\"payload\":{\"cmd\":\"pwd\"}}
{\"step\":3,\"kind\":\"file_read\",\"ts\":\"2026-05-16T00:00:07Z\",\"payload\":{\"path\":\"/tmp/x\",\"content_hash\":\"blake3:0000000000000000000000000000000000000000000000000000000000000002\"}}
{\"step\":4,\"kind\":\"annotation\",\"ts\":\"2026-05-16T00:00:08Z\",\"payload\":{\"by\":\"agent\",\"note\":\"second\"},\"parent_step\":2}
{\"step\":5,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:09Z\",\"payload\":{\"outcome\":\"success\"}}
"
    .to_owned();
    build_cassette(dir, "b.tape", &tracks, "B", "2", BTreeMap::new())
}

#[test]
fn happy_path_writes_verify_clean_with_8_contiguous_steps() {
    let dir = tempfile::tempdir().unwrap();
    let a = cassette_a(dir.path());
    let b = cassette_b(dir.path());
    let out = dir.path().join("merged.tape");

    let r = std::process::Command::new(binary_path())
        .args([
            "merge",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(r.status.success(), "tape merge failed: {r:?}");
    assert!(out.exists());

    // (a) Output passes `tape verify`.
    let verify = std::process::Command::new(binary_path())
        .args(["verify", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "tape verify on merged output failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );

    // (b) Step count = 5 + 5 - 2 = 8, contiguous 1..8.
    let raw = tape_format::reader::RawTape::open(&out).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(&raw.tracks_jsonl.clone().unwrap()).unwrap();
    assert_eq!(tracks.len(), 8);
    for (i, t) in tracks.iter().enumerate() {
        assert_eq!(t.step, (i as u64) + 1, "step {i} should be {}", i + 1);
    }
    assert_eq!(
        tracks.first().unwrap().kind,
        tape_format::tracks::Kind::Task
    );
    assert_eq!(
        tracks.last().unwrap().kind,
        tape_format::tracks::Kind::Eject
    );
}

#[test]
fn parent_step_on_cassette2_is_offset_rewritten_correctly() {
    let dir = tempfile::tempdir().unwrap();
    let a = cassette_a(dir.path());
    let b = cassette_b(dir.path());
    let out = dir.path().join("merged.tape");

    let r = std::process::Command::new(binary_path())
        .args([
            "merge",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");

    let raw = tape_format::reader::RawTape::open(&out).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(&raw.tracks_jsonl.clone().unwrap()).unwrap();
    // Cassette2's annotation (originally step 4, parent_step=2) becomes
    // merged step 7 (4 surviving cassette1 tracks: task,shell,file_read,
    // annotation; then cassette2's 3 surviving tracks before annotation:
    // shell,file_read; annotation is the 7th merged track). parent_step=2
    // (cassette2's shell) → merged step 5 (4 cassette1 surviving + 1
    // = shell from cassette2).
    let annot_b = tracks
        .iter()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation && t.step == 7)
        .expect("cassette2's annotation at merged step 7");
    assert_eq!(
        annot_b.parent_step,
        Some(5),
        "cassette2's annotation parent_step (orig 2) → merged step 5"
    );
}

#[test]
fn meta_and_liner_come_from_cassette1() {
    let dir = tempfile::tempdir().unwrap();
    let a = cassette_a(dir.path());
    let b = cassette_b(dir.path());
    let out = dir.path().join("merged.tape");

    let r = std::process::Command::new(binary_path())
        .args([
            "merge",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(r.status.success());

    let inp_a = tape_format::reader::RawTape::open(&a).unwrap();
    let merged = tape_format::reader::RawTape::open(&out).unwrap();
    assert_eq!(merged.meta_yaml, inp_a.meta_yaml, "meta.yaml = cassette1");
    assert_eq!(merged.liner_md, inp_a.liner_md, "liner = cassette1");
    // Sanity: cassette2's task label is absent in the merged meta.
    assert!(
        !merged
            .meta_yaml
            .as_deref()
            .unwrap_or("")
            .contains("merge test B"),
        "cassette2's task label should not appear in merged meta"
    );
}

#[test]
fn artifacts_union_with_dedup() {
    let dir = tempfile::tempdir().unwrap();
    // Build cassette A with one artifact (cassette1's bytes win).
    let mut a_artifacts = BTreeMap::new();
    a_artifacts.insert("artifacts/aa/bb/shared.bin".to_owned(), vec![1, 1, 1]);
    a_artifacts.insert("artifacts/cc/aa.bin".to_owned(), vec![0xAA]);
    let a_tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"a\"}}
{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"outcome\":\"success\"}}
"
    .to_owned();
    let a = build_cassette(dir.path(), "a.tape", &a_tracks, "A", "1", a_artifacts);
    // Cassette B with the same shared key + a unique one.
    let mut b_artifacts = BTreeMap::new();
    b_artifacts.insert("artifacts/aa/bb/shared.bin".to_owned(), vec![2, 2, 2]);
    b_artifacts.insert("artifacts/dd/bb.bin".to_owned(), vec![0xBB]);
    let b_tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{\"prompt\":\"b\"}}
{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:03Z\",\"payload\":{\"outcome\":\"success\"}}
"
    .to_owned();
    let b = build_cassette(dir.path(), "b.tape", &b_tracks, "B", "2", b_artifacts);
    let out = dir.path().join("merged.tape");
    let r = std::process::Command::new(binary_path())
        .args([
            "merge",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");

    let merged = tape_format::reader::RawTape::open(&out).unwrap();
    assert_eq!(merged.artifacts.len(), 3, "3 distinct artifact paths");
    assert_eq!(
        merged.artifacts.get("artifacts/aa/bb/shared.bin"),
        Some(&vec![1, 1, 1]),
        "cassette1's bytes win on shared path"
    );
    assert_eq!(
        merged.artifacts.get("artifacts/cc/aa.bin"),
        Some(&vec![0xAA])
    );
    assert_eq!(
        merged.artifacts.get("artifacts/dd/bb.bin"),
        Some(&vec![0xBB])
    );
}

#[test]
fn output_equals_input_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    let a = cassette_a(dir.path());
    let b = cassette_b(dir.path());

    // --output == a
    let r1 = std::process::Command::new(binary_path())
        .args([
            "merge",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--output",
            a.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(r1.status.code(), Some(2), "{r1:?}");
    let stderr1 = String::from_utf8_lossy(&r1.stderr);
    assert!(
        stderr1.contains("--output must differ"),
        "stderr: {stderr1}"
    );

    // --output == b
    let r2 = std::process::Command::new(binary_path())
        .args([
            "merge",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--output",
            b.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(r2.status.code(), Some(2), "{r2:?}");
}

#[test]
fn verify_invalid_input_aborts_pre_write() {
    let dir = tempfile::tempdir().unwrap();
    // Hand-build a malformed cassette: missing the eject event.
    let bad_tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"oops\"}}
{\"step\":2,\"kind\":\"shell\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"cmd\":\"x\"}}
"
    .to_owned();
    let bad = build_cassette(
        dir.path(),
        "bad.tape",
        &bad_tracks,
        "BAD",
        "9",
        BTreeMap::new(),
    );
    let good = cassette_b(dir.path());
    let out = dir.path().join("merged.tape");

    let r = std::process::Command::new(binary_path())
        .args([
            "merge",
            bad.to_str().unwrap(),
            good.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    assert!(!out.exists(), "no output file on verify-invalid input");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("failed tape verify"), "stderr: {stderr}");
}

#[test]
fn stdout_mode_emits_valid_zip() {
    let dir = tempfile::tempdir().unwrap();
    let a = cassette_a(dir.path());
    let b = cassette_b(dir.path());
    let r = std::process::Command::new(binary_path())
        .args(["merge", a.to_str().unwrap(), b.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    assert!(!r.stdout.is_empty(), "stdout should be a non-empty zip");

    // Round-trip the stdout bytes through RawTape::from_reader.
    let cursor = std::io::Cursor::new(r.stdout);
    let raw = tape_format::reader::RawTape::from_reader(cursor).expect("stdout parses as zip");
    let tracks = tape_format::tracks::parse_jsonl(&raw.tracks_jsonl.clone().unwrap()).unwrap();
    assert_eq!(tracks.len(), 8);
    // Verify passes on the stdout-mode output too.
    let verify = tape_format::verify::verify(&raw);
    assert!(
        verify.is_valid(),
        "stdout-mode output should verify clean: {:?}",
        verify.errors().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );
}
