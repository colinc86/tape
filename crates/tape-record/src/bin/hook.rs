//! `tape-hook` — small CLI invoked from a Claude Code `PostToolUse` /
//! `PreToolUse` hook. Reads the hook event JSON on stdin, translates it into
//! a `tape/v0` track event (`shell`, `file_read`, or `file_write`), and posts
//! it to the recorder Unix socket configured via `TAPE_RECORDER_SOCKET`.
//!
//! Exits 0 on success and 0 on transient failures too — a hook that returns
//! non-zero blocks Claude Code's tool flow, which we never want to do for a
//! recording side-channel. Failures are emitted on stderr for diagnostics.

use std::io::Read;
use std::os::unix::net::UnixStream;

use serde_json::Value;

fn main() {
    let mut stdin = std::io::stdin();
    let mut buf = String::new();
    if let Err(e) = stdin.read_to_string(&mut buf) {
        eprintln!("tape-hook: read stdin: {e}");
        return;
    }

    let v: Value = match serde_json::from_str(&buf) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("tape-hook: parse stdin JSON: {e}");
            return;
        }
    };

    let tool_name = v.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let tool_input = v.get("tool_input").cloned().unwrap_or(Value::Null);
    let tool_response = v.get("tool_response").cloned().unwrap_or(Value::Null);

    let event = match tool_name {
        "Bash" => Some(shell_event(&tool_input, &tool_response)),
        "Read" => Some(file_read_event(&tool_input, &tool_response)),
        "Write" | "Edit" | "MultiEdit" => {
            Some(file_write_event(tool_name, &tool_input, &tool_response))
        }
        _ => None,
    };

    let Some(event) = event else { return };

    let socket_path = match std::env::var("TAPE_RECORDER_SOCKET") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("tape-hook: TAPE_RECORDER_SOCKET not set; skipping");
            return;
        }
    };

    if let Err(e) = post_event(&socket_path, &event) {
        eprintln!("tape-hook: post failed: {e}");
    }
}

fn shell_event(input: &Value, response: &Value) -> Value {
    let command = input
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let exit_code = response
        .get("exit_code")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let stdout = response
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let stderr = response
        .get("stderr")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let duration_ms = response
        .get("duration_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    serde_json::json!({
        "kind": "shell",
        "payload": {
            "command": command,
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
            "duration_ms": duration_ms
        }
    })
}

fn file_read_event(input: &Value, response: &Value) -> Value {
    let path = input
        .get("file_path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // If the response includes file_content, hash it so the consumer can
    // correlate. Otherwise, omit the field entirely — emitting a fake
    // `blake3:0` would violate the format's hash format. Readers should
    // treat content_hash as optional in the file_read payload.
    let content_hash = response
        .get("file_content")
        .and_then(Value::as_str)
        .map(|c| format!("blake3:{}", blake3::hash(c.as_bytes()).to_hex()));

    let mut payload = serde_json::json!({"path": path});
    if let Some(h) = content_hash {
        payload["content_hash"] = serde_json::Value::String(h);
    }
    serde_json::json!({
        "kind": "file_read",
        "payload": payload
    })
}

fn file_write_event(tool_name: &str, input: &Value, response: &Value) -> Value {
    let path = input
        .get("file_path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // After-hash from new content, or from response file_content if present.
    let after_source = match tool_name {
        "Write" => input
            .get("content")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => response
            .get("file_content")
            .and_then(Value::as_str)
            .map(str::to_owned),
    };
    // Omit after_hash when content isn't available rather than emitting a
    // sentinel like "blake3:0" — the format expects 64 hex chars after
    // the prefix, and a sentinel violates that. Readers should treat
    // after_hash as optional.
    let after_hash = after_source
        .as_deref()
        .map(|s| format!("blake3:{}", blake3::hash(s.as_bytes()).to_hex()));

    let diff = match tool_name {
        "Edit" => {
            let old = input
                .get("old_string")
                .and_then(Value::as_str)
                .unwrap_or("");
            let new = input
                .get("new_string")
                .and_then(Value::as_str)
                .unwrap_or("");
            Some(simple_diff(old, new))
        }
        "MultiEdit" => Some("(multi-edit)".to_string()),
        _ => None,
    };

    let mut payload = serde_json::json!({
        "path": path,
        "before_hash": serde_json::Value::Null,
    });
    if let Some(h) = after_hash {
        payload["after_hash"] = Value::String(h);
    }
    if let Some(d) = diff {
        payload["diff"] = Value::String(d);
    }
    serde_json::json!({
        "kind": "file_write",
        "payload": payload
    })
}

fn simple_diff(old: &str, new: &str) -> String {
    format!(
        "- {}\n+ {}",
        old.replace('\n', "\\n"),
        new.replace('\n', "\\n")
    )
}

fn post_event(socket_path: &str, event: &Value) -> std::io::Result<()> {
    use std::io::Write;
    let mut stream = UnixStream::connect(socket_path)?;
    let line = event.to_string();
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(())
}
