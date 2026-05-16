//! Simulates Claude Code hook firings against the recorder Unix socket via
//! the `tape-hook` binary, and asserts the corresponding `shell`/`file_read`/
//! `file_write` events land in the session.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use tape_format::tracks::Kind;
use tape_record::session::Session;
use tape_record::socket;

fn hook_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape-hook"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bash_hook_records_shell_event() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("rec.sock");
    let session = Session::start("hook test", "test/0.0.1");
    let handle = socket::spawn(sock.clone(), session.clone()).await.unwrap();

    let event = serde_json::json!({
        "session_id": "test",
        "tool_name": "Bash",
        "tool_input": {"command": "ls /tmp"},
        "tool_response": {"exit_code": 0, "stdout": "foo\nbar\n", "stderr": "", "duration_ms": 12}
    });

    let mut child = Command::new(hook_bin())
        .env("TAPE_RECORDER_SOCKET", &sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let status = child.wait().unwrap();
    assert!(status.success());

    tokio::time::sleep(Duration::from_millis(80)).await;
    let snap = session.snapshot();
    let shell_events: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::Shell)
        .collect();
    assert_eq!(shell_events.len(), 1, "expected one shell event");
    let payload = &shell_events[0].payload;
    assert_eq!(payload["command"], "ls /tmp");
    assert_eq!(payload["exit_code"], 0);
    assert_eq!(payload["stdout"], "foo\nbar\n");

    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_hook_records_file_read_event() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("rec.sock");
    let session = Session::start("hook test", "test/0.0.1");
    let handle = socket::spawn(sock.clone(), session.clone()).await.unwrap();

    let event = serde_json::json!({
        "tool_name": "Read",
        "tool_input": {"file_path": "/etc/hosts"},
        "tool_response": {"file_content": "127.0.0.1 localhost\n"}
    });

    let mut child = Command::new(hook_bin())
        .env("TAPE_RECORDER_SOCKET", &sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let _ = child.wait().unwrap();

    tokio::time::sleep(Duration::from_millis(80)).await;
    let snap = session.snapshot();
    let reads: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileRead)
        .collect();
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].payload["path"], "/etc/hosts");
    assert!(reads[0].payload["content_hash"]
        .as_str()
        .unwrap()
        .starts_with("blake3:"));

    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_hook_records_file_write_event() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("rec.sock");
    let session = Session::start("hook test", "test/0.0.1");
    let handle = socket::spawn(sock.clone(), session.clone()).await.unwrap();

    let event = serde_json::json!({
        "tool_name": "Write",
        "tool_input": {"file_path": "/tmp/x.txt", "content": "hello"},
        "tool_response": {}
    });

    let mut child = Command::new(hook_bin())
        .env("TAPE_RECORDER_SOCKET", &sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let _ = child.wait().unwrap();

    tokio::time::sleep(Duration::from_millis(80)).await;
    let snap = session.snapshot();
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].payload["path"], "/tmp/x.txt");
    assert!(writes[0].payload["after_hash"]
        .as_str()
        .unwrap()
        .starts_with("blake3:"));

    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_hook_records_diff() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("rec.sock");
    let session = Session::start("hook test", "test/0.0.1");
    let handle = socket::spawn(sock.clone(), session.clone()).await.unwrap();

    let event = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": "/tmp/x.txt",
            "old_string": "foo",
            "new_string": "bar"
        },
        "tool_response": {"file_content": "bar"}
    });

    let mut child = Command::new(hook_bin())
        .env("TAPE_RECORDER_SOCKET", &sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let _ = child.wait().unwrap();

    tokio::time::sleep(Duration::from_millis(80)).await;
    let snap = session.snapshot();
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    assert!(payload["diff"].as_str().unwrap().contains("foo"));
    assert!(payload["diff"].as_str().unwrap().contains("bar"));

    handle.shutdown().await;
}

