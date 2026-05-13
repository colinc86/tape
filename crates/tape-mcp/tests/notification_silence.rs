//! Issue #56 regression: JSON-RPC 2.0 §4.1 says the server MUST NOT reply
//! to a Notification (a Request without an `id` member). Even unknown
//! notification methods must produce silence, not an error response.

use serde_json::json;

fn run(input: &str) -> String {
    let mut out = Vec::<u8>::new();
    tape_mcp::server::run(input.as_bytes(), &mut out, tape_mcp::Deck::new()).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn unknown_notification_method_emits_no_response() {
    let resp = run(&(json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
    .to_string()
        + "\n"));
    assert!(
        resp.is_empty(),
        "expected silence for notification; got: {resp}"
    );
}

#[test]
fn well_known_notification_methods_are_silent() {
    let lines: String = [
        "notifications/initialized",
        "notifications/cancelled",
        "notifications/progress",
    ]
    .iter()
    .map(|m| json!({"jsonrpc": "2.0", "method": m}).to_string() + "\n")
    .collect();
    let resp = run(&lines);
    assert!(
        resp.is_empty(),
        "no notification should produce a response; got: {resp}"
    );
}

/// A Notification mixed with a regular Request: only the Request gets a
/// response, in order.
#[test]
fn notification_in_between_requests_does_not_emit() {
    let mut buf = String::new();
    buf.push_str(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}).to_string());
    buf.push('\n');
    buf.push_str(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string());
    buf.push('\n');
    buf.push_str(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string());
    buf.push('\n');

    let resp = run(&buf);
    let lines: Vec<&str> = resp.lines().collect();
    assert_eq!(lines.len(), 2, "exactly one response per Request; got {resp:?}");
    // ids 1, 2 in order — no `id: null` from the notification.
    let r1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let r2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(r1["id"].as_u64(), Some(1));
    assert_eq!(r2["id"].as_u64(), Some(2));
}

/// A regular Request whose method is unknown still gets an error response —
/// this is the inverse contract. Pre-fix, this also fired for notifications.
#[test]
fn unknown_request_method_still_errors() {
    let resp = run(&(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "totally/unknown"
    })
    .to_string()
        + "\n"));
    let parsed: serde_json::Value = serde_json::from_str(resp.trim()).unwrap();
    assert_eq!(parsed["id"].as_u64(), Some(1));
    assert!(parsed["error"].is_object(), "expected error; got {parsed}");
}
