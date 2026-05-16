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
    let resp = run_snapshot(
        &fixture_path("minimal.jsonl"),
        &out_path,
        Some("testing snapshot"),
    );

    assert!(
        !resp["result"]["isError"].as_bool().unwrap_or(false),
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
    assert!(
        !resp["result"]["isError"].as_bool().unwrap_or(false),
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

/// Regression test for issue #5: every per-event `ts` in the snapshot output
/// must reflect the *transcript's* timestamp for that event, not the snapshot
/// moment. The minimal fixture has two distinct timestamps; before the fix
/// every track collapsed onto `Utc::now()` at snapshot time.
#[test]
fn snapshot_preserves_per_event_timestamps_from_transcript() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("ts.tape");
    let resp = run_snapshot(&fixture_path("minimal.jsonl"), &out_path, Some("ts test"));
    assert!(
        !resp["result"]["isError"].as_bool().unwrap_or(false),
        "snapshot returned error: {resp}"
    );

    let raw = tape_format::reader::RawTape::open(&out_path).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(&raw.tracks_jsonl.unwrap()).unwrap();
    let task = tracks
        .iter()
        .find(|t| t.kind == tape_format::tracks::Kind::Task)
        .unwrap();
    let model = tracks
        .iter()
        .find(|t| t.kind == tape_format::tracks::Kind::ModelCall)
        .unwrap();

    // The minimal fixture has the user prompt at 10:00:00.000 and the
    // assistant turn at 10:00:01.500. Each should round-trip into its event.
    assert!(
        task.ts.starts_with("2026-05-06T10:00:00"),
        "task ts {} did not preserve transcript user prompt time",
        task.ts
    );
    assert!(
        model.ts.starts_with("2026-05-06T10:00:01"),
        "model_call ts {} did not preserve transcript assistant time",
        model.ts
    );
    assert_ne!(
        task.ts, model.ts,
        "events collapsed onto a single ts — see issue #5"
    );
}

#[test]
fn snapshot_with_unknown_type_surfaces_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("warn.tape");
    let resp = run_snapshot(&fixture_path("unknown_type.jsonl"), &out_path, None);
    assert!(
        !resp["result"]["isError"].as_bool().unwrap_or(false),
        "snapshot returned error: {resp}"
    );

    let warnings = &resp["result"]["structuredContent"]["parse_warnings"];
    assert_eq!(
        warnings["unknown_event_types"]["future-thing"], 1,
        "expected unknown type to surface in parse_warnings: {warnings}"
    );
}
