//! JSON-RPC over stdio loop. Read newline-delimited JSON, dispatch, write
//! responses on stdout. Synchronous because MCP over stdio is sequential
//! per the spec (no out-of-order responses required).

use std::io::{BufRead, BufReader, Read, Write};

use serde_json::{json, Value};

use crate::deck::Deck;
use crate::jsonrpc::{
    Request, Response, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR,
};
use crate::tools;

/// Run the stdio MCP loop until stdin is closed.
pub fn stdio_loop() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let deck = Deck::new();

    let reader = BufReader::new(stdin);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle_line(&deck, &line) {
            let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap());
            let _ = out.flush();
        }
    }
    Ok(())
}

/// Run the loop with arbitrary reader/writer (used by tests).
pub fn run<R: Read, W: Write>(reader: R, mut writer: W, deck: Deck) -> std::io::Result<()> {
    let buf = BufReader::new(reader);
    for line in buf.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle_line(&deck, &line) {
            writeln!(writer, "{}", serde_json::to_string(&resp).unwrap())?;
            writer.flush()?;
        }
    }
    Ok(())
}

/// Process one JSON-RPC line. Returns `None` for notifications — requests
/// with no `id` member — because JSON-RPC 2.0 §4.1 forbids replying to
/// them. (Issue #56.) Everything else returns the response the caller
/// must serialize and flush.
fn handle_line(deck: &Deck, line: &str) -> Option<Response> {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            // Parse failure: we can't tell whether the message was a
            // Request or a Notification. JSON-RPC §4.2 allows id=null
            // here; we still emit the error so a strict client at least
            // learns that its line was unparsable.
            return Some(Response::err(None, PARSE_ERROR, format!("parse error: {e}")));
        }
    };
    if req.jsonrpc != "2.0" {
        return Some(Response::err(
            req.id,
            INVALID_REQUEST,
            "jsonrpc must be '2.0'",
        ));
    }

    // Notification — fire-and-forget. We dispatch the side-effect-free
    // standard MCP notifications (`notifications/initialized`,
    // `notifications/cancelled`, etc.) only by parsing them; the deck
    // has no notification state machine yet, so nothing else to do.
    if req.id.is_none() {
        return None;
    }

    let id = req.id.clone();

    Some(match req.method.as_str() {
        "initialize" => Response::ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {
                    "name": "tape",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "shutdown" => Response::ok(id, Value::Null),
        "tools/list" => {
            let defs: Vec<Value> = tools::definitions()
                .into_iter()
                .map(|d| {
                    json!({
                        "name": d.name,
                        "description": d.description,
                        "inputSchema": d.input_schema,
                    })
                })
                .collect();
            Response::ok(id, json!({"tools": defs}))
        }
        "tools/call" => {
            let params = req.params.unwrap_or(Value::Null);
            let name = match params.get("name").and_then(Value::as_str) {
                Some(n) => n,
                None => {
                    return Some(Response::err(id, INVALID_PARAMS, "missing `name`"));
                }
            };
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            match tools::dispatch(deck, name, &args) {
                Ok(result) => Response::ok(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string(&result).unwrap()
                        }],
                        "structuredContent": result,
                    }),
                ),
                Err(e) => Response::ok(
                    id,
                    json!({
                        "content": [{"type": "text", "text": e.message}],
                        "isError": true,
                        "_meta": {"code": e.code}
                    }),
                ),
            }
        }
        _ => Response::err(id, METHOD_NOT_FOUND, format!("unknown method: {}", req.method)),
    })
}
