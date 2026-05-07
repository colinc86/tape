//! The single demo that must work for v0 to ship.
//!
//! Engineer A's tape (the `killer-scenario-a.tape` fixture) embeds a known
//! smoking-gun fact: `customer CUST-447139` and `process_refund()`.
//! Engineer B drives `tape mcp` over stdio and must surface that fact via
//! a sequence of deck calls: `tape.load` → `tape.summary` (or `tape.seek`)
//! → `tape.play` → final synthesized answer.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
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
fn engineer_b_finds_smoking_gun_via_deck() {
    let tape_path = fixture_path("killer-scenario-a.tape");
    assert!(tape_path.exists(), "fixture must exist");

    let mut child = Command::new(binary_path())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn tape mcp");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let mut send = |req: Value| {
        writeln!(stdin, "{}", req).unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let resp: Value = serde_json::from_str(line.trim()).unwrap();
        resp
    };

    // 1. initialize
    let init = send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
    }));
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");

    // 2. tape.load
    let load = send(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "tape.load", "arguments": {"path": tape_path.to_str().unwrap()}}
    }));
    let handle = load["result"]["structuredContent"]["handle"]
        .as_str()
        .expect("handle returned")
        .to_owned();

    // 3. tape.seek for the smoking gun
    let seek = send(json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "tape.seek", "arguments": {"handle": handle, "query": "smoking gun", "k": 5}}
    }));
    let hits = seek["result"]["structuredContent"]["hits"].as_array().expect("hits");
    assert!(!hits.is_empty(), "expected to surface the annotation");
    let smoking_step = hits[0]["step"].as_u64().expect("step");

    // 4. tape.play that step to get the full annotation payload
    let play = send(json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": {"name": "tape.play", "arguments": {"handle": handle, "step": smoking_step}}
    }));
    let played = &play["result"]["structuredContent"]["tracks"][0];
    let note = played["payload"]["note"]
        .as_str()
        .expect("annotation note");

    // 5. The smoking-gun fact: customer CUST-447139 and process_refund()
    assert!(
        note.contains("CUST-447139"),
        "Engineer B's answer must reference customer CUST-447139; got: {note}"
    );
    assert!(
        note.contains("process_refund"),
        "Engineer B's answer must reference the function; got: {note}"
    );

    drop(stdin);
    let _ = child.wait();
}