// Helper: run the tape-hook binary with a hook event, wait briefly, return
// the session snapshot.
async fn run_hook_and_snapshot(
    event: serde_json::Value,
) -> (
    tape_record::session::SessionSnapshot,
    tape_record::socket::SocketHandle,
    tape_record::session::Session,
) {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("rec.sock");
    let session = Session::start("hook test", "test/0.0.1");
    let handle = socket::spawn(sock.clone(), session.clone()).await.unwrap();

    let mut child = Command::new(hook_bin())
        .env("TAPE_RECORDER_SOCKET", &sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let _ = child.wait().unwrap();

    tokio::time::sleep(Duration::from_millis(80)).await;
    // Leak `dir` into the returned handle's lifetime by dropping it here;
    // the socket is already bound to the FD and no longer needs the path.
    let snap = session.snapshot();
    (snap, handle, session)
}

fn assert_unified_diff_shape(diff: &str) {
    assert!(
        diff.lines().any(|l| l.starts_with("---")),
        "expected `---` header line in unified diff, got:\n{diff}"
    );
    assert!(
        diff.lines().any(|l| l.starts_with("+++")),
        "expected `+++` header line in unified diff, got:\n{diff}"
    );
    assert!(
        diff.lines().any(|l| l.starts_with("@@")),
        "expected at least one `@@` hunk header in unified diff, got:\n{diff}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_hook_emits_unified_diff_and_after_hash() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("new.txt");

    let event = serde_json::json!({
        "tool_name": "Write",
        "tool_input": {"file_path": file_path.to_str().unwrap(), "content": "alpha\nbeta\n"},
        "tool_response": {}
    });

    let (snap, handle, _session) = run_hook_and_snapshot(event).await;
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    assert!(
        payload["before_hash"].is_null(),
        "PR 1 leaves before_hash null"
    );
    let after = payload["after_hash"]
        .as_str()
        .expect("after_hash always present");
    assert!(after.starts_with("blake3:") && after.len() == "blake3:".len() + 64);
    let diff = payload["diff"].as_str().expect("diff always present");
    assert_unified_diff_shape(diff);
    assert!(diff.contains("+alpha"));
    assert!(diff.contains("+beta"));

    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_hook_emits_unified_diff_with_post_image() {
    // Edit with response.file_content set: pre-image recovered via reverse-apply.
    let event = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": "/tmp/x.txt",
            "old_string": "foo\nbar\n",
            "new_string": "foo\nBAZ\n"
        },
        "tool_response": {"file_content": "prefix\nfoo\nBAZ\nsuffix\n"}
    });

    let (snap, handle, _session) = run_hook_and_snapshot(event).await;
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    let after = payload["after_hash"]
        .as_str()
        .expect("after_hash always present");
    assert!(after.starts_with("blake3:"));
    // Hash should match blake3 of the post-image we declared.
    let expected = format!(
        "blake3:{}",
        blake3::hash(b"prefix\nfoo\nBAZ\nsuffix\n").to_hex()
    );
    assert_eq!(after, expected);

    let diff = payload["diff"].as_str().expect("diff always present");
    assert_unified_diff_shape(diff);
    assert!(diff.contains("-bar"));
    assert!(diff.contains("+BAZ"));

    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiedit_hook_emits_unified_diff() {
    let event = serde_json::json!({
        "tool_name": "MultiEdit",
        "tool_input": {
            "file_path": "/tmp/x.txt",
            "edits": [
                {"old_string": "alpha", "new_string": "ALPHA"},
                {"old_string": "gamma", "new_string": "GAMMA"}
            ]
        },
        "tool_response": {"file_content": "ALPHA\nbeta\nGAMMA\n"}
    });

    let (snap, handle, _session) = run_hook_and_snapshot(event).await;
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    let after = payload["after_hash"]
        .as_str()
        .expect("after_hash always present");
    assert_eq!(
        after,
        format!("blake3:{}", blake3::hash(b"ALPHA\nbeta\nGAMMA\n").to_hex())
    );
    let diff = payload["diff"].as_str().expect("diff always present");
    assert_unified_diff_shape(diff);
    assert!(diff.contains("-alpha"));
    assert!(diff.contains("+ALPHA"));
    assert!(diff.contains("-gamma"));
    assert!(diff.contains("+GAMMA"));

    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_hook_hashes_from_disk_when_response_omits_content() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("hello.txt");
    std::fs::write(&file_path, b"hello\n").unwrap();

    let event = serde_json::json!({
        "tool_name": "Read",
        "tool_input": {"file_path": file_path.to_str().unwrap()},
        "tool_response": {}
    });

    let (snap, handle, _session) = run_hook_and_snapshot(event).await;
    let reads: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileRead)
        .collect();
    assert_eq!(reads.len(), 1);
    let payload = &reads[0].payload;
    let h = payload["content_hash"]
        .as_str()
        .expect("content_hash always present");
    let expected = format!("blake3:{}", blake3::hash(b"hello\n").to_hex());
    assert_eq!(h, expected);

    handle.shutdown().await;
}

