//! End-to-end test for `tape.snapshot`. Pass `transcript_path` arg directly
//! to point at a checked-in fixture transcript, call the tool via JSON-RPC,
//! and assert the produced `.tape` passes verify and contains expected events.

use serde_json::{json, Value};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tape-record")
        .join("tests")
        .join("fixtures")
        .join("transcripts")
        .join(name)
}

fn run_snapshot(fixture: &std::path::Path, out: &std::path::Path, task: Option<&str>) -> Value {
    let mut args = json!({
        "out": out.to_str().unwrap(),
        "transcript_path": fixture.to_str().unwrap(),
    });
    if let Some(t) = task {
        args["task"] = Value::String(t.to_string());
    }
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "tape.snapshot", "arguments": args}
    });
    let mut output = Vec::<u8>::new();
    tape_mcp::server::run(
        format!("{}\n", request).as_bytes(),
        &mut output,
        tape_mcp::Deck::new(),
    )
    .unwrap();
    serde_json::from_str(String::from_utf8(output).unwrap().lines().next().unwrap()).unwrap()
}

#[test]
fn snapshot_minimal_fixture_produces_valid_tape() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("snap.tape");
    let resp = run_snapshot(&fixture_path("minimal.jsonl"), &out_path, Some("testing snapshot"));

    assert_eq!(
        resp["result"]["isError"].as_bool().unwrap_or(false),
        false,
        "snapshot returned error: {resp}"
    );
    let track_count = resp["result"]["structuredContent"]["track_count"]
        .as_u64()
        .unwrap();
    assert!(
        track_count >= 2,
        "expected ≥2 tracks (task + model_call), got {track_count}"
    );

    assert!(out_path.exists());
    let raw = tape_format::reader::RawTape::open(&out_path).unwrap();
    let report = tape_format::verify::verify(&raw);
    assert!(
        report.is_valid(),
        "snapshot tape failed verify: {:?}",
        report.errors().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn snapshot_redaction_fixture_strips_aws_key() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("redacted.tape");
    let resp = run_snapshot(&fixture_path("redaction_bait.jsonl"), &out_path, None);
    assert_eq!(
        resp["result"]["isError"].as_bool().unwrap_or(false),
        false,
        "snapshot returned error: {resp}"
    );

    let raw = tape_format::reader::RawTape::open(&out_path).unwrap();
    let tracks = raw.tracks_jsonl.unwrap();
    assert!(
        !tracks.contains("AKIA1234567890ABCDEF"),
        "raw AWS key leaked into tracks"
    );
    assert!(
        tracks.contains("<API_KEY:aws_access>"),
        "aws_access redaction replacement missing\n{tracks}"
    );
    assert!(
        tracks.contains("<EMAIL>"),
        "email redaction replacement missing\n{tracks}"
    );
}

#[test]
fn snapshot_with_unknown_type_surfaces_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("warn.tape");
    let resp = run_snapshot(&fixture_path("unknown_type.jsonl"), &out_path, None);
    assert_eq!(
        resp["result"]["isError"].as_bool().unwrap_or(false),
        false,
        "snapshot returned error: {resp}"
    );

    let warnings = &resp["result"]["structuredContent"]["parse_warnings"];
    assert_eq!(
        warnings["unknown_event_types"]["future-thing"], 1,
        "expected unknown type to surface in parse_warnings: {warnings}"
    );
}
