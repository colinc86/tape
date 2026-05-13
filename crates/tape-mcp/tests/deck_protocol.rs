//! Drives the deck through its JSON-RPC stdio loop and validates the
//! 11-tool contract from the `tape-mcp-deck` skill.

use serde_json::{json, Value};

fn pump<I: IntoIterator<Item = Value>>(requests: I) -> Vec<Value> {
    let mut input = String::new();
    for r in requests {
        input.push_str(&r.to_string());
        input.push('\n');
    }
    let mut output = Vec::<u8>::new();
    let deck = tape_mcp::Deck::new();
    tape_mcp::server::run(input.as_bytes(), &mut output, deck).unwrap();
    let out_str = String::from_utf8(output).unwrap();
    out_str
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn initialize_and_tools_list_returns_twelve() {
    let resp = pump([
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    ]);
    assert_eq!(resp[0]["id"], 1);
    assert_eq!(resp[0]["result"]["protocolVersion"], "2024-11-05");

    assert_eq!(resp[1]["id"], 2);
    let tools = resp[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 12, "expected 12 deck tools");
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "tape.load",
        "tape.summary",
        "tape.tracks",
        "tape.play",
        "tape.seek",
        "tape.tools",
        "tape.diff",
        "tape.fork",
        "tape.record",
        "tape.annotate",
        "tape.eject",
        "tape.snapshot",
    ] {
        assert!(names.contains(&expected), "missing tool: {expected}");
    }
}

#[test]
fn load_then_summary_returns_handle_and_meta() {
    let path = fixture_path("killer-scenario-a.tape");
    let resp = pump([json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.load", "arguments": {"path": path.to_str().unwrap()}}
    })]);
    let result = &resp[0]["result"];
    assert!(result["isError"].as_bool().unwrap_or(false) == false);
    let structured = &result["structuredContent"];
    assert!(structured["handle"].is_string());
    assert!(structured["summary"]["track_count"].as_u64().unwrap() > 0);
}

#[test]
fn full_workflow_load_tracks_play_seek_tools_fork() {
    let path = fixture_path("killer-scenario-a.tape");
    let resp = pump([json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.load", "arguments": {"path": path.to_str().unwrap()}}
    })]);
    let handle = resp[0]["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();

    let r = pump([
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "tape.tracks", "arguments": {"handle": handle}}}),
    ]);
    // Each request is its own deck instance — handle from a different pump call is invalid.
    // Switch to a single multi-request pump for stateful tests.
    assert!(
        r[0]["result"]["isError"].as_bool().unwrap_or(false),
        "isolated pump should reject foreign handle"
    );
}

#[test]
fn stateful_workflow_within_one_session() {
    let path = fixture_path("killer-scenario-a.tape");
    let path_str = path.to_str().unwrap();
    let mut input = String::new();
    input.push_str(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.load", "arguments": {"path": path_str}}
    }).to_string());
    input.push('\n');

    // We don't yet know the handle. Use a single pump and chain by ID, but
    // the second request needs the handle from the first response — so
    // do this in two phases: pump #1 to learn the handle, then pump #2
    // builds a fresh session to drive everything.
    //
    // Easier: use stateful Deck directly and call run() with chained input
    // in one go. But we don't know handle ahead of time.
    //
    // Solution: hand-build a Deck and invoke server::run with an in-process
    // flow that uses the response of #1 to compose #2.

    let deck = tape_mcp::Deck::new();
    let mut buf = Vec::<u8>::new();
    tape_mcp::server::run(input.as_bytes(), &mut buf, deck.clone()).unwrap();
    let load_resp: Value = serde_json::from_str(
        String::from_utf8(buf).unwrap().lines().next().unwrap()
    ).unwrap();
    let handle = load_resp["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();

    // Now exercise stateful tools using the same deck.
    let calls = [
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "tape.summary", "arguments": {"handle": handle}}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "tape.tracks", "arguments": {"handle": handle}}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": {"name": "tape.play", "arguments": {"handle": handle, "step": 1}}}),
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": {"name": "tape.seek", "arguments": {"handle": handle, "query": "smoking gun", "k": 3}}}),
        json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
            "params": {"name": "tape.tools", "arguments": {"handle": handle}}}),
        json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": {"name": "tape.fork", "arguments": {"handle": handle, "from_step": 3}}}),
    ];
    let mut input2 = String::new();
    for c in &calls {
        input2.push_str(&c.to_string());
        input2.push('\n');
    }
    let mut buf2 = Vec::<u8>::new();
    tape_mcp::server::run(input2.as_bytes(), &mut buf2, deck.clone()).unwrap();
    let lines: Vec<Value> = String::from_utf8(buf2)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // tape.summary
    assert!(lines[0]["result"]["structuredContent"]["meta"].is_object());
    // tape.tracks
    let tracks = &lines[1]["result"]["structuredContent"]["tracks"];
    assert!(tracks.as_array().unwrap().len() > 0);
    // tape.play
    let played = &lines[2]["result"]["structuredContent"]["tracks"];
    assert_eq!(played.as_array().unwrap().len(), 1);
    // tape.seek
    let hits = &lines[3]["result"]["structuredContent"]["hits"];
    assert!(
        hits.as_array().unwrap().len() >= 1,
        "expected to find 'smoking gun' annotation"
    );
    // tape.tools
    let calls_field = &lines[4]["result"]["structuredContent"]["calls"];
    assert!(
        calls_field.as_array().unwrap().len() >= 1,
        "killer-scenario-a has an mcp_call"
    );
    // tape.fork
    let new_handle = lines[5]["result"]["structuredContent"]["new_handle"]
        .as_str()
        .unwrap();
    assert_ne!(new_handle, handle);
}