/// #43 regression — the Read fallback must stream-hash so a multi-MiB file
/// doesn't get slurped into RAM. We can't observe RSS reliably on CI, but
/// we can prove the streaming hasher produces a hash identical to a
/// one-shot `blake3::hash` over the same bytes. A 1 MiB sparse zero file
/// (created via `File::set_len`) costs no real disk and forces the read
/// loop to cross several `HASH_CHUNK` (64 KiB) boundaries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_hook_streaming_hash_matches_one_shot_on_large_sparse_file() {
    const SIZE: usize = 1024 * 1024; // 1 MiB — ~16 chunks at 64 KiB
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("sparse.bin");
    let f = std::fs::File::create(&file_path).unwrap();
    f.set_len(SIZE as u64).unwrap();
    drop(f);

    let event = serde_json::json!({
        "tool_name": "Read",
        "tool_input": {"file_path": file_path.to_str().unwrap()},
        "tool_response": {}
    });

    let (snap, handle, _session) = run_hook_and_snapshot(event).await;
    let reads: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileRead)
        .collect();
    assert_eq!(reads.len(), 1);
    let h = reads[0].payload["content_hash"]
        .as_str()
        .expect("content_hash always present");

    // The expected hash is blake3 of SIZE all-zero bytes (sparse-file
    // semantics on every UNIX filesystem of interest). Build it via a
    // one-shot `blake3::hash` so the test independently exercises the
    // streaming-vs-one-shot equivalence.
    let zeros = vec![0u8; SIZE];
    let expected = format!("blake3:{}", blake3::hash(&zeros).to_hex());
    assert_eq!(h, expected, "streaming hash must equal one-shot hash");

    handle.shutdown().await;
}

/// #43 regression — Edit fallback (no `file_content` in response) must
/// stream-read the post-image into both the diff input and the blake3
/// hasher in a single pass. Verify that the emitted `after_hash` and
/// `diff` match what the prior buffer-everything path produced for a
/// small file: hash equals `blake3(post-image bytes)`, and the diff
/// header/hunk reflect the substring replacement.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_hook_disk_fallback_stream_hashes_and_diffs() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("e.txt");
    let post = "prefix\nFOO\nsuffix\n";
    std::fs::write(&file_path, post.as_bytes()).unwrap();

    // No `file_content` in the response → forces the disk fallback path.
    let event = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": file_path.to_str().unwrap(),
            "old_string": "BAR",
            "new_string": "FOO"
        },
        "tool_response": {}
    });

    let (snap, handle, _session) = run_hook_and_snapshot(event).await;
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;

    let after = payload["after_hash"]
        .as_str()
        .expect("after_hash always present");
    let expected = format!("blake3:{}", blake3::hash(post.as_bytes()).to_hex());
    assert_eq!(
        after, expected,
        "stream-hash must match one-shot hash of post-image"
    );

    let diff = payload["diff"].as_str().expect("diff always present");
    assert_unified_diff_shape(diff);
    assert!(
        diff.contains("-BAR"),
        "diff missing pre-image substring: {diff}"
    );
    assert!(
        diff.contains("+FOO"),
        "diff missing post-image substring: {diff}"
    );

    handle.shutdown().await;
}

/// #43 regression — `MultiEdit` fallback (no `file_content` in response)
/// must also stream the post-image through both the hasher and the
/// reverse-apply pre-image reconstruction. Confirms hash byte-identity
/// against a one-shot blake3 on a small file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiedit_hook_disk_fallback_stream_hashes_and_diffs() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("m.txt");
    let post = "ALPHA\nbeta\nGAMMA\n";
    std::fs::write(&file_path, post.as_bytes()).unwrap();

    let event = serde_json::json!({
        "tool_name": "MultiEdit",
        "tool_input": {
            "file_path": file_path.to_str().unwrap(),
            "edits": [
                {"old_string": "alpha", "new_string": "ALPHA"},
                {"old_string": "gamma", "new_string": "GAMMA"}
            ]
        },
        "tool_response": {}
    });

    let (snap, handle, _session) = run_hook_and_snapshot(event).await;
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;

    let after = payload["after_hash"]
        .as_str()
        .expect("after_hash always present");
    let expected = format!("blake3:{}", blake3::hash(post.as_bytes()).to_hex());
    assert_eq!(
        after, expected,
        "stream-hash must match one-shot hash of post-image"
    );

    let diff = payload["diff"].as_str().expect("diff always present");
    assert_unified_diff_shape(diff);
    assert!(diff.contains("-alpha"));
    assert!(diff.contains("+ALPHA"));
    assert!(diff.contains("-gamma"));
    assert!(diff.contains("+GAMMA"));

    handle.shutdown().await;
}

