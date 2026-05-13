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
use similar::TextDiff;

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
    // Prefer hashing the inline `file_content` when present — it's authoritative
    // for what the tool returned. If the response omits content, fall back to
    // reading the file from disk at hook time; this is a small race but the
    // hook runs in PostToolUse right after the Read tool returns, so contents
    // are overwhelmingly stable. If both paths fail (file deleted, permission
    // error), omit `content_hash` rather than emit a sentinel that violates
    // the v0 hash format.
    let content_hash = response
        .get("file_content")
        .and_then(Value::as_str)
        .map(|c| hash_str(c))
        .or_else(|| {
            if path.is_empty() {
                None
            } else {
                std::fs::read(&path).ok().map(|b| hash_bytes(&b))
            }
        });

    let mut payload = serde_json::json!({"path": path});
    if let Some(h) = content_hash {
        payload["content_hash"] = serde_json::Value::String(h);
    } else {
        eprintln!("tape-hook: could not compute content_hash for {path}");
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

    // Reconstruct (old_content, new_content) for this write:
    // - Write: old = "" (we don't know pre-state until PR 2's PreToolUse hook;
    //   treating Write as "new file" gives an honest unified diff against an
    //   empty baseline). new = input.content.
    // - Edit: old = previous file contents (best effort: read from disk; if
    //   the response carries file_content we can derive old by reversing
    //   new_string→old_string, but reading disk is simpler and matches what
    //   `before_hash` will become in PR 2). new = apply edit to old.
    // - MultiEdit: same as Edit, applying input.edits in order.
    //
    // The PostToolUse hook runs after the write has happened, so disk reflects
    // the *new* state, not the old. For Edit/MultiEdit we therefore prefer
    // response.file_content (the tool's post-image) as the new content when
    // present, and reconstruct old by reversing the edit chain from new.
    // When that's not feasible (e.g. response.file_content missing), we
    // fall back to applying edits forward from old_string and accept that
    // the "old" half of the diff is just the matched substring.
    let (old_content, new_content) = match tool_name {
        "Write" => {
            let new = input
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            (String::new(), new)
        }
        "Edit" => {
            let old_string = input
                .get("old_string")
                .and_then(Value::as_str)
                .unwrap_or("");
            let new_string = input
                .get("new_string")
                .and_then(Value::as_str)
                .unwrap_or("");
            // Try response.file_content first — that's the authoritative
            // post-image. Reverse the edit to recover the pre-image.
            if let Some(post) = response.get("file_content").and_then(Value::as_str) {
                let pre = post.replacen(new_string, old_string, 1);
                (pre, post.to_string())
            } else {
                // Fall back to reading disk; the post-tool state is on disk now.
                let post = std::fs::read_to_string(&path).unwrap_or_default();
                if !post.is_empty() {
                    let pre = post.replacen(new_string, old_string, 1);
                    (pre, post)
                } else {
                    // Last resort: just show the substring-level edit. This is
                    // the same shape as v0 already produced for Edit, just in
                    // unified-diff form instead of "- old / + new".
                    (old_string.to_string(), new_string.to_string())
                }
            }
        }
        "MultiEdit" => {
            // Apply input.edits in order. Each edit is {old_string, new_string,
            // replace_all?}. We compute new = post-image (from response or
            // disk if present), and old = repeatedly reverse-apply in reverse
            // order. Fallback when post-image is missing: build old by
            // concatenating original substrings, build new by applying.
            let edits = input
                .get("edits")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let post_image = response
                .get("file_content")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| std::fs::read_to_string(&path).ok());

            if let Some(post) = post_image {
                // Reverse each edit (in reverse order) to recover the pre-image.
                let mut pre = post.clone();
                for edit in edits.iter().rev() {
                    let old_s = edit.get("old_string").and_then(Value::as_str).unwrap_or("");
                    let new_s = edit.get("new_string").and_then(Value::as_str).unwrap_or("");
                    let replace_all = edit
                        .get("replace_all")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    pre = if replace_all {
                        pre.replace(new_s, old_s)
                    } else {
                        pre.replacen(new_s, old_s, 1)
                    };
                }
                (pre, post)
            } else {
                // Synthesize a virtual file: each edit's old_string concatenated
                // for "old" and new_string concatenated for "new". Imperfect
                // but produces a meaningful unified diff.
                let mut old = String::new();
                let mut new = String::new();
                for edit in &edits {
                    let old_s = edit.get("old_string").and_then(Value::as_str).unwrap_or("");
                    let new_s = edit.get("new_string").and_then(Value::as_str).unwrap_or("");
                    if !old.is_empty() {
                        old.push('\n');
                    }
                    if !new.is_empty() {
                        new.push('\n');
                    }
                    old.push_str(old_s);
                    new.push_str(new_s);
                }
                (old, new)
            }
        }
        _ => (String::new(), String::new()),
    };

    // after_hash is always emitted now — we always have a `new_content` string
    // (possibly empty if input was malformed). The v0 hash format is
    // `blake3:<hex>`; hashing an empty string gives a valid hex digest.
    let after_hash = hash_str(&new_content);
    let diff = unified_diff(&path, &old_content, &new_content);

    // TODO(#9 PR 2): populate `before_hash` via PreToolUse hook. For now
    // emit null to match v0 spec's optional-field convention.
    let payload = serde_json::json!({
        "path": path,
        "before_hash": serde_json::Value::Null,
        "after_hash": after_hash,
        "diff": diff,
    });
    serde_json::json!({
        "kind": "file_write",
        "payload": payload
    })
}

fn unified_diff(path: &str, old: &str, new: &str) -> String {
    TextDiff::from_lines(old, new)
        .unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

fn hash_str(s: &str) -> String {
    hash_bytes(s.as_bytes())
}

fn hash_bytes(b: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(b).to_hex())
}

fn post_event(socket_path: &str, event: &Value) -> std::io::Result<()> {
    use std::io::Write;
    let mut stream = UnixStream::connect(socket_path)?;
    let line = event.to_string();
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(())
}