#[test]
fn second_record_without_eject_returns_already_recording() {
    let deck = tape_mcp::Deck::new();
    let mut buf = Vec::<u8>::new();
    let calls = [
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"tape.record","arguments":{"task":"first"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"tape.record","arguments":{"task":"second"}}}),
    ];
    let mut input = String::new();
    for c in &calls {
        input.push_str(&c.to_string());
        input.push('\n');
    }
    tape_mcp::server::run(input.as_bytes(), &mut buf, deck).unwrap();
    let lines: Vec<Value> = String::from_utf8(buf).unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // First record succeeds.
    assert_eq!(
        lines[0]["result"]["isError"].as_bool().unwrap_or(false),
        false,
        "first record should succeed"
    );
    // Second record fails with ALREADY_RECORDING.
    assert_eq!(lines[1]["result"]["isError"], true);
    assert_eq!(lines[1]["result"]["_meta"]["code"], "ALREADY_RECORDING");
}

/// Regression for issue #3: tape.annotate must reject `step` arguments that
/// would write an out-of-range or self-referential `parent_step`. SPEC §5.3
/// requires `parent_step` to be in `[1, step)`. We exercise three bad
/// values — way out of range, zero, and equal-to-next-step — and assert each
/// returns an error rather than a fresh annotation event.
#[test]
fn annotate_rejects_out_of_range_step_arg() {
    let deck = tape_mcp::Deck::new();

    // Phase 1: open a recording so we have a handle with a few tracks.
    let line = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.record", "arguments": {"task": "annotate bounds"}}
    })
    .to_string()
        + "\n";
    let mut buf = Vec::<u8>::new();
    tape_mcp::server::run(line.as_bytes(), &mut buf, deck.clone()).unwrap();
    let resp: Value =
        serde_json::from_str(String::from_utf8(buf).unwrap().lines().next().unwrap()).unwrap();
    let handle = resp["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();

    // The recording starts with a task event at step 1, so next_step is 2.
    // step=2 would equal next_step and therefore violate `< step`; step=9999
    // is way out of range; step=0 is below the `>= 1` floor.
    let bad_calls = [
        ("nine thousand", 9999u64),
        ("equal to next_step", 2u64),
        ("zero", 0u64),
    ];
    for (label, bad_step) in bad_calls {
        let req = json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {
                "name": "tape.annotate",
                "arguments": {"handle": handle, "note": "x", "step": bad_step}
            }
        })
        .to_string()
            + "\n";
        let mut out = Vec::<u8>::new();
        tape_mcp::server::run(req.as_bytes(), &mut out, deck.clone()).unwrap();
        let resp: Value =
            serde_json::from_str(String::from_utf8(out).unwrap().lines().next().unwrap()).unwrap();
        let is_error = resp["result"]["isError"].as_bool().unwrap_or(false);
        assert!(
            is_error,
            "annotate with bad step ({label}={bad_step}) should error; got {resp}"
        );
    }
}