// --- PR 2 (#9): PreToolUse + PostToolUse before_hash flow -----------------

/// Helper: drive a single hook invocation against the recorder socket,
/// supplying an isolated `TAPE_BEFORE_DIR` so concurrent tests don't see
/// each other's buffered entries.
fn drive_hook(
    sock: &std::path::Path,
    before_dir: &std::path::Path,
    event: &serde_json::Value,
) -> std::process::Output {
    let mut child = Command::new(hook_bin())
        .env("TAPE_RECORDER_SOCKET", sock)
        .env("TAPE_BEFORE_DIR", before_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

/// Test scaffold for the Pre/Post hook flow. Holds the recorder socket and
/// a per-test `TAPE_BEFORE_DIR` so concurrent tests don't collide.
struct HookRig {
    sock: std::path::PathBuf,
    before_dir: std::path::PathBuf,
    handle: tape_record::socket::SocketHandle,
    session: Session,
    _dir: tempfile::TempDir,
}

async fn hook_rig() -> HookRig {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("rec.sock");
    let before_dir = dir.path().join("before");
    std::fs::create_dir_all(&before_dir).unwrap();
    let session = Session::start("hook test", "test/0.0.1");
    let handle = socket::spawn(sock.clone(), session.clone()).await.unwrap();
    HookRig {
        sock,
        before_dir,
        handle,
        session,
        _dir: dir,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_new_file_before_hash_is_null_after_pretooluse_pair() {
    let work = tempfile::tempdir().unwrap();
    let file_path = work.path().join("brand-new.txt");
    let path_s = file_path.to_str().unwrap().to_string();
    // PRECONDITION: file does NOT exist before the PreToolUse hook fires.
    assert!(!file_path.exists());

    let rig = hook_rig().await;

    let pre = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_use_id": "tu_write_new_1",
        "tool_name": "Write",
        "tool_input": {"file_path": path_s, "content": "hi\n"}
    });
    let pre_out = drive_hook(&rig.sock, &rig.before_dir, &pre);
    assert!(pre_out.status.success());

    // Simulate Claude Code actually performing the write between Pre and Post.
    std::fs::write(&file_path, b"hi\n").unwrap();

    let post = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_use_id": "tu_write_new_1",
        "tool_name": "Write",
        "tool_input": {"file_path": path_s, "content": "hi\n"},
        "tool_response": {}
    });
    let post_out = drive_hook(&rig.sock, &rig.before_dir, &post);
    assert!(post_out.status.success());

    tokio::time::sleep(Duration::from_millis(120)).await;
    let snap = rig.session.snapshot();
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    assert!(
        payload["before_hash"].is_null(),
        "new file → before_hash MUST be null (SPEC §5.5.6), got {:?}",
        payload["before_hash"]
    );
    let after = payload["after_hash"].as_str().expect("after_hash present");
    assert!(after.starts_with("blake3:"));

    rig.handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_existing_file_before_and_after_hashes_differ() {
    let work = tempfile::tempdir().unwrap();
    let file_path = work.path().join("exists.txt");
    std::fs::write(&file_path, b"old contents\n").unwrap();
    let path_s = file_path.to_str().unwrap().to_string();

    let rig = hook_rig().await;

    let pre = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_use_id": "tu_write_existing_1",
        "tool_name": "Write",
        "tool_input": {"file_path": path_s, "content": "new contents\n"}
    });
    let pre_out = drive_hook(&rig.sock, &rig.before_dir, &pre);
    assert!(pre_out.status.success());

    // Simulate the Write tool actually mutating the file.
    std::fs::write(&file_path, b"new contents\n").unwrap();

    let post = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_use_id": "tu_write_existing_1",
        "tool_name": "Write",
        "tool_input": {"file_path": path_s, "content": "new contents\n"},
        "tool_response": {}
    });
    let post_out = drive_hook(&rig.sock, &rig.before_dir, &post);
    assert!(post_out.status.success());

    tokio::time::sleep(Duration::from_millis(120)).await;
    let snap = rig.session.snapshot();
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    let before = payload["before_hash"]
        .as_str()
        .expect("before_hash present for existing file");
    let after = payload["after_hash"].as_str().expect("after_hash present");
    assert_eq!(
        before,
        format!("blake3:{}", blake3::hash(b"old contents\n").to_hex())
    );
    assert_eq!(
        after,
        format!("blake3:{}", blake3::hash(b"new contents\n").to_hex())
    );
    assert_ne!(before, after);

    rig.handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_existing_file_before_after_differ_and_diff_is_unified() {
    let work = tempfile::tempdir().unwrap();
    let file_path = work.path().join("edit.txt");
    std::fs::write(&file_path, b"prefix\nfoo\nsuffix\n").unwrap();
    let path_s = file_path.to_str().unwrap().to_string();

    let rig = hook_rig().await;

    let pre = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_use_id": "tu_edit_1",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": path_s,
            "old_string": "foo",
            "new_string": "BAR"
        }
    });
    let pre_out = drive_hook(&rig.sock, &rig.before_dir, &pre);
    assert!(pre_out.status.success());

    // Simulate the Edit tool's mutation.
    std::fs::write(&file_path, b"prefix\nBAR\nsuffix\n").unwrap();

    let post = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_use_id": "tu_edit_1",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": path_s,
            "old_string": "foo",
            "new_string": "BAR"
        },
        "tool_response": {"file_content": "prefix\nBAR\nsuffix\n"}
    });
    let post_out = drive_hook(&rig.sock, &rig.before_dir, &post);
    assert!(post_out.status.success());

    tokio::time::sleep(Duration::from_millis(120)).await;
    let snap = rig.session.snapshot();
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    let before = payload["before_hash"]
        .as_str()
        .expect("before_hash present");
    let after = payload["after_hash"].as_str().expect("after_hash present");
    assert_eq!(
        before,
        format!("blake3:{}", blake3::hash(b"prefix\nfoo\nsuffix\n").to_hex())
    );
    assert_eq!(
        after,
        format!("blake3:{}", blake3::hash(b"prefix\nBAR\nsuffix\n").to_hex())
    );
    assert_ne!(before, after);
    let diff = payload["diff"].as_str().expect("diff present");
    assert_unified_diff_shape(diff);
    assert!(diff.contains("-foo"));
    assert!(diff.contains("+BAR"));

    rig.handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiedit_existing_file_before_after_differ_and_diff_is_unified() {
    let work = tempfile::tempdir().unwrap();
    let file_path = work.path().join("multi.txt");
    std::fs::write(&file_path, b"alpha\nbeta\ngamma\n").unwrap();
    let path_s = file_path.to_str().unwrap().to_string();

    let rig = hook_rig().await;

    let pre = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_use_id": "tu_multi_1",
        "tool_name": "MultiEdit",
        "tool_input": {
            "file_path": path_s,
            "edits": [
                {"old_string": "alpha", "new_string": "ALPHA"},
                {"old_string": "gamma", "new_string": "GAMMA"}
            ]
        }
    });
    let pre_out = drive_hook(&rig.sock, &rig.before_dir, &pre);
    assert!(pre_out.status.success());

    std::fs::write(&file_path, b"ALPHA\nbeta\nGAMMA\n").unwrap();

    let post = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_use_id": "tu_multi_1",
        "tool_name": "MultiEdit",
        "tool_input": {
            "file_path": path_s,
            "edits": [
                {"old_string": "alpha", "new_string": "ALPHA"},
                {"old_string": "gamma", "new_string": "GAMMA"}
            ]
        },
        "tool_response": {"file_content": "ALPHA\nbeta\nGAMMA\n"}
    });
    let post_out = drive_hook(&rig.sock, &rig.before_dir, &post);
    assert!(post_out.status.success());

    tokio::time::sleep(Duration::from_millis(120)).await;
    let snap = rig.session.snapshot();
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    let before = payload["before_hash"]
        .as_str()
        .expect("before_hash present");
    let after = payload["after_hash"].as_str().expect("after_hash present");
    assert_eq!(
        before,
        format!("blake3:{}", blake3::hash(b"alpha\nbeta\ngamma\n").to_hex())
    );
    assert_eq!(
        after,
        format!("blake3:{}", blake3::hash(b"ALPHA\nbeta\nGAMMA\n").to_hex())
    );
    assert_ne!(before, after);
    let diff = payload["diff"].as_str().expect("diff present");
    assert_unified_diff_shape(diff);
    assert!(diff.contains("-alpha"));
    assert!(diff.contains("+ALPHA"));
    assert!(diff.contains("-gamma"));
    assert!(diff.contains("+GAMMA"));

    rig.handle.shutdown().await;
}

