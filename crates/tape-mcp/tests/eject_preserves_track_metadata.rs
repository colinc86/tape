//! Issue #49 regression at the deck level: `tape.eject` and `tape.snapshot`
//! must preserve `parent_step`, `refs`, and `annotations` on the loaded /
//! converted tracks when round-tripping through the replay path. Before
//! `Session::append_track`, every replay call site went through
//! `append_at(kind, payload, ts)` which hardcoded those three fields to
//! their defaults — so refs (the addresses of spilled artifacts), parent_step
//! linkage, and inline annotations were all silently dropped.

use serde_json::{json, Value};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

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

/// `oversized-payload.tape` has a `file_read` event at step 2 with
/// `refs: ["sha:62cd..."]` pointing at the spilled artifact. After a
/// load → eject round-trip, that `refs` array must survive — otherwise the
/// re-ejected tape has bytes in `artifacts/` (carried by #41) but no event
/// pointing at them.
#[test]
fn eject_preserves_refs_on_loaded_tracks() {
    let deck = tape_mcp::Deck::new();
    let src = fixture_path("oversized-payload.tape");
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("re-ejected.tape");

    let load = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.load", "arguments": {"path": src.to_str().unwrap()}}
    });
    let load_resp = pump(deck.clone(), &[load]);
    let handle = load_resp[0]["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();

    let eject = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "tape.eject", "arguments": {
            "handle": handle, "out": out_path.to_str().unwrap()
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

    // Re-ejected tape must verify clean — if refs were dropped, the file_read
    // event would point at no artifact and `artifacts/` would contain bytes
    // unreferenced by any track. (Coverage of MISSING_ARTIFACT and the
    // converse "orphan artifact" path overlap here.)
    let raw = tape_format::reader::RawTape::open(&out_path).unwrap();
    let report = tape_format::verify::verify(&raw);
    assert!(
        report.is_valid(),
        "re-ejected tape failed verify: {:?}",
        report.errors().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );

    // Source refs are mirrored on the re-ejected tape.
    let src_raw = tape_format::reader::RawTape::open(&src).unwrap();
    let src_tracks =
        tape_format::tracks::parse_jsonl(src_raw.tracks_jsonl.as_deref().unwrap()).unwrap();
    let dst_tracks =
        tape_format::tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap()).unwrap();

    let src_refs: Vec<&Vec<String>> = src_tracks
        .iter()
        .filter(|t| !t.refs.is_empty())
        .map(|t| &t.refs)
        .collect();
    assert!(
        !src_refs.is_empty(),
        "fixture should have at least one event with refs; test setup invariant"
    );
    let dst_refs: Vec<&Vec<String>> = dst_tracks
        .iter()
        .filter(|t| !t.refs.is_empty())
        .map(|t| &t.refs)
        .collect();
    assert_eq!(
        dst_refs, src_refs,
        "refs were dropped on the re-ejected tape (issue #49)"
    );
}

/// `tape.record` → `tape.annotate {step: 1}` → `tape.eject` → reload. The
/// reloaded annotation event must carry `parent_step: Some(1)`. Before
/// `append_track`, the eject replay path clobbered `parent_step` to `None`,
/// so the annotation lost its link to the task it commented on.
#[test]
fn annotate_with_parent_step_survives_eject_round_trip() {
    let deck = tape_mcp::Deck::new();
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("annot-parent-step.tape");

    // Phase 1: open a recording.
    let record_resp = pump(
        deck.clone(),
        &[json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "tape.record", "arguments": {"task": "parent_step survival"}}
        })],
    );
    let handle = record_resp[0]["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();

    // Phase 2: annotate with parent_step = 1 (the task event).
    let annot_resp = pump(
        deck.clone(),
        &[json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "tape.annotate", "arguments": {
                "handle": handle, "note": "links back to task", "step": 1
            }}
        })],
    );
    assert!(
        !annot_resp[0]["result"]["isError"]
            .as_bool()
            .unwrap_or(false),
        "annotate should succeed; got {:?}",
        annot_resp[0]
    );

    // Phase 3: eject.
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

    // Phase 4: reload and inspect.
    let raw = tape_format::reader::RawTape::open(&out_path).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap()).unwrap();
    let annot = tracks
        .iter()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation)
        .expect("annotation event present");
    assert_eq!(
        annot.parent_step,
        Some(1),
        "annotation parent_step was clobbered by the eject replay path (issue #49)"
    );
}
