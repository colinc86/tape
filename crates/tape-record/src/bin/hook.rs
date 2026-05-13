//! `tape-hook` — small CLI invoked from a Claude Code `PostToolUse` /
//! `PreToolUse` hook. Reads the hook event JSON on stdin, translates it into
//! a `tape/v0` track event (`shell`, `file_read`, or `file_write`), and posts
//! it to the recorder Unix socket configured via `TAPE_RECORDER_SOCKET`.
//!
//! For `PreToolUse` on `Write|Edit|MultiEdit`, the hook does *not* post a
//! track event — it hashes the file's current bytes and buffers the result
//! to a temp file keyed by `tool_use_id`. The matching `PostToolUse` hook
//! reads that buffer back to populate `file_write.before_hash`.
//!
//! Exits 0 on success and 0 on transient failures too — a hook that returns
//! non-zero blocks Claude Code's tool flow, which we never want to do for a
//! recording side-channel. Failures are emitted on stderr for diagnostics.

use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

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

    // Claude Code passes the hook event name in `hook_event_name` per the
    // hook input schema. The `TAPE_HOOK_KIND` env var (set in the overlay)
    // is a backup discriminator for cases where Claude Code's payload shape
    // differs from what we expect.
    let hook_event_name = v
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let hook_kind_env = std::env::var("TAPE_HOOK_KIND").unwrap_or_default();
    let is_pre = hook_event_name == "PreToolUse" || hook_kind_env.ends_with("_pre");

    let tool_use_id = v
        .get("tool_use_id")
        .and_then(Value::as_str)
        .map(str::to_owned);

    // PreToolUse on file-mutating tools: buffer the file's current hash so
    // the matching PostToolUse hook can emit it as `before_hash`. No track
    // event is posted from PreToolUse.
    if is_pre && matches!(tool_name, "Write" | "Edit" | "MultiEdit") {
        handle_file_write_pre(&tool_input, tool_use_id.as_deref());
        return;
    }

    // PreToolUse for other tools (e.g. Bash → shell_pre): no event yet.
    if is_pre {
        return;
    }

    let event = match tool_name {
        "Bash" => Some(shell_event(&tool_input, &tool_response)),
        "Read" => Some(file_read_event(&tool_input, &tool_response)),
        "Write" | "Edit" | "MultiEdit" => Some(file_write_event(
            tool_name,
            &tool_input,
            &tool_response,
            tool_use_id.as_deref(),
        )),
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

/// `PreToolUse` handler for Write/Edit/MultiEdit: hash the current bytes of
/// the target file and buffer the result so the matching `PostToolUse` hook
/// can emit it as `before_hash`.
///
/// Buffer encoding:
/// - File contains the literal string `null` if the target file did not
///   exist before (ENOENT), per SPEC §5.5.6's "null iff file did not exist
///   before".
/// - Otherwise the file contains the `blake3:<hex>` hash string.
fn handle_file_write_pre(input: &Value, tool_use_id: Option<&str>) {
    let path = input
        .get("file_path")
        .and_then(Value::as_str)
        .unwrap_or("");
    if path.is_empty() {
        return;
    }

    let before_hash: Option<String> = match std::fs::read(path) {
        Ok(b) => Some(hash_bytes(&b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!("tape-hook: PreToolUse read {path} failed: {e}");
            // Treat unreadable as missing — PostToolUse will surface this
            // as `before_hash: null`.
            None
        }
    };

    let Some(key) = buffer_key(tool_use_id, path) else {
        eprintln!("tape-hook: no buffer key for PreToolUse on {path}; skipping");
        return;
    };
    let dir = before_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "tape-hook: create before-dir {}: {e}",
            dir.display()
        );
        return;
    }
    let buf_path = dir.join(format!("{key}.before"));
    let contents = match &before_hash {
        Some(h) => h.clone(),
        None => "null".to_string(),
    };
    if let Err(e) = std::fs::write(&buf_path, contents.as_bytes()) {
        eprintln!(
            "tape-hook: write before-hash buffer {}: {e}",
            buf_path.display()
        );
    }
}

/// Result of draining a buffered before-hash entry.
enum BufferedBeforeHash {
    /// `PreToolUse` ran and the file existed; here is its hash.
    Hash(String),
    /// `PreToolUse` ran and the file did NOT exist before. SPEC §5.5.6 says
    /// `before_hash` MUST be `null` in this case.
    FileDidNotExist,
    /// No buffer entry was found. The `PreToolUse` hook didn't run for this
    /// tool call (race, environment mismatch, or test-only `PostToolUse`).
    Missing,
}

