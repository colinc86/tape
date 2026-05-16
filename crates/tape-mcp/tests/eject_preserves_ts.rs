//! Regression tests for issue #20: `tape.eject` must preserve per-event `ts`
//! values on the loaded tracks (and `meta.created_at`) when round-tripping
//! a loaded tape through the eject pipeline, rather than clobbering every
//! timestamp with "now". This mirrors the issue #5 / PR #16 fix on
//! `tape.snapshot` but for the `tool_eject` call site.

use serde_json::{json, Value};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Drive a sequence of JSON-RPC requests through the deck and return parsed
/// response lines. Reuses a single Deck so handles stay valid across calls.
fn pump(deck: tape_mcp::Deck, requests: &[Value]) -> Vec<Value> {
    let mut input = String::new();
    for r in requests {
        input.push_str(&r.to_string());
        input.push('\n');
    }
    let mut output = Vec::<u8>::new();
    tape_mcp::server::run(input.as_bytes(), &mut output, deck).unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// Load + eject the killer-scenario fixture and return (`ejected_meta`, `ejected_tracks`).
fn load_and_eject(
    fixture: &std::path::Path,
    out: &std::path::Path,
) -> (tape_format::meta::Meta, Vec<tape_format::tracks::Track>) {
    let deck = tape_mcp::Deck::new();
    let load = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.load", "arguments": {"path": fixture.to_str().unwrap()}}
    });
    let load_resp = pump(deck.clone(), &[load]);
    let handle = load_resp[0]["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();

    let eject = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "tape.eject", "arguments": {
            "handle": handle, "out": out.to_str().unwrap()
        }}
    });
    let eject_resp = pump(deck, &[eject]);
    assert!(
        !eject_resp[0]["result"]["isError"]
            .as_bool()
            .unwrap_or(false),
        "eject should succeed; got {:?}",
        eject_resp[0]
    );
    assert!(out.exists(), "tape file written");

    let raw = tape_format::reader::RawTape::open(out).unwrap();
    let meta = tape_format::meta::Meta::parse(raw.meta_yaml.as_deref().unwrap()).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(&raw.tracks_jsonl.unwrap()).unwrap();
    (meta, tracks)
}

/// Issue #20 (1): every per-event `ts` on the source tape must survive a
/// load → eject round-trip. The killer-scenario fixture spreads events
/// across 10:00:00 → 10:00:50 — before the fix, every event collapsed
/// onto `Utc::now()` at eject time.
#[test]
fn eject_preserves_per_event_timestamps_from_loaded_tape() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("round-trip.tape");
    let (_meta, tracks) = load_and_eject(&fixture_path("killer-scenario-a.tape"), &out_path);

    // The source fixture has these distinct timestamps at specific kinds.
    let task = tracks
        .iter()
        .find(|t| t.kind == tape_format::tracks::Kind::Task)
        .expect("task event");
    assert!(
        task.ts.starts_with("2026-05-06T10:00:00"),
        "task ts {} did not survive eject",
        task.ts
    );

    let model_calls: Vec<&tape_format::tracks::Track> = tracks
        .iter()
        .filter(|t| t.kind == tape_format::tracks::Kind::ModelCall)
        .collect();
    assert_eq!(model_calls.len(), 2, "fixture has two model_call events");
    assert!(
        model_calls[0].ts.starts_with("2026-05-06T10:00:15"),
        "first model_call ts {} did not survive eject",
        model_calls[0].ts
    );
    assert!(
        model_calls[1].ts.starts_with("2026-05-06T10:00:50"),
        "second model_call ts {} did not survive eject",
        model_calls[1].ts
    );

    let mcp = tracks
        .iter()
        .find(|t| t.kind == tape_format::tracks::Kind::McpCall)
        .expect("mcp_call event");
    assert!(
        mcp.ts.starts_with("2026-05-06T10:00:25"),
        "mcp_call ts {} did not survive eject",
        mcp.ts
    );

    let annot = tracks
        .iter()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation)
        .expect("annotation event");
    assert!(
        annot.ts.starts_with("2026-05-06T10:00:40"),
        "annotation ts {} did not survive eject",
        annot.ts
    );

    // Sanity: events did NOT collapse onto a single ts.
    let unique_ts: std::collections::BTreeSet<&str> =
        tracks.iter().map(|t| t.ts.as_str()).collect();
    assert!(
        unique_ts.len() >= 4,
        "expected ≥4 distinct timestamps after eject; got {:?}",
        unique_ts
    );
}

/// Issue #20 (4): `meta.created_at` on the ejected tape must reflect the
/// *source* tape's `created_at`, not the moment eject ran.
#[test]
fn eject_preserves_meta_created_at_from_loaded_tape() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("created-at.tape");
    let (meta, _tracks) = load_and_eject(&fixture_path("killer-scenario-a.tape"), &out_path);

    assert!(
        meta.created_at.starts_with("2026-05-06T10:00:00"),
        "meta.created_at {} did not preserve source tape's created_at",
        meta.created_at
    );
}

/// Issue #20 (2): a record → annotate → eject flow with a real time gap
/// between annotate and eject must produce an annotation event whose `ts`
/// matches the annotate time, not the eject time.
///
/// Note: the bug repro for `tool_eject` specifically targets the *replay*
/// path that copies `loaded.tracks` into a fresh `Session`. For tracks that
/// were appended in-process during `tape.record` / `tape.annotate`, they
/// already carry a `ts` (set when the event was appended), and this test
/// verifies that ts survives the replay.
#[test]
fn record_annotate_eject_preserves_annotation_ts() {
    let deck = tape_mcp::Deck::new();
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("annot.tape");

    // Phase 1: open a recording.
    let record_resp = pump(
        deck.clone(),
        &[json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "tape.record", "arguments": {"task": "ts preservation"}}
        })],
    );
    let handle = record_resp[0]["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();

    // Phase 2: annotate.
    let annotate_at = chrono::Utc::now();
    let _ = pump(
        deck.clone(),
        &[json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "tape.annotate", "arguments": {"handle": handle, "note": "pre-eject note"}}
        })],
    );

    // Phase 3: wait long enough that "now" at eject time is clearly distinct
    // from when the annotation was appended. 1500ms is plenty for second-
    // precision rfc3339 comparison and is forgiving on slow CI.
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // Phase 4: eject.
    let eject_resp = pump(
        deck,
        &[json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "tape.eject", "arguments": {
                "handle": handle, "out": out_path.to_str().unwrap()
            }}
        })],
    );
    assert!(
        !eject_resp[0]["result"]["isError"]
            .as_bool()
            .unwrap_or(false),
        "eject should succeed; got {:?}",
        eject_resp[0]
    );

    let raw = tape_format::reader::RawTape::open(&out_path).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(&raw.tracks_jsonl.unwrap()).unwrap();
    let annot = tracks
        .iter()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation)
        .expect("annotation event present");
    let annot_ts =
        chrono::DateTime::parse_from_rfc3339(&annot.ts).expect("annotation ts is rfc3339");

    // The annotation ts must be close to `annotate_at`, not to "now". Allow
    // a generous 1s window on either side of annotate_at for clock skew; the
    // 1.5s sleep means a clobbered (eject-time) ts would be far outside.
    let diff_from_annotate = (annot_ts.with_timezone(&chrono::Utc) - annotate_at)
        .num_milliseconds()
        .abs();
    assert!(
        diff_from_annotate < 1000,
        "annotation ts {} drifted >1s from annotate-time {} — likely clobbered with eject-time",
        annot.ts,
        annotate_at.to_rfc3339()
    );
}
