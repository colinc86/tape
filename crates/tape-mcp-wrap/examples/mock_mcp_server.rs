//! Tiny mock MCP-shaped server for integration testing.
//!
//! Reads newline-delimited JSON-RPC requests on stdin; writes responses on stdout.
//! Recognized methods:
//!  - `initialize`        → returns a static result
//!  - `tools/list`        → returns one fake tool
//!  - `tools/call`        → returns `{ok: true, args_received: <args>}`
//!  - everything else     → returns method-not-found

use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(serde_json::Value::Null);

        let response: serde_json::Value = match method {
            "initialize" => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"protocolVersion": "2024-11-05", "capabilities": {}}
            }),
            "tools/list" => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"tools": [{"name": "echo", "description": "echoes args"}]}
            }),
            "tools/call" => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"ok": true, "args_received": params.get("arguments").cloned().unwrap_or(serde_json::Value::Null)}
            }),
            _ => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": "method not found"}
            }),
        };

        let s = response.to_string();
        let _ = writeln!(out, "{s}");
        let _ = out.flush();
    }
}
