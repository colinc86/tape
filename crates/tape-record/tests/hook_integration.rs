//! Simulates Claude Code hook firings against the recorder Unix socket via
//! the `tape-hook` binary, and asserts the corresponding shell/file_read/
//! file_write events land in the session.

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
    let shell_events: Vec<_> = snap.tracks.iter().filter(|t| t.kind == Kind::Shell).collect();
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
    let reads: Vec<_> = snap.tracks.iter().filter(|t| t.kind == Kind::FileRead).collect();
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].payload["path"], "/etc/hosts");
    assert!(
        reads[0].payload["content_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );

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
    let writes: Vec<_> = snap.tracks.iter().filter(|t| t.kind == Kind::FileWrite).collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].payload["path"], "/tmp/x.txt");
    assert!(
        writes[0].payload["after_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );

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
    let writes: Vec<_> = snap.tracks.iter().filter(|t| t.kind == Kind::FileWrite).collect();
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
    let writes: Vec<_> = snap.tracks.iter().filter(|t| t.kind == Kind::FileWrite).collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    assert!(payload["before_hash"].is_null(), "PR 1 leaves before_hash null");
    let after = payload["after_hash"].as_str().expect("after_hash always present");
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
    let writes: Vec<_> = snap.tracks.iter().filter(|t| t.kind == Kind::FileWrite).collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    let after = payload["after_hash"].as_str().expect("after_hash always present");
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
    let writes: Vec<_> = snap.tracks.iter().filter(|t| t.kind == Kind::FileWrite).collect();
    assert_eq!(writes.len(), 1);
    let payload = &writes[0].payload;
    let after = payload["after_hash"].as_str().expect("after_hash always present");
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
    let reads: Vec<_> = snap.tracks.iter().filter(|t| t.kind == Kind::FileRead).collect();
    assert_eq!(reads.len(), 1);
    let payload = &reads[0].payload;
    let h = payload["content_hash"].as_str().expect("content_hash always present");
    let expected = format!("blake3:{}", blake3::hash(b"hello\n").to_hex());
    assert_eq!(h, expected);

    handle.shutdown().await;
}
