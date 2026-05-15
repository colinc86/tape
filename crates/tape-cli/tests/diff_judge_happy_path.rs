//! Happy-path coverage for `tape diff --judge` against an `axum`
//! mock judge server. The mock returns a deterministic narration
//! string; the test asserts the rendered text output contains a
//! `judge:` line under one of the diff entries and that the CLI exits
//! 0. Mirrors the pattern `tape-judge`'s own integration tests use.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

#[derive(Clone)]
struct MockState {
    call_count: Arc<AtomicU32>,
    response: String,
}

async fn handle(State(state): State<MockState>, Json(_body): Json<Value>) -> Json<Value> {
    state.call_count.fetch_add(1, Ordering::SeqCst);
    Json(json!({
        "choices": [{
            "message": { "role": "assistant", "content": state.response }
        }]
    }))
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

#[test]
fn diff_judge_renders_narration_under_substantive_entry() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let (mock_endpoint, shutdown, call_count) = rt.block_on(async {
        let state = MockState {
            call_count: Arc::new(AtomicU32::new(0)),
            response: "MOCK_JUDGE_NARRATION: the agent ran a different shell command this time."
                .into(),
        };
        let counter = state.call_count.clone();
        let app = Router::new()
            .route("/v1/chat/completions", post(handle))
            .with_state(state);
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
        (
            format!("http://{addr}/v1/chat/completions"),
            tx,
            counter,
        )
    });

    // Stage a `.taperc` in a temp dir whose `judge::endpoint` points at
    // the mock. Spawn the CLI with cwd set to that dir so
    // `TapeRcConfig::locate_workspace` finds it and `HOME` set to the
    // same dir so the user-level fallback also resolves there. We use
    // the `MOCK_JUDGE_KEY` env var name so a developer machine's
    // `OPENAI_API_KEY` doesn't get picked up by accident.
    let tmp = tempfile::tempdir().unwrap();
    let taperc = tmp.path().join(".taperc");
    std::fs::write(
        &taperc,
        format!(
            "judge:\n  model: placeholder\n  endpoint: {mock_endpoint}\n  api_key_env: MOCK_JUDGE_KEY\n  max_attempts: 1\n"
        ),
    )
    .unwrap();

    let out = std::process::Command::new(binary_path())
        .current_dir(tmp.path())
        .env("HOME", tmp.path())
        .env("MOCK_JUDGE_KEY", "test-key")
        .args([
            "diff",
            "--judge",
            "test-model",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "tape diff --judge should exit 0: {out:?}"
    );
    let text = String::from_utf8(out.stdout).unwrap();

    // AC #1: the `judge:` marker is what distinguishes narrated entries
    // from structural ones. At least one substantive entry across these
    // two fixtures must have triggered the mock and been narrated.
    assert!(
        text.contains("judge:") && text.contains("MOCK_JUDGE_NARRATION"),
        "expected `judge:` marker + mock-returned text in output:\n{text}"
    );
    assert!(
        call_count.load(Ordering::SeqCst) >= 1,
        "mock should have received at least one judge call"
    );

    let _ = shutdown.send(());
}

#[test]
fn diff_judge_budget_caps_calls() {
    // #149 AC #5: `--judge-budget 0` cuts off all calls; remaining
    // substantive entries render with the budget-exceeded marker. This
    // exercises the cap without needing a multi-substantive fixture —
    // budget 0 forces the no-call branch to fire on the first
    // substantive entry. The mock is still here in case a future change
    // accidentally calls anyway; the assertion below catches that.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let (mock_endpoint, shutdown, call_count) = rt.block_on(async {
        let state = MockState {
            call_count: Arc::new(AtomicU32::new(0)),
            response: "should not appear".into(),
        };
        let counter = state.call_count.clone();
        let app = Router::new()
            .route("/v1/chat/completions", post(handle))
            .with_state(state);
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
        (
            format!("http://{addr}/v1/chat/completions"),
            tx,
            counter,
        )
    });

    let tmp = tempfile::tempdir().unwrap();
    let taperc = tmp.path().join(".taperc");
    std::fs::write(
        &taperc,
        format!(
            "judge:\n  model: placeholder\n  endpoint: {mock_endpoint}\n  api_key_env: MOCK_JUDGE_KEY\n"
        ),
    )
    .unwrap();

    let out = std::process::Command::new(binary_path())
        .current_dir(tmp.path())
        .env("HOME", tmp.path())
        .env("MOCK_JUDGE_KEY", "test-key")
        .args([
            "diff",
            "--judge",
            "test-model",
            "--judge-budget",
            "0",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("budget exceeded"),
        "budget=0 should mark substantive entries skipped:\n{text}"
    );
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        0,
        "budget=0 must not contact the upstream"
    );

    let _ = shutdown.send(());
}