/// Issue #83: `NotebookEdit` was missing from both the `PreToolUse` and
/// `PostToolUse` dispatch lists in hook.rs. `PreToolUse` fell through to the
/// `return` on line 73 (no before-hash buffered), and `PostToolUse` fell
/// through to `None` (no `file_write` event posted at all). After the fix
/// both branches accept `NotebookEdit`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn notebook_edit_pre_and_post_dispatch_produce_event_with_before_hash() {
    let work = tempfile::tempdir().unwrap();
    let nb_path = work.path().join("foo.ipynb");
    std::fs::write(&nb_path, "{\"cells\": []}\n").unwrap();
    let path_s = nb_path.to_str().unwrap().to_string();

    let rig = hook_rig().await;

    // PreToolUse: should buffer the pre-edit hash.
    let pre = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_use_id": "tu_notebook_pre",
        "tool_name": "NotebookEdit",
        "tool_input": {"file_path": path_s, "new_source": "x"},
        "tool_response": {}
    });
    let out = drive_hook(&rig.sock, &rig.before_dir, &pre);
    assert!(out.status.success(), "NotebookEdit PreToolUse should run");

    // Mutate the file so the PostToolUse "after" content differs.
    std::fs::write(&nb_path, "{\"cells\": [\"new\"]}\n").unwrap();

    // PostToolUse: should dispatch to file_write_event and emit an event
    // whose before_hash is the pre-edit hash, not null.
    let post = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_use_id": "tu_notebook_pre",
        "tool_name": "NotebookEdit",
        "tool_input": {"file_path": path_s, "new_source": "x"},
        "tool_response": {"file_content": "{\"cells\": [\"new\"]}\n"}
    });
    let out = drive_hook(&rig.sock, &rig.before_dir, &post);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("no buffered before_hash"),
        "NotebookEdit before-hash should NOT fall back to null; got stderr:\n{stderr}"
    );

    tokio::time::sleep(Duration::from_millis(80)).await;
    let snap = rig.session.snapshot();
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(
        writes.len(),
        1,
        "expected one file_write event from NotebookEdit PostToolUse; got {}",
        writes.len()
    );
    let payload = &writes[0].payload;
    assert_eq!(payload["path"], path_s);
    let before = payload["before_hash"].as_str().unwrap_or("");
    assert!(
        before.starts_with("blake3:"),
        "before_hash should be a real blake3 hex (not null); got {before:?}"
    );
    assert!(
        payload["after_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:"),
        "after_hash still populated"
    );

    rig.handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn posttooluse_only_falls_back_to_null_before_hash_with_stderr_warning() {
    let work = tempfile::tempdir().unwrap();
    let file_path = work.path().join("only-post.txt");
    let path_s = file_path.to_str().unwrap().to_string();

    let rig = hook_rig().await;

    // No PreToolUse ran. Drive only the PostToolUse.
    let post = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_use_id": "tu_orphan_post",
        "tool_name": "Write",
        "tool_input": {"file_path": path_s, "content": "x\n"},
        "tool_response": {}
    });
    let out = drive_hook(&rig.sock, &rig.before_dir, &post);
    assert!(
        out.status.success(),
        "PostToolUse hook should not crash without a preceding PreToolUse"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no buffered before_hash"),
        "expected diagnostic warning on stderr, got: {stderr}"
    );

    tokio::time::sleep(Duration::from_millis(80)).await;
    let snap = rig.session.snapshot();
    let writes: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::FileWrite)
        .collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    assert!(
        payload["before_hash"].is_null(),
        "orphan PostToolUse → before_hash falls back to null"
    );
    assert!(
        payload["after_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:"),
        "after_hash still populated"
    );

    rig.handle.shutdown().await;
}
