//! Issue #41 regression: `tape.load` of a tape with spilled artifacts,
//! followed by `tape.eject` to a new path, must produce a tape that
//! contains the same `artifacts/*.bin` bytes — not an empty `artifacts/`
//! directory that would fail `tape verify` with `MISSING_ARTIFACT`.

use serde_json::{json, Value};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn eject_carries_loaded_artifacts_through_to_new_tape() {
    let deck = tape_mcp::Deck::new();
    let src = fixture_path("oversized-payload.tape");
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("re-ejected.tape");

    // Phase 1: load → handle.
    let load_req = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.load", "arguments": {"path": src.to_str().unwrap()}}
    })
    .to_string()
        + "\n";
    let mut buf = Vec::<u8>::new();
    tape_mcp::server::run(load_req.as_bytes(), &mut buf, deck.clone()).unwrap();
    let load_resp: Value =
        serde_json::from_str(String::from_utf8(buf).unwrap().lines().next().unwrap()).unwrap();
    let handle = load_resp["result"]["structuredContent"]["handle"]
        .as_str()
        .expect("load returned a handle")
        .to_owned();

    // Phase 2: eject to the temp path.
    let eject_req = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "tape.eject",
            "arguments": {"handle": handle, "out": out_path.to_str().unwrap()}
        }
    })
    .to_string()
        + "\n";
    let mut buf2 = Vec::<u8>::new();
    tape_mcp::server::run(eject_req.as_bytes(), &mut buf2, deck).unwrap();
    let eject_resp: Value =
        serde_json::from_str(String::from_utf8(buf2).unwrap().lines().next().unwrap()).unwrap();
    assert!(
        !eject_resp["result"]["isError"].as_bool().unwrap_or(false),
        "eject reported an error: {eject_resp}"
    );
    assert!(out_path.exists(), "tape file written");

    // Phase 3: the re-ejected tape must verify clean. Pre-#41 it failed
    // MISSING_ARTIFACT because the spilled bytes were dropped on the floor.
    let raw = tape_format::reader::RawTape::open(&out_path).unwrap();
    let report = tape_format::verify::verify(&raw);
    assert!(
        report.is_valid(),
        "re-ejected tape failed verify: {:?}",
        report.errors().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );

    // The bytes match the source's artifact (content-addressed).
    let src_raw = tape_format::reader::RawTape::open(&src).unwrap();
    assert!(
        !src_raw.artifacts.is_empty(),
        "fixture should have spilled artifacts; test setup invariant"
    );
    for (path, bytes) in &src_raw.artifacts {
        let carried = raw
            .artifacts
            .get(path)
            .unwrap_or_else(|| panic!("artifact {path} missing from re-ejected tape"));
        assert_eq!(carried, bytes, "artifact bytes diverged for {path}");
    }
}
