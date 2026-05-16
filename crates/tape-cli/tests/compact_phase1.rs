//! End-to-end coverage for `tape compact` Phase 1 (issue #51, carved
//! per #215). Hand-builds tapes via `PendingTape::write_to` with an
//! oversize `shell.stdout` and asserts the output cassette is
//! smaller, re-verifies clean, and preserves every non-`tracks.jsonl`
//! zip entry byte-identical.

use std::collections::BTreeMap;
use std::path::Path;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

const STD_LINER: &str = "## What I was asked to do\nx\n\n\
                         ## What I found\ny\n\n\
                         ## Suggested next step / fix\nz\n\n\
                         ## What I'm uncertain about\nnothing\n";

const STD_META: &str = "tape_version: \"tape/v0\"\n\
                        id: \"01h8xy00-0000-7000-b8aa-000000000215\"\n\
                        created_at: \"2026-05-16T00:00:00Z\"\n\
                        ejected_at: \"2026-05-16T00:00:30Z\"\n\
                        task: \"compact test\"\n\
                        recorder:\n  agent: \"test/0.0.1\"\n\
                        outcome: success\n";

fn build_cassette(
    dir: &Path,
    name: &str,
    tracks_jsonl: &str,
    artifacts: BTreeMap<String, Vec<u8>>,
) -> std::path::PathBuf {
    let path = dir.join(name);
    let pending = tape_format::writer::PendingTape {
        meta_yaml: STD_META.to_owned(),
        liner_md: STD_LINER.to_owned(),
        tracks_jsonl: tracks_jsonl.to_owned(),
        redactions_json: None,
        artifacts,
    };
    pending.write_to(&path).unwrap();
    path
}

