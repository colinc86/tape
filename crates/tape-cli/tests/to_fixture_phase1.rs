//! End-to-end coverage for `tape to-fixture` Phase 1 (issue #102,
//! carved per #217). Hand-builds tapes via `PendingTape::write_to`
//! with at least one `model_call` and asserts:
//! - stdout-mode emits valid VCR YAML with the right interaction count
//! - `--output` mode writes the same content to disk
//! - `--format polly|httpretty|jsonl` exit 2 with Phase-1 message
//! - `--format <bogus>` exit 2 with format-list diagnostic
//! - missing input exits 2 (`open_input`'s posture).

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
                        id: \"01h8xy00-0000-7000-b8aa-000000000217\"\n\
                        created_at: \"2026-05-16T00:00:00Z\"\n\
                        ejected_at: \"2026-05-16T00:00:30Z\"\n\
                        task: \"to-fixture test\"\n\
                        recorder:\n  agent: \"test/0.0.1\"\n\
                        outcome: success\n";

fn build_cassette(dir: &Path, name: &str, tracks_jsonl: &str) -> std::path::PathBuf {
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

/// One task + one anthropic `model_call` + one eject. The `model_call`
/// projects to a single VCR interaction; task and eject are ignored.
fn cassette_with_one_anthropic_call(dir: &Path) -> std::path::PathBuf {
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"go\"}}
{\"step\":2,\"kind\":\"model_call\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"vendor\":\"anthropic\",\"model\":\"claude-opus-4-7\",\"request\":{\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]},\"response\":{\"content\":[{\"type\":\"text\",\"text\":\"hello\"}]},\"status_code\":200}}
{\"step\":3,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{\"outcome\":\"success\"}}
"
    .to_owned();
    build_cassette(dir, "one-call.tape", &tracks)
}

#[test]
fn stdout_emits_valid_vcr_yaml_with_one_interaction() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = cassette_with_one_anthropic_call(dir.path());
    let r = std::process::Command::new(binary_path())
        .args(["to-fixture", fixture.to_str().unwrap(), "--format", "vcr"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&stdout).expect("stdout is valid YAML");
    let interactions = doc
        .get("http_interactions")
        .and_then(|v| v.as_sequence())
        .expect("http_interactions");
    assert_eq!(interactions.len(), 1);
    assert_eq!(
        doc.get("recorded_with").and_then(|v| v.as_str()),
        Some("VCR 6.2.0")
    );
    // Sanity: request URI is anthropic's, method POST, response status 200.
    assert_eq!(
        interactions[0]["request"]["uri"].as_str(),
        Some("https://api.anthropic.com/v1/messages")
    );
    assert_eq!(interactions[0]["request"]["method"].as_str(), Some("POST"));
    assert_eq!(
        interactions[0]["response"]["status"]["code"]
            .as_u64()
            .unwrap_or(0),
        200
    );
}

#[test]
fn output_flag_writes_byte_identical_yaml_to_file() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = cassette_with_one_anthropic_call(dir.path());
    let out_path = dir.path().join("nested").join("cassette.yml");

    let stdout_r = std::process::Command::new(binary_path())
        .args(["to-fixture", fixture.to_str().unwrap(), "--format", "vcr"])
        .output()
        .unwrap();
    assert!(stdout_r.status.success());

    let file_r = std::process::Command::new(binary_path())
        .args([
            "to-fixture",
            fixture.to_str().unwrap(),
            "--format",
            "vcr",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(file_r.status.success(), "{file_r:?}");
    assert!(out_path.exists(), "output file should exist");

    let on_disk = std::fs::read(&out_path).unwrap();
    assert_eq!(
        on_disk, stdout_r.stdout,
        "--output mode bytes must match stdout-mode bytes"
    );
}

#[test]
fn unknown_vendor_emits_skip_comment_at_top_of_output() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"go\"}}
{\"step\":2,\"kind\":\"model_call\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"vendor\":\"google\",\"model\":\"gemini\",\"request\":{},\"response\":{},\"status_code\":200}}
{\"step\":3,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:02Z\",\"payload\":{\"outcome\":\"success\"}}
"
    .to_owned();
    let fixture = build_cassette(dir.path(), "google.tape", &tracks);
    let r = std::process::Command::new(binary_path())
        .args(["to-fixture", fixture.to_str().unwrap(), "--format", "vcr"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.starts_with("# tape to-fixture: skipped 1"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("google"), "stdout: {stdout}");
    // YAML body still parses (post-comment).
    let _: serde_yaml::Value =
        serde_yaml::from_str(&stdout).expect("YAML body still parses with leading comment");
}

#[test]
fn polly_format_exits_two_with_phase_one_message() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = cassette_with_one_anthropic_call(dir.path());
    for fmt in ["polly", "httpretty", "jsonl"] {
        let r = std::process::Command::new(binary_path())
            .args(["to-fixture", fixture.to_str().unwrap(), "--format", fmt])
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2), "format {fmt}: {r:?}");
        let stderr = String::from_utf8_lossy(&r.stderr);
        assert!(
            stderr.contains("not yet implemented in Phase 1"),
            "format {fmt} stderr: {stderr}"
        );
        assert!(
            stderr.contains("#102"),
            "format {fmt} stderr should reference #102: {stderr}"
        );
    }
}

#[test]
fn bogus_format_exits_two_with_format_list() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = cassette_with_one_anthropic_call(dir.path());
    let r = std::process::Command::new(binary_path())
        .args(["to-fixture", fixture.to_str().unwrap(), "--format", "csv"])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("unknown --format"), "stderr: {stderr}");
    assert!(stderr.contains("vcr"), "stderr should list vcr: {stderr}");
}

#[test]
fn missing_input_file_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    let nope = dir.path().join("nope.tape");
    let r = std::process::Command::new(binary_path())
        .args(["to-fixture", nope.to_str().unwrap(), "--format", "vcr"])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
}

#[test]
fn cassette_with_no_model_calls_emits_empty_interactions() {
    let dir = tempfile::tempdir().unwrap();
    // No model_call event — just task + eject.
    let tracks = "\
{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-16T00:00:00Z\",\"payload\":{\"prompt\":\"silent\"}}
{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-16T00:00:01Z\",\"payload\":{\"outcome\":\"success\"}}
"
    .to_owned();
    let fixture = build_cassette(dir.path(), "silent.tape", &tracks);
    let r = std::process::Command::new(binary_path())
        .args(["to-fixture", fixture.to_str().unwrap(), "--format", "vcr"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&stdout).unwrap();
    let interactions = doc
        .get("http_interactions")
        .and_then(|v| v.as_sequence())
        .expect("http_interactions sequence");
    assert!(interactions.is_empty(), "expected no interactions");
}
