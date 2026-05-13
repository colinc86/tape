//! Regression tests for issue #41: `tape.eject` of a loaded handle must
//! carry the source tape's spilled artifact bytes through into the new
//! tape. Before the fix, `tool_eject` constructed a fresh `Session` from
//! the loaded tracks (which contain `{"ref": "sha:<hex>"}` stubs but no
//! bytes), and the eject pipeline created an empty artifact map — so the
//! resulting tape failed `tape verify MISSING_ARTIFACT`.
//!
//! Orphan dropping is part of the contract: artifacts that no surviving
//! track references (e.g. after a fork truncation) must be removed before
//! the tape is written, so the resulting tape stays minimal.

use serde_json::{json, Value};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Drive a sequence of JSON-RPC requests through one deck and parse the
/// responses. Sharing the deck across calls keeps handles valid.
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

fn load_call(path: &std::path::Path, id: u64) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": "tape.load", "arguments": {"path": path.to_str().unwrap()}}
    })
}

fn eject_call(handle: &str, out: &std::path::Path, id: u64) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": "tape.eject", "arguments": {
            "handle": handle, "out": out.to_str().unwrap()
        }}
    })
}

fn fork_call(handle: &str, from_step: u64, id: u64) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": "tape.fork", "arguments": {
            "handle": handle, "from_step": from_step
        }}
    })
}

fn handle_of(resp: &Value) -> String {
    resp["result"]["structuredContent"]["handle"]
        .as_str()
        .expect("handle string")
        .to_owned()
}

fn assert_ok(resp: &Value) {
    let is_err = resp["result"]["isError"].as_bool().unwrap_or(false);
    assert!(!is_err, "expected success; got {resp:?}");
}

/// Issue #41 — the reproducer. Load the oversized-payload fixture (whose
/// tracks reference a spilled artifact) via the deck, eject to a temp
/// path, and confirm:
///   1. the result has no `MISSING_ARTIFACT` diagnostic, and
///   2. `raw.artifacts` is non-empty (the source bytes were carried
///      through the eject pipeline rather than silently dropped).
///
/// Before the fix, `tool_eject` rebuilt a fresh Session from the loaded
/// tracks and called the eject pipeline with an empty artifact map; the
/// `{"ref": "sha:..."}` stubs on the new tracks landed without bytes,
/// and the resulting tape failed `verify MISSING_ARTIFACT`.
///
/// We deliberately do not assert `is_valid()` here because the fixture
/// already contains an `eject` event of its own, and `tool_eject` of a
/// loaded tape currently appends a second one (issue #26 / PR #32). The
/// `EjectNotLast` diagnostic that produces is out of scope for #41.
#[test]
fn eject_of_loaded_tape_preserves_spilled_artifacts() {
    let deck = tape_mcp::Deck::new();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("round-tripped.tape");
    let src = fixture_path("oversized-payload.tape");

    let load_resp = pump(deck.clone(), &[load_call(&src, 1)]);
    assert_ok(&load_resp[0]);
    let handle = handle_of(&load_resp[0]);

    let eject_resp = pump(deck, &[eject_call(&handle, &out, 2)]);
    assert_ok(&eject_resp[0]);
    assert!(out.exists(), "ejected tape should be written");

    // Read back the resulting tape and check for the specific
    // diagnostic this issue is about. Other diagnostics (#26's
    // EjectNotLast) are tolerated — they have their own fix in flight.
    let raw = tape_format::reader::RawTape::open(&out).expect("open ejected tape");
    let report = tape_format::verify::verify(&raw);
    let missing_artifact = report
        .errors()
        .any(|d| matches!(d.code, tape_format::verify::DiagnosticCode::MissingArtifact));
    assert!(
        !missing_artifact,
        "ejected tape still has MISSING_ARTIFACT diagnostics: {:?}",
        report.diagnostics
    );

    // The fixture's referenced bytes must be present in the new tape.
    assert!(
        !raw.artifacts.is_empty(),
        "ejected tape has no artifacts — the spilled bytes were dropped"
    );
}

