//! End-to-end smoke test for `tape record`.
//!
//! Stands up a mock Anthropic-shaped upstream, then invokes the built `tape`
//! binary in `record` mode with a child that hits the proxy via curl. After
//! the child exits, asserts the produced `.tape` is valid and contains the
//! expected events.

use std::process::Command;
use std::time::Duration;

use axum::{
    body::Body,
    http::{header, StatusCode},
    response::Response,
    routing::post,
    Router,
};

async fn mock_messages(_body: axum::body::Bytes) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "id": "msg_smoke",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "ok"}]
            }))
            .unwrap(),
        ))
        .unwrap()
}

fn binary_path() -> std::path::PathBuf {
    // CARGO_BIN_EXE_<name> is set by cargo for integration tests.
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

#[test]
fn record_smoke_produces_valid_tape() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (mock_addr, mock_shutdown) = rt.block_on(async {
        let app: Router = Router::new().route("/v1/messages", post(mock_messages));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });
        (addr, tx)
    });

    if Command::new("curl").arg("--version").output().is_err() {
        eprintln!("SKIP: curl not available");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("smoke.tape");

    // Child command: a shell that uses curl to POST to $ANTHROPIC_BASE_URL.
    // The proxy intercepts, forwards to mock_addr, records the call.
    let upstream = format!("http://{mock_addr}");
    let child_cmd = "curl -sS -X POST $ANTHROPIC_BASE_URL/v1/messages -H 'content-type: application/json' -d '{\"model\":\"claude-opus-4-7\",\"messages\":[{\"role\":\"user\",\"content\":\"x\"}]}' >/dev/null";

    let status = Command::new(binary_path())
        .arg("record")
        .arg("--task")
        .arg("smoke test")
        .arg("--out")
        .arg(&out_path)
        .arg("--upstream-anthropic")
        .arg(&upstream)
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(child_cmd)
        .status()
        .expect("spawn tape record");
    assert!(status.success(), "tape record exited with {status:?}");

    // Verify the produced tape.
    let out = Command::new(binary_path())
        .arg("verify")
        .arg(&out_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "tape verify failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Inspect via tape ls — should have task, model_call, eject.
    let ls = Command::new(binary_path())
        .arg("ls")
        .arg(&out_path)
        .output()
        .unwrap();
    let ls_text = String::from_utf8(ls.stdout).unwrap();
    assert!(ls_text.contains("task"), "expected task in ls output:\n{ls_text}");
    assert!(
        ls_text.contains("model_call"),
        "expected model_call in ls output:\n{ls_text}"
    );
    assert!(ls_text.contains("eject"), "expected eject in ls output:\n{ls_text}");

    rt.block_on(async {
        let _ = mock_shutdown.send(());
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
}
