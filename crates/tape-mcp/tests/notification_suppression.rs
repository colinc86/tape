//! Regression: JSON-RPC 2.0 §4.1 — the server MUST NOT reply to a
//! Notification (a request with no `id`). Issue #56.
//!
//! Before the fix, the deck wrote a `METHOD_NOT_FOUND` (or any error)
//! response for every notification it received, which clients like MCP
//! correctly flagged as a protocol violation when they sent the standard
//! `notifications/initialized` lifecycle message.

use serde_json::{json, Value};

/// Drive the server with one or more JSON-RPC lines and return the raw
/// stdout output. Tests assert on this string directly so they can
/// distinguish "no response" from "response with null id".
fn run_lines<I: IntoIterator<Item = Value>>(requests: I) -> String {
    let mut input = String::new();
    for r in requests {
        input.push_str(&r.to_string());
        input.push('\n');
    }
    let mut output = Vec::<u8>::new();
    let deck = tape_mcp::Deck::new();
    tape_mcp::server::run(input.as_bytes(), &mut output, deck).unwrap();
    String::from_utf8(output).unwrap()
}

/// Reproducer for #56: `notifications/initialized` is the MCP lifecycle
/// notification sent by every conforming client right after `initialize`.
/// It has no `id` field, so the deck MUST stay silent.
#[test]
fn initialized_notification_produces_no_response() {
    let out = run_lines([json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })]);
    assert!(
        out.is_empty(),
        "notification must not be replied to; got: {out:?}"
    );
}

/// Regression guard: a normal request that carries an `id` still gets a
/// response. The notification suppression must not swallow real requests.
#[test]
fn request_with_id_still_gets_response() {
    let out = run_lines([json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    })]);
    let resp: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
}

/// Unparseable lines still produce a `PARSE_ERROR` reply with `id: null`.
/// We can't tell from invalid JSON whether the sender intended a
/// notification, and JSON-RPC §4.2 explicitly permits replying with
/// `id: null` in this case. Best-effort visibility wins.
#[test]
fn parse_error_still_responds_with_null_id() {
    let mut input = String::from("{not valid json\n");
    let mut output = Vec::<u8>::new();
    let deck = tape_mcp::Deck::new();
    // Build the bytes by hand so we don't have to escape the broken JSON
    // through `serde_json::to_string`.
    input.push('\n');
    tape_mcp::server::run(input.as_bytes(), &mut output, deck).unwrap();
    let out_str = String::from_utf8(output).unwrap();
    let resp: Value = serde_json::from_str(out_str.trim()).unwrap();
    assert_eq!(resp["error"]["code"], -32700);
    assert!(resp["id"].is_null());
}

/// A request with an unknown method still errors normally when it has
/// an `id`. Notification suppression only kicks in when `id` is absent.
#[test]
fn unknown_method_with_id_still_returns_method_not_found() {
    let out = run_lines([json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "no/such/method"
    })]);
    let resp: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(resp["id"], 42);
    assert_eq!(resp["error"]["code"], -32601);
}

/// Other standard MCP notifications must also be silently dropped.
/// `notifications/progress` and `notifications/cancelled` are the two
/// other common ones clients fire mid-session.
#[test]
fn other_mcp_notifications_produce_no_response() {
    let out = run_lines([
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/progress",
            "params": {"progressToken": "tok", "progress": 0.5}
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/cancelled",
            "params": {"requestId": 7, "reason": "user abort"}
        }),
    ]);
    assert!(
        out.is_empty(),
        "notifications must be silently dropped; got: {out:?}"
    );
}

/// A notification with a wrong `jsonrpc` version is still a notification —
/// no `id` means no reply, even though we'd otherwise have flagged it as
/// `INVALID_REQUEST`.
#[test]
fn notification_with_bad_jsonrpc_version_still_silent() {
    let out = run_lines([json!({
        "jsonrpc": "1.0",
        "method": "notifications/initialized"
    })]);
    assert!(
        out.is_empty(),
        "version-mismatched notification still must not be replied to; got: {out:?}"
    );
}

/// Interleaved: a notification between two real requests must not shift
/// or duplicate the request replies. The output should have exactly two
/// lines, one per request, with matching ids.
#[test]
fn notification_between_requests_does_not_disturb_replies() {
    let out = run_lines([
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    ]);
    let lines: Vec<Value> = out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2, "expected 2 responses (notification dropped); got: {out}");
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[1]["id"], 2);
}
