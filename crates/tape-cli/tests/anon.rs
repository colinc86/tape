//! `tape anon` end-to-end CLI integration coverage. Per issue #204
//! "tape-cli/tests/anon.rs — one end-to-end shell-out test" but
//! expanded slightly to cover the three exit-2 refusal paths
//! (`-o in.tape`, `-o existing.tape`, missing input → exit 3).

use std::collections::BTreeMap;
use std::path::Path;

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

/// Build a cassette in `dir` that contains 3 `/Users/colin` occurrences
/// (one in meta.task, two in track payloads) so the anon happy-path
/// has visible substitution. Returns the file path.
fn build_home_path_fixture(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("input.tape");
    let meta = "tape_version: \"tape/v0\"\n\
                id: \"01h8xy00-0000-7000-b8aa-000000000204\"\n\
                created_at: \"2026-05-16T00:00:00Z\"\n\
                ejected_at: \"2026-05-16T00:00:30Z\"\n\
                task: \"investigate /Users/colin/work/billing\"\n\
                recorder:\n  agent: \"test/0.0.1\"\n\
                outcome: success\n"
        .to_owned();
    let liner = "## What I was asked to do\nx\n\n\
                 ## What I found\ny\n\n\
                 ## Suggested next step / fix\nz\n\n\
                 ## What I'm uncertain about\nnothing\n"
        .to_owned();
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"investigate\"}}
{\"step\":2,\"kind\":\"shell\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"cmd\":\"cat /Users/colin/.bashrc\"}}
{\"step\":3,\"kind\":\"shell\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{\"cmd\":\"ls /Users/colin/work\"}}
{\"step\":4,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:03Z\",\"payload\":{\"outcome\":\"success\"}}
"
    .to_owned();
    let pending = tape_format::writer::PendingTape {
        meta_yaml: meta,
        liner_md: liner,
        tracks_jsonl: tracks,
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(&path).unwrap();
    path
}

fn read_meta_and_tracks(path: &Path) -> (String, String) {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    (
        raw.meta_yaml.unwrap_or_default(),
        raw.tracks_jsonl.unwrap_or_default(),
    )
}

#[test]
fn happy_path_default_out_writes_anonymized_cassette() {
    // AC #1 + ticket required-behavior items 1, 5, 6: default `-o` is
    // `<basename>.anon.tape`; the output passes `tape verify` clean;
    // every home path occurrence is replaced.
    let dir = tempfile::tempdir().unwrap();
    let input = build_home_path_fixture(dir.path());
    let expected_out = dir.path().join("input.anon.tape");

    let r = std::process::Command::new(binary_path())
        .args(["anon", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "tape anon failed: stdout={} stderr={}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr),
    );

    // Stderr summary present per ticket AC #7.
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("tape anon: wrote"), "stderr: {stderr}");
    assert!(
        stderr.contains("replacements via unix_home_path"),
        "stderr: {stderr}"
    );

    assert!(expected_out.exists(), "expected default output path");

    // Output has no /Users/colin anywhere.
    let (out_meta, out_tracks) = read_meta_and_tracks(&expected_out);
    assert!(
        !out_meta.contains("/Users/colin"),
        "leak in meta: {out_meta}"
    );
    assert!(
        !out_tracks.contains("/Users/colin"),
        "leak in tracks: {out_tracks}"
    );
    assert!(out_tracks.matches("<PATH:home:").count() == 2);
    assert!(out_meta.contains("<PATH:home:"));

    // tape verify on the output passes (AC #5).
    let verify = std::process::Command::new(binary_path())
        .args(["verify", expected_out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "tape verify on anon output failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn out_equals_input_exits_two() {
    // Required-behavior item 3.
    let dir = tempfile::tempdir().unwrap();
    let input = build_home_path_fixture(dir.path());

    let r = std::process::Command::new(binary_path())
        .args([
            "anon",
            input.to_str().unwrap(),
            "-o",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("--out must differ from input path"),
        "stderr: {stderr}"
    );
}

#[test]
fn out_path_already_exists_exits_two() {
    // Required-behavior item 4.
    let dir = tempfile::tempdir().unwrap();
    let input = build_home_path_fixture(dir.path());
    let pre_existing = dir.path().join("collision.tape");
    std::fs::write(&pre_existing, b"not a real tape but exists").unwrap();

    let r = std::process::Command::new(binary_path())
        .args([
            "anon",
            input.to_str().unwrap(),
            "-o",
            pre_existing.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("already exists"), "stderr: {stderr}");

    // The pre-existing file is unchanged.
    let preserved = std::fs::read(&pre_existing).unwrap();
    assert_eq!(preserved, b"not a real tape but exists");
}

#[test]
fn nonexistent_input_exits_three() {
    // Required-behavior item 9 — open failure exits 3 (mirrors verify).
    let dir = tempfile::tempdir().unwrap();
    let nope = dir.path().join("nope.tape");
    let r = std::process::Command::new(binary_path())
        .args(["anon", nope.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(3), "{r:?}");
}

#[test]
fn input_is_unchanged_byte_for_byte_after_anon() {
    // Required-behavior item 1 second clause — input bytes preserved.
    let dir = tempfile::tempdir().unwrap();
    let input = build_home_path_fixture(dir.path());
    let before = std::fs::read(&input).unwrap();

    let r = std::process::Command::new(binary_path())
        .args(["anon", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");

    let after = std::fs::read(&input).unwrap();
    assert_eq!(before, after, "input cassette was mutated by tape anon");
}

#[test]
fn fixture_with_no_home_paths_anon_exits_clean_with_zero_replacements() {
    // The bundled minimal-success fixture has no home paths. anon
    // should succeed with 0 replacements.
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &copy).unwrap();
    let out = dir.path().join("out.tape");

    let r = std::process::Command::new(binary_path())
        .args(["anon", copy.to_str().unwrap(), "-o", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("0 replacements"), "stderr: {stderr}");
    assert!(out.exists());
}
