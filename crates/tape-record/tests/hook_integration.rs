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
