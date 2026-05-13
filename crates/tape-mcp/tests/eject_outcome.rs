//! Regression tests for issue #30: `tape.eject` must accept an optional
//! `outcome` arg ("success" | "failure" | "abandoned" | "unknown") and
//! default to `Unknown` when omitted, matching `tape.snapshot`. Before the
//! fix, `meta.outcome` was hardcoded to `success` regardless of input.

use serde_json::{json, Value};

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

/// Open a recording with one annotation, eject with the supplied arguments,
/// and return the response and (on success) the parsed ejected tape.
fn record_and_eject(
    out: &std::path::Path,
    extra_eject_args: Value,
) -> (
    Value,
    Option<(tape_format::meta::Meta, Vec<tape_format::tracks::Track>)>,
) {
    let deck = tape_mcp::Deck::new();
    let record_resp = pump(
        deck.clone(),
        &[json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "tape.record", "arguments": {"task": "outcome test"}}
        })],
    );
    let handle = record_resp[0]["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();

    // Add an annotation so the tape isn't empty (NOT_RECORDING gating).
    let _ = pump(
        deck.clone(),
        &[json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "tape.annotate", "arguments": {
                "handle": handle, "note": "marker"
            }}
        })],
    );

    let mut eject_args = json!({
        "handle": handle,
        "out": out.to_str().unwrap(),
    });
    if let Value::Object(extra) = extra_eject_args {
        for (k, v) in extra {
            eject_args[k] = v;
        }
    }

    let eject_resp = pump(
        deck,
        &[json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "tape.eject", "arguments": eject_args}
        })],
    );

    let resp = eject_resp[0].clone();
    if resp["result"]["isError"].as_bool().unwrap_or(false) {
        return (resp, None);
    }
    assert!(out.exists(), "tape file written when eject succeeds");

    let raw = tape_format::reader::RawTape::open(out).unwrap();
    let meta = tape_format::meta::Meta::parse(raw.meta_yaml.as_deref().unwrap()).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(&raw.tracks_jsonl.unwrap()).unwrap();
    (resp, Some((meta, tracks)))
}

/// Locate the eject event in a track list. Eject is always the final event
/// per SPEC §4.x.
fn eject_event(tracks: &[tape_format::tracks::Track]) -> &tape_format::tracks::Track {
    tracks
        .iter()
        .rev()
        .find(|t| t.kind == tape_format::tracks::Kind::Eject)
        .expect("eject event present at end of tape")
}

/// Default behavior — no `outcome` arg supplied. Issue #30 fix says this
/// must produce `unknown`, not the old hardcoded `success`.
#[test]
fn eject_with_no_outcome_arg_defaults_to_unknown() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("default.tape");
    let (resp, parsed) = record_and_eject(&out_path, json!({}));
    assert!(
        !resp["result"]["isError"].as_bool().unwrap_or(false),
        "eject should succeed; got {resp:?}"
    );
    let (meta, tracks) = parsed.expect("parsed tape");

    assert_eq!(
        meta.outcome,
        tape_format::meta::Outcome::Unknown,
        "default meta.outcome must be unknown, not success"
    );
    assert_eq!(
        eject_event(&tracks).payload["outcome"].as_str().unwrap(),
        "unknown",
        "default eject event payload.outcome must be \"unknown\""
    );
}

/// Explicit success — round-trips to `success` in both meta and payload.
#[test]
fn eject_with_outcome_success_sets_success() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("success.tape");
    let (resp, parsed) = record_and_eject(&out_path, json!({"outcome": "success"}));
    assert!(
        !resp["result"]["isError"].as_bool().unwrap_or(false),
        "eject should succeed; got {resp:?}"
    );
    let (meta, tracks) = parsed.expect("parsed tape");

    assert_eq!(meta.outcome, tape_format::meta::Outcome::Success);
    assert_eq!(
        eject_event(&tracks).payload["outcome"].as_str().unwrap(),
        "success"
    );
}

/// Explicit failure — round-trips to `failure` in both meta and payload.
#[test]
fn eject_with_outcome_failure_sets_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("failure.tape");
    let (resp, parsed) = record_and_eject(&out_path, json!({"outcome": "failure"}));
    assert!(
        !resp["result"]["isError"].as_bool().unwrap_or(false),
        "eject should succeed; got {resp:?}"
    );
    let (meta, tracks) = parsed.expect("parsed tape");

    assert_eq!(meta.outcome, tape_format::meta::Outcome::Failure);
    assert_eq!(
        eject_event(&tracks).payload["outcome"].as_str().unwrap(),
        "failure"
    );
}

/// Invalid outcome — params-class error, no tape produced.
#[test]
fn eject_with_invalid_outcome_returns_params_error() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("huh.tape");
    let (resp, parsed) = record_and_eject(&out_path, json!({"outcome": "huh"}));
    assert!(
        resp["result"]["isError"].as_bool().unwrap_or(false),
        "invalid outcome must produce an error response; got {resp:?}"
    );
    assert_eq!(
        resp["result"]["_meta"]["code"].as_str().unwrap(),
        "INVALID_PARAMS",
        "error must be classified as INVALID_PARAMS"
    );
    assert!(
        parsed.is_none(),
        "no parsed tape expected on error path"
    );
    assert!(
        !out_path.exists(),
        "no tape file should be written on params error"
    );
}