/// Issue #41 — round-trip identity. Load → eject (no fork) → reload, and
/// confirm the artifact set on the reloaded tape matches the set in the
/// source fixture. We compare keys (canonical zip paths) and their byte
/// contents.
#[test]
fn round_trip_eject_preserves_full_artifact_set() {
    let deck = tape_mcp::Deck::new();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("identity.tape");
    let src = fixture_path("oversized-payload.tape");

    let load_resp = pump(deck.clone(), &[load_call(&src, 1)]);
    let handle = handle_of(&load_resp[0]);
    let eject_resp = pump(deck, &[eject_call(&handle, &out, 2)]);
    assert_ok(&eject_resp[0]);

    let src_raw = tape_format::reader::RawTape::open(&src).unwrap();
    let dst_raw = tape_format::reader::RawTape::open(&out).unwrap();

    let src_keys: std::collections::BTreeSet<&str> =
        src_raw.artifacts.keys().map(String::as_str).collect();
    let dst_keys: std::collections::BTreeSet<&str> =
        dst_raw.artifacts.keys().map(String::as_str).collect();
    assert_eq!(
        src_keys, dst_keys,
        "artifact key set differs between source and ejected tape"
    );

    for (k, v) in &src_raw.artifacts {
        let dst_bytes = dst_raw
            .artifacts
            .get(k)
            .unwrap_or_else(|| panic!("missing artifact {k} on ejected tape"));
        assert_eq!(
            dst_bytes.len(),
            v.len(),
            "byte length mismatch for artifact {k}"
        );
        assert_eq!(dst_bytes, v, "byte contents differ for artifact {k}");
    }
}

/// Issue #41 — orphan dropping. Fork the oversized-payload fixture at
/// step 1 (the Task event), truncating the only track that references
/// the spilled artifact. Ejecting the fork must produce a clean tape
/// with no leftover artifact bytes.
#[test]
fn fork_then_eject_drops_orphaned_artifacts() {
    let deck = tape_mcp::Deck::new();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("fork-orphan.tape");
    let src = fixture_path("oversized-payload.tape");

    // Load.
    let load_resp = pump(deck.clone(), &[load_call(&src, 1)]);
    let handle = handle_of(&load_resp[0]);

    // Fork at step 1 — truncates the file_read at step 2, which is the
    // only track referencing the artifact. (Step 3 is `eject`; the eject
    // pipeline appends a fresh eject event so the source tape's eject
    // track gets dropped too.)
    let fork_resp = pump(deck.clone(), &[fork_call(&handle, 1, 2)]);
    assert_ok(&fork_resp[0]);
    let new_handle = fork_resp[0]["result"]["structuredContent"]["new_handle"]
        .as_str()
        .unwrap()
        .to_owned();

    // Eject the fork.
    let eject_resp = pump(deck, &[eject_call(&new_handle, &out, 3)]);
    assert_ok(&eject_resp[0]);

    let raw = tape_format::reader::RawTape::open(&out).expect("open ejected tape");
    let report = tape_format::verify::verify(&raw);
    assert!(
        report.is_valid(),
        "ejected fork failed verify; diagnostics: {:?}",
        report.diagnostics
    );

    // The fixture's artifact is no longer referenced by any surviving
    // track — orphan dropping should have removed it. Any artifact the
    // pipeline still emits would have to be one the spillover loop
    // *produced* from the surviving payloads, which the trimmed fork
    // (a single small Task event) does not trigger.
    assert!(
        raw.artifacts.is_empty(),
        "expected no artifacts on truncated fork; got {:?}",
        raw.artifacts.keys().collect::<Vec<_>>()
    );
}

/// Issue #41 — orphan dropping must preserve artifacts still referenced
/// by surviving tracks. Fork at step 2 (keeps the file_read), eject,
/// and confirm the artifact set is preserved (not dropped as orphan).
#[test]
fn fork_keeping_referencing_track_preserves_artifact() {
    let deck = tape_mcp::Deck::new();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("fork-keep.tape");
    let src = fixture_path("oversized-payload.tape");

    let load_resp = pump(deck.clone(), &[load_call(&src, 1)]);
    let handle = handle_of(&load_resp[0]);

    // Fork at step 2 — keeps the file_read that references the artifact.
    let fork_resp = pump(deck.clone(), &[fork_call(&handle, 2, 2)]);
    assert_ok(&fork_resp[0]);
    let new_handle = fork_resp[0]["result"]["structuredContent"]["new_handle"]
        .as_str()
        .unwrap()
        .to_owned();

    let eject_resp = pump(deck, &[eject_call(&new_handle, &out, 3)]);
    assert_ok(&eject_resp[0]);

    let raw = tape_format::reader::RawTape::open(&out).unwrap();
    let report = tape_format::verify::verify(&raw);
    assert!(
        report.is_valid(),
        "ejected fork failed verify; diagnostics: {:?}",
        report.diagnostics
    );

    // Source artifact set should survive — its referencing track is in
    // the fork.
    let src_raw = tape_format::reader::RawTape::open(&src).unwrap();
    let src_keys: std::collections::BTreeSet<&str> =
        src_raw.artifacts.keys().map(String::as_str).collect();
    let dst_keys: std::collections::BTreeSet<&str> =
        raw.artifacts.keys().map(String::as_str).collect();
    assert_eq!(
        src_keys, dst_keys,
        "fork that retains the referencing track should retain its artifacts"
    );
}
