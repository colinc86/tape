//! Integration test: spawn the recorder Unix socket, spawn `tape-mcp-wrap`
//! pointed at the `mock_mcp_server` example, drive a `tools/call` through
//! its stdin/stdout, and assert the recording session captured an mcp_call
//! event with the expected shape.

use std::process::Stdio;
use std::time::Duration;

use tape_format::tracks::Kind;
use tape_record::session::Session;
use tape_record::socket;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn wrap_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape-mcp-wrap"))
}

fn mock_server_bin() -> std::path::PathBuf {
    // Cargo only sets CARGO_BIN_EXE_<name> for [[bin]] targets, not examples.
    // Examples live at target/<profile>/examples/<name>.
    let wrap = wrap_bin();
    let examples = wrap.parent().unwrap().join("examples");
    examples.join("mock_mcp_server")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrap_records_tools_call() {
    // Build the mock server example before the test runs.
    let status = std::process::Command::new("cargo")
        .args(["build", "--example", "mock_mcp_server", "-p", "tape-mcp-wrap"])
        .status()
        .expect("cargo build example");
    assert!(status.success(), "failed to build mock_mcp_server example");

    let mock = mock_server_bin();
    assert!(
        mock.exists(),
        "mock_mcp_server not found at {}",
        mock.display()
    );

    // Recorder socket.
    let dir = tempfile::tempdir().unwrap();
    let sock_path = dir.path().join("rec.sock");
    let session = Session::start("mcp wrap test", "test/0.0.1");
    let socket_handle = socket::spawn(sock_path.clone(), session.clone()).await.unwrap();

    // Spawn the wrap binary.
    let mut child = tokio::process::Command::new(wrap_bin())
        .env("TAPE_WRAP_CMD", &mock)
        .env("TAPE_WRAP_ARGS_JSON", "[]")
        .env("TAPE_WRAP_SOCKET", &sock_path)
        .env("TAPE_WRAP_SERVER_NAME", "mock-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();

    let mut wrap_stdin = child.stdin.take().unwrap();
    let mut wrap_stdout = BufReader::new(child.stdout.take().unwrap());

    // initialize
    let init = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
    });
    wrap_stdin.write_all(format!("{init}\n").as_bytes()).await.unwrap();
    wrap_stdin.flush().await.unwrap();
    let mut line = String::new();
    wrap_stdout.read_line(&mut line).await.unwrap();
    let init_resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(init_resp["id"], 1);
    assert!(init_resp.get("result").is_some());

    // tools/call
    line.clear();
    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {"name": "echo", "arguments": {"hello": "world"}}
    });
    wrap_stdin.write_all(format!("{call}\n").as_bytes()).await.unwrap();
    wrap_stdin.flush().await.unwrap();
    wrap_stdout.read_line(&mut line).await.unwrap();
    let call_resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(call_resp["id"], 2);
    assert_eq!(call_resp["result"]["ok"], true);

    // Close stdin so the wrap can exit.
    drop(wrap_stdin);
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;

    // Give the recorder socket a moment to drain.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let snap = session.snapshot();
    let mcp_calls: Vec<_> = snap
        .tracks
        .iter()
        .filter(|t| t.kind == Kind::McpCall)
        .collect();
    assert_eq!(mcp_calls.len(), 1, "expected one mcp_call recorded");
    let call = mcp_calls[0];
    assert_eq!(call.payload["server"], "mock-server");
    assert_eq!(call.payload["tool"], "echo");
    assert_eq!(call.payload["args"]["hello"], "world");
    assert_eq!(call.payload["result"]["ok"], true);

    socket_handle.shutdown().await;
}