/// Look up a buffered before-hash and remove the buffer file.
fn drain_before_hash(tool_use_id: Option<&str>, path: &str) -> BufferedBeforeHash {
    let Some(key) = buffer_key(tool_use_id, path) else {
        return BufferedBeforeHash::Missing;
    };
    let buf_path = before_dir().join(format!("{key}.before"));
    let Ok(contents) = std::fs::read_to_string(&buf_path) else {
        return BufferedBeforeHash::Missing;
    };
    // Best-effort cleanup — don't fail the hook if removal fails.
    let _ = std::fs::remove_file(&buf_path);
    let trimmed = contents.trim();
    if trimmed == "null" {
        BufferedBeforeHash::FileDidNotExist
    } else if trimmed.starts_with("blake3:") {
        BufferedBeforeHash::Hash(trimmed.to_string())
    } else {
        // Malformed buffer — treat as no entry so the caller falls back.
        BufferedBeforeHash::Missing
    }
}

/// Stable filename key for the per-tool-use buffer. Prefers `tool_use_id`
/// (uniquely supplied by Claude Code on each tool invocation); falls back
/// to a hash of `path` if absent. The fallback is fine for tests where we
/// drive Pre+Post manually — Claude Code itself always supplies the id.
fn buffer_key(tool_use_id: Option<&str>, path: &str) -> Option<String> {
    if let Some(id) = tool_use_id.filter(|s| !s.is_empty()) {
        // Hash the id to neutralize any awkward filename characters.
        Some(format!("id-{}", blake3::hash(id.as_bytes()).to_hex()))
    } else if !path.is_empty() {
        Some(format!("path-{}", blake3::hash(path.as_bytes()).to_hex()))
    } else {
        None
    }
}

fn before_dir() -> PathBuf {
    if let Ok(p) = std::env::var("TAPE_BEFORE_DIR") {
        return PathBuf::from(p);
    }
    std::env::temp_dir().join("tape-before-hashes")
}

/// Reconstruct `(old_content, new_content)` for a Write/Edit/MultiEdit hook
/// invocation.
///
/// - Write: old = "" (the unified diff is computed against an empty baseline;
///   the buffered `before_hash` carries the authoritative pre-image identity).
///   new = `input.content`.
/// - Edit: old = previous file contents (best effort: prefer reversing
///   `response.file_content`, fall back to reading disk).
/// - `MultiEdit`: same as Edit, applying `input.edits` in order.
///
/// The `PostToolUse` hook runs after the write has happened, so disk reflects
/// the *new* state, not the old. For Edit/MultiEdit we therefore prefer
/// `response.file_content` (the tool's post-image) as the new content when
/// present, and reconstruct old by reversing the edit chain from new.
fn reconstruct_old_new(tool_name: &str, input: &Value, response: &Value, path: &str) -> (String, String) {
    match tool_name {
        "Write" => {
            let new = input
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            (String::new(), new)
        }
        "Edit" => reconstruct_edit(input, response, path),
        "MultiEdit" => reconstruct_multiedit(input, response, path),
        _ => (String::new(), String::new()),
    }
}

fn reconstruct_edit(input: &Value, response: &Value, path: &str) -> (String, String) {
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
        return (pre, post.to_string());
    }
    // Fall back to reading disk; the post-tool state is on disk now.
    let post = std::fs::read_to_string(path).unwrap_or_default();
    if post.is_empty() {
        // Last resort: just show the substring-level edit.
        return (old_string.to_string(), new_string.to_string());
    }
    let pre = post.replacen(new_string, old_string, 1);
    (pre, post)
}

fn reconstruct_multiedit(input: &Value, response: &Value, path: &str) -> (String, String) {
    let edits = input
        .get("edits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let post_image = response
        .get("file_content")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| std::fs::read_to_string(path).ok());

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
        return (pre, post);
    }
    // Synthesize a virtual file: each edit's old_string concatenated for
    // "old" and new_string concatenated for "new". Imperfect but produces a
    // meaningful unified diff.
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

fn file_write_event(
    tool_name: &str,
    input: &Value,
    response: &Value,
    tool_use_id: Option<&str>,
) -> Value {
    let path = input
        .get("file_path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let (old_content, new_content) = reconstruct_old_new(tool_name, input, response, &path);

    // `after_hash` is always emitted — we always have a `new_content` string
    // (possibly empty if input was malformed). The v0 hash format is
    // `blake3:<hex>`; hashing an empty string gives a valid hex digest.
    let after_hash = hash_str(&new_content);
    let diff = unified_diff(&path, &old_content, &new_content);

    // `before_hash`: drain the buffered value the `PreToolUse` hook wrote.
    // - `Hash(h)`          → file existed before, here's its hash
    // - `FileDidNotExist`  → file did NOT exist before; emit JSON null per SPEC §5.5.6
    // - `Missing`          → `PreToolUse` didn't run (race, environment didn't
    //                        propagate, or test driving `PostToolUse` only). Fall
    //                        back to null and warn on stderr so it's diagnosable.
    let before_hash_value = match drain_before_hash(tool_use_id, &path) {
        BufferedBeforeHash::Hash(h) => Value::String(h),
        BufferedBeforeHash::FileDidNotExist => Value::Null,
        BufferedBeforeHash::Missing => {
            eprintln!(
                "tape-hook: no buffered before_hash for {path} (PreToolUse hook missing?); emitting null"
            );
            Value::Null
        }
    };

    let payload = serde_json::json!({
        "path": path,
        "before_hash": before_hash_value,
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