/// Build a tape with one 4 KiB `shell.stdout` so the default 1024-char
/// threshold triggers a truncation. Use less-compressible content so
/// the byte-size delta survives DEFLATE — a string of repeated 'x'
/// compresses to near-zero, masking the truncation in the zip layer.
fn oversize_shell_cassette(dir: &Path) -> std::path::PathBuf {
    // Pseudo-random alphanumeric content — high entropy → DEFLATE
    // can't compress it much, so the truncation shows up in output
    // bytes. Deterministic so the test stays repeatable.
    // 64 KiB of pseudo-random alphanumeric content — much larger
    // than the 1024-char default threshold so the post-truncation
    // bytes are dramatically smaller even after DEFLATE.
    let mut big = String::with_capacity(65_536);
    for i in 0..65_536 {
        let b = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
            [(i * 2654435761usize) % 62];
        big.push(b as char);
    }
    let tracks = format!(
        "\
{{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{{\"prompt\":\"compact me\"}}}}
{{\"step\":2,\"kind\":\"shell\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{{\"cmd\":\"ls -la\",\"stdout\":\"{big}\"}}}}
{{\"step\":3,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{{\"outcome\":\"success\"}}}}
"
    );
    build_cassette(dir, "oversize.tape", &tracks, BTreeMap::new())
}

/// Build a tape with no oversize fields — compact should be a no-op
/// on the truncation count.
fn small_cassette(dir: &Path) -> std::path::PathBuf {
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"already small\"}}
{\"step\":2,\"kind\":\"shell\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"cmd\":\"echo hi\",\"stdout\":\"hi\\n\"}}
{\"step\":3,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{\"outcome\":\"success\"}}
"
    .to_owned();
    build_cassette(dir, "small.tape", &tracks, BTreeMap::new())
}

fn read_back(path: &Path) -> tape_format::reader::RawTape {
    tape_format::reader::RawTape::open(path).unwrap()
}

#[test]
fn happy_path_default_output_writes_smaller_cassette_and_verifies_clean() {
    let dir = tempfile::tempdir().unwrap();
    let input = oversize_shell_cassette(dir.path());
    let expected_out = dir.path().join("oversize.compact.tape");

    let r = std::process::Command::new(binary_path())
        .args(["compact", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "tape compact failed: {r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("tape compact: wrote"), "stderr: {stderr}");
    assert!(
        stderr.contains("string leaves truncated"),
        "stderr: {stderr}"
    );

    assert!(expected_out.exists());

    // (b) output strictly smaller when payload exceeded threshold.
    let in_size = std::fs::metadata(&input).unwrap().len();
    let out_size = std::fs::metadata(&expected_out).unwrap().len();
    assert!(
        out_size < in_size,
        "output ({out_size} bytes) should be smaller than input ({in_size} bytes)"
    );

    // (c) post-write `tape verify` clean.
    let verify = std::process::Command::new(binary_path())
        .args(["verify", expected_out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "tape verify on compact output failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );

    // (d) meta.yaml + liner-notes.md pass through byte-identical.
    let inp = read_back(&input);
    let outp = read_back(&expected_out);
    assert_eq!(inp.meta_yaml, outp.meta_yaml, "meta.yaml mutated");
    assert_eq!(inp.liner_md, outp.liner_md, "liner-notes.md mutated");

    // The truncated track contains the marker.
    let tracks = outp.tracks_jsonl.unwrap();
    assert!(
        tracks.contains("[truncated, 65536 chars]"),
        "truncation marker missing from output tracks (first 200 bytes: {})",
        &tracks[..tracks.len().min(200)]
    );
}

#[test]
fn no_oversize_payloads_passes_through_parsed_equivalent() {
    // (b) bytes IDENTICAL when none exceeded threshold — caveat:
    // re-serialization via `Track::to_line` may reorder JSON object
    // keys, so we assert PARSED equivalence rather than byte equality
    // (mirrors the relinernote / anon precedents from prior PRs).
    let dir = tempfile::tempdir().unwrap();
    let input = small_cassette(dir.path());
    let r = std::process::Command::new(binary_path())
        .args(["compact", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "tape compact failed: {r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("0 string leaves truncated"),
        "stderr: {stderr}"
    );

    let inp = read_back(&input);
    let outp = read_back(&dir.path().join("small.compact.tape"));
    assert_eq!(inp.meta_yaml, outp.meta_yaml);
    let inp_tracks = inp.tracks_jsonl.unwrap();
    let out_tracks = outp.tracks_jsonl.unwrap();
    // Parse both sides and compare structurally.
    for (lhs, rhs) in inp_tracks.lines().zip(out_tracks.lines()) {
        let l: serde_json::Value = serde_json::from_str(lhs).unwrap();
        let r: serde_json::Value = serde_json::from_str(rhs).unwrap();
        assert_eq!(l, r, "track parsed-equivalence");
    }
}

#[test]
fn output_equals_input_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    let input = small_cassette(dir.path());
    let r = std::process::Command::new(binary_path())
        .args([
            "compact",
            input.to_str().unwrap(),
            "--output",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("--output must differ"), "stderr: {stderr}");
}

#[test]
fn max_output_chars_zero_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    let input = small_cassette(dir.path());
    let r = std::process::Command::new(binary_path())
        .args([
            "compact",
            input.to_str().unwrap(),
            "--max-output-chars",
            "0",
        ])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("must be ≥ 1"), "stderr: {stderr}");
}

#[test]
fn explicit_output_path_honored() {
    let dir = tempfile::tempdir().unwrap();
    let input = oversize_shell_cassette(dir.path());
    let custom_out = dir.path().join("custom").join("out.tape");
    let r = std::process::Command::new(binary_path())
        .args([
            "compact",
            input.to_str().unwrap(),
            "--output",
            custom_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    assert!(custom_out.exists(), "custom output path should exist");
    // Default path should NOT exist (we used --output).
    assert!(!dir.path().join("oversize.compact.tape").exists());
}

#[test]
fn smaller_threshold_truncates_more_aggressively() {
    // The same fixture compacted with --max-output-chars 16 should
    // produce a smaller output than with the default 1024.
    let dir = tempfile::tempdir().unwrap();
    let input = oversize_shell_cassette(dir.path());
    let out_default = dir.path().join("d.tape");
    let out_tight = dir.path().join("t.tape");
    for (out, args) in [
        (
            &out_default,
            vec!["--output", out_default.to_str().unwrap()],
        ),
        (
            &out_tight,
            vec![
                "--output",
                out_tight.to_str().unwrap(),
                "--max-output-chars",
                "16",
            ],
        ),
    ] {
        let mut cmd = std::process::Command::new(binary_path());
        cmd.args(["compact", input.to_str().unwrap()]);
        cmd.args(&args);
        let r = cmd.output().unwrap();
        assert!(r.status.success(), "{r:?} for out={out:?}");
    }
    let d_size = std::fs::metadata(&out_default).unwrap().len();
    let t_size = std::fs::metadata(&out_tight).unwrap().len();
    assert!(
        t_size < d_size,
        "tighter threshold should produce smaller output (default {d_size} vs tight {t_size})"
    );
}

#[test]
fn artifacts_pass_through_byte_identical() {
    // Build a tape with an artifact under `artifacts/`; assert that
    // after compact, the same artifact bytes survive byte-identical
    // even though the tracks were rewritten.
    let dir = tempfile::tempdir().unwrap();
    let big = "x".repeat(4096);
    let tracks = format!(
        "\
{{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{{\"prompt\":\"with artifact\"}}}}
{{\"step\":2,\"kind\":\"shell\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{{\"cmd\":\"x\",\"stdout\":\"{big}\"}}}}
{{\"step\":3,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{{\"outcome\":\"success\"}}}}
"
    );
    let artifact_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02];
    let mut artifacts = BTreeMap::new();
    artifacts.insert("artifacts/test.bin".to_owned(), artifact_bytes.clone());
    let input = build_cassette(dir.path(), "with-artifact.tape", &tracks, artifacts);

    let r = std::process::Command::new(binary_path())
        .args(["compact", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");

    let out = read_back(&dir.path().join("with-artifact.compact.tape"));
    let preserved = out
        .artifacts
        .get("artifacts/test.bin")
        .expect("artifact key preserved");
    assert_eq!(
        preserved, &artifact_bytes,
        "artifact bytes must be byte-identical after compact"
    );
}
