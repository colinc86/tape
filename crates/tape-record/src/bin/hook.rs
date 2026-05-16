//! `tape-hook` — small CLI invoked from a Claude Code `PostToolUse` /
//! `PreToolUse` hook. Reads the hook event JSON on stdin, translates it into
//! a `tape/v0` track event (`shell`, `file_read`, or `file_write`), and posts
//! it to the recorder Unix socket configured via `TAPE_RECORDER_SOCKET`.
//!
//! For `PreToolUse` on `Write|Edit|MultiEdit`, the hook does *not* post a
//! track event — it stream-hashes the file's current bytes and buffers the
//! result to a temp file keyed by `tool_use_id` under `$TAPE_BEFORE_DIR`.
//! The matching `PostToolUse` hook reads that buffer back to populate
//! `file_write.before_hash` (SPEC §5.5.6).
//!
//! Exits 0 on success and 0 on transient failures too — a hook that returns
//! non-zero blocks Claude Code's tool flow, which we never want to do for a
//! recording side-channel. Failures are emitted on stderr for diagnostics.

use std::io::{BufReader, Read};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde_json::Value;
use similar::TextDiff;

/// Buffer size for streaming file reads. 64 KiB is a common sweet spot:
/// small enough to keep RSS bounded for arbitrarily large files, large
/// enough that syscall overhead is amortized.
const HASH_CHUNK: usize = 64 * 1024;

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
    //
    // The list MUST stay in sync with the PostToolUse dispatch below and
    // with the overlay matchers in `crates/tape-record/src/overlay.rs`.
    // (Issue #83: NotebookEdit slipped past PreToolUse for the same shape
    // of reason as #75 missed it from the overlay.)
    if is_pre && matches!(tool_name, "Write" | "Edit" | "MultiEdit" | "NotebookEdit") {
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
        // Keep this list in sync with the PreToolUse dispatch above and the
        // overlay matchers in `crates/tape-record/src/overlay.rs`. (Issue #83.)
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => Some(file_write_event(
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
        .map(hash_str)
        .or_else(|| {
            if path.is_empty() {
                None
            } else {
                // Stream-hash the file from disk so multi-GB inputs don't
                // pin the whole content in RSS — the hook runs synchronously
                // inside Claude Code's tool flow.
                hash_file(Path::new(&path)).ok()
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

/// `PreToolUse` handler for Write/Edit/MultiEdit: stream-hash the current
/// bytes of the target file and buffer the result so the matching
/// `PostToolUse` hook can emit it as `before_hash`.
///
/// Buffer encoding (one file per tool invocation, keyed by `tool_use_id`):
/// - File contains the literal string `null` if the target file did not
///   exist before (ENOENT), per SPEC §5.5.6's "null iff file did not exist
///   before".
/// - Otherwise the file contains the `blake3:<hex>` hash string.
///
/// Streaming via `hash_file` (the same helper the Read fallback uses,
/// added by #43) means a multi-GB pre-image is hashed without sitting in
/// RSS — the hook runs synchronously inside Claude Code's tool flow.
fn handle_file_write_pre(input: &Value, tool_use_id: Option<&str>) {
    let path = input.get("file_path").and_then(Value::as_str).unwrap_or("");
    if path.is_empty() {
        return;
    }

    let before_hash: Option<String> = match hash_file(Path::new(path)) {
        Ok(h) => Some(h),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!("tape-hook: PreToolUse hash {path} failed: {e}");
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
        eprintln!("tape-hook: create before-dir {}: {e}", dir.display());
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

/// Resolve the per-recording before-hash buffer directory. Falls back to a
/// shared `$TMPDIR/tape-before-hashes` if `TAPE_BEFORE_DIR` isn't set (e.g.
/// the hook was invoked outside `tape record`), so the file flow still
/// works end-to-end in dev/test scenarios.
fn before_dir() -> PathBuf {
    if let Ok(p) = std::env::var("TAPE_BEFORE_DIR") {
        return PathBuf::from(p);
    }
    std::env::temp_dir().join("tape-before-hashes")
}

#[allow(clippy::too_many_lines)] // four cases × stream-hash plumbing — each branch is small
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

    // Reconstruct (old_content, new_content, after_hash) for this write:
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
    //
    // When the post-image comes from disk we stream-hash it during the read
    // so the file's bytes only flow through memory once (#43); otherwise we
    // hash the in-memory string we already have. Hash output is identical.
    let (old_content, new_content, after_hash) = match tool_name {
        "Write" => {
            let new = input
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let hash = hash_str(&new);
            (String::new(), new, hash)
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
                let hash = hash_str(post);
                (pre, post.to_string(), hash)
            } else {
                // Fall back to reading disk; the post-tool state is on disk now.
                // Stream-read into the diff input and the blake3 hasher in one
                // pass so memory stays bounded even on multi-GB files (#43).
                match read_and_hash_file(Path::new(&path)) {
                    Ok((post, hash)) if !post.is_empty() => {
                        let pre = post.replacen(new_string, old_string, 1);
                        (pre, post, hash)
                    }
                    _ => {
                        // Last resort: just show the substring-level edit. This is
                        // the same shape as v0 already produced for Edit, just in
                        // unified-diff form instead of "- old / + new".
                        let new = new_string.to_string();
                        let hash = hash_str(&new);
                        (old_string.to_string(), new, hash)
                    }
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
            // Stream-hash the disk fallback in the same pass as the read so
            // a multi-GB post-image never sits in RSS twice (#43).
            let post_with_hash: Option<(String, String)> = response
                .get("file_content")
                .and_then(Value::as_str)
                .map(|c| {
                    let h = hash_str(c);
                    (c.to_owned(), h)
                })
                .or_else(|| read_and_hash_file(Path::new(&path)).ok());

            if let Some((post, hash)) = post_with_hash {
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
                (pre, post, hash)
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
                let hash = hash_str(&new);
                (old, new, hash)
            }
        }
        _ => {
            let empty = String::new();
            let hash = hash_str(&empty);
            (String::new(), empty, hash)
        }
    };

    // after_hash is always emitted now — we always have a `new_content` string
    // (possibly empty if input was malformed). The v0 hash format is
    // `blake3:<hex>`; hashing an empty string gives a valid hex digest.
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

/// Stream `path` through a blake3 hasher in fixed-size chunks. Used by the
/// Read fallback where the file's contents aren't needed beyond the hash
/// (#43) — memory stays bounded at `HASH_CHUNK` bytes regardless of file
/// size. Output is byte-identical to `hash_bytes(&fs::read(path)?)`.
fn hash_file(path: &Path) -> std::io::Result<String> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(HASH_CHUNK, file);
    let mut hasher = blake3::Hasher::new();
    // Heap-allocated to keep the stack frame small; the hook process is
    // short-lived but the OS thread stack default isn't generous.
    let mut buf = vec![0u8; HASH_CHUNK];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

/// Stream `path` into a `String` *and* a blake3 hasher in a single pass
/// (#43). Used by the Edit/MultiEdit disk fallback where we need both the
/// post-image content (to feed the unified-diff helper) and its hash. The
/// returned string matches what `fs::read_to_string(path)?` would produce
/// and the hash matches `hash_str` of that string. Non-UTF-8 bytes cause
/// `InvalidData` just like `read_to_string`.
///
/// Reads through a raw `Vec<u8>` so multi-byte UTF-8 codepoints that
/// straddle chunk boundaries don't get rejected — `from_utf8` runs once
/// on the whole buffer, identical to `fs::read_to_string`'s contract. The
/// blake3 hasher still updates incrementally per chunk.
fn read_and_hash_file(path: &Path) -> std::io::Result<(String, String)> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(HASH_CHUNK, file);
    let mut hasher = blake3::Hasher::new();
    let mut bytes: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; HASH_CHUNK];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        bytes.extend_from_slice(&buf[..n]);
    }
    let content = String::from_utf8(bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok((content, format!("blake3:{}", hasher.finalize().to_hex())))
}

fn post_event(socket_path: &str, event: &Value) -> std::io::Result<()> {
    use std::io::Write;
    let mut stream = UnixStream::connect(socket_path)?;
    let line = event.to_string();
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(())
}