#[test]
fn record_annotate_eject_round_trip() {
    let deck = tape_mcp::Deck::new();
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("session.tape");

    // Phase 1: tape.record → handle
    let mut buf = Vec::<u8>::new();
    let line = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.record", "arguments": {"task": "hello world"}}
    }).to_string() + "\n";
    tape_mcp::server::run(line.as_bytes(), &mut buf, deck.clone()).unwrap();
    let resp: Value =
        serde_json::from_str(String::from_utf8(buf).unwrap().lines().next().unwrap()).unwrap();
    let handle = resp["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(resp["result"]["structuredContent"]["recording"], true);

    // Phase 2: annotate twice, then eject.
    let calls = [
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "tape.annotate", "arguments": {"handle": handle, "note": "first note"}}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "tape.annotate", "arguments": {"handle": handle, "note": "second note"}}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": {"name": "tape.eject", "arguments": {"handle": handle, "out": out_path.to_str().unwrap()}}}),
    ];
    let mut input = String::new();
    for c in &calls {
        input.push_str(&c.to_string());
        input.push('\n');
    }
    let mut buf2 = Vec::<u8>::new();
    tape_mcp::server::run(input.as_bytes(), &mut buf2, deck.clone()).unwrap();
    let out_str = String::from_utf8(buf2).unwrap();
    let lines: Vec<Value> = out_str
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // Eject must succeed and produce a valid .tape file.
    let ej = &lines[2]["result"];
    assert_eq!(ej["isError"].as_bool().unwrap_or(false), false, "eject should succeed; got {ej}");
    assert!(out_path.exists(), "tape file written");
    let raw = tape_format::reader::RawTape::open(&out_path).unwrap();
    let report = tape_format::verify::verify(&raw);
    assert!(
        report.is_valid(),
        "ejected tape should verify; errors: {:?}",
        report.errors().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );
}

/// Helper: spin up a recording, append one annotation, eject to `out` with
/// the given JSON `eject_args` (which must include "handle" and "out").
/// Returns the eject response and the contents of meta.yaml.
fn record_and_eject_with(eject_args: Value) -> (Value, String) {
    let deck = tape_mcp::Deck::new();

    // Phase 1: record.
    let line = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.record", "arguments": {"task": "outcome test"}}
    })
    .to_string()
        + "\n";
    let mut buf = Vec::<u8>::new();
    tape_mcp::server::run(line.as_bytes(), &mut buf, deck.clone()).unwrap();
    let resp: Value =
        serde_json::from_str(String::from_utf8(buf).unwrap().lines().next().unwrap()).unwrap();
    let handle = resp["result"]["structuredContent"]["handle"]
        .as_str()
        .unwrap()
        .to_owned();

    // Phase 2: one annotation + eject with the supplied args.
    let mut args = eject_args;
    args["handle"] = json!(handle);
    let calls = [
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "tape.annotate", "arguments": {"handle": handle, "note": "n"}}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "tape.eject", "arguments": args}}),
    ];
    let mut input = String::new();
    for c in &calls {
        input.push_str(&c.to_string());
        input.push('\n');
    }
    let mut buf2 = Vec::<u8>::new();
    tape_mcp::server::run(input.as_bytes(), &mut buf2, deck.clone()).unwrap();
    let lines: Vec<Value> = String::from_utf8(buf2)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let eject_resp = lines[1]["result"].clone();
    let path = eject_resp["structuredContent"]["path"]
        .as_str()
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    let meta = if path.exists() {
        let raw = tape_format::reader::RawTape::open(&path).unwrap();
        raw.meta_yaml.unwrap_or_default()
    } else {
        String::new()
    };
    (eject_resp, meta)
}

/// Issue #30: a tape.eject call that omits `outcome` should produce a tape
/// whose meta.outcome is `unknown` — not the old hardcoded `success`.
#[test]
fn eject_without_outcome_defaults_to_unknown() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("unknown.tape");
    let (resp, meta) = record_and_eject_with(json!({"out": out_path.to_str().unwrap()}));
    assert_eq!(
        resp["isError"].as_bool().unwrap_or(false),
        false,
        "eject should succeed; got {resp}"
    );
    assert!(
        meta.contains("outcome: unknown"),
        "expected meta.outcome=unknown by default; got:\n{meta}"
    );
}

#[test]
fn eject_with_outcome_failure_records_failure_in_meta() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("failure.tape");
    let (resp, meta) = record_and_eject_with(json!({
        "out": out_path.to_str().unwrap(),
        "outcome": "failure",
    }));
    assert_eq!(resp["isError"].as_bool().unwrap_or(false), false);
    assert!(
        meta.contains("outcome: failure"),
        "expected meta.outcome=failure; got:\n{meta}"
    );
}

#[test]
fn eject_with_invalid_outcome_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("bad.tape");
    let (resp, _meta) = record_and_eject_with(json!({
        "out": out_path.to_str().unwrap(),
        "outcome": "in_progress",
    }));
    assert!(
        resp["isError"].as_bool().unwrap_or(false),
        "expected error for invalid outcome; got {resp}"
    );
}
