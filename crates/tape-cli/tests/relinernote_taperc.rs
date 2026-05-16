//! `tape relinernote` + `.taperc::relinernote.default_model` integration
//! coverage. Step-2 of #71 / issue #194. Mocks the judge upstream with
//! axum (same pattern as `relinernote_integration.rs` /
//! `recap_auto_happy.rs`) and inspects the request body to verify
//! which model id reached the judge HTTP call.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

#[derive(Clone)]
struct MockState {
    call_count: Arc<AtomicU32>,
    last_model: Arc<Mutex<Option<String>>>,
}

async fn handle(State(state): State<MockState>, Json(body): Json<Value>) -> Json<Value> {
    state.call_count.fetch_add(1, Ordering::SeqCst);
    if let Some(m) = body.get("model").and_then(Value::as_str) {
        *state.last_model.lock().unwrap() = Some(m.to_owned());
    }
    Json(json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "## What I was asked to do\nx\n\n## What I found\nx\n\n## Suggested next step / fix\nx\n\n## What I'm uncertain about\nx\n"
            }
        }]
    }))
}

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

struct MockServer {
    addr: std::net::SocketAddr,
    _shutdown: tokio::sync::oneshot::Sender<()>,
    last_model: Arc<Mutex<Option<String>>>,
}

fn spawn_mock(rt: &tokio::runtime::Runtime) -> MockServer {
    rt.block_on(async {
        let last_model = Arc::new(Mutex::new(None));
        let state = MockState {
            call_count: Arc::new(AtomicU32::new(0)),
            last_model: last_model.clone(),
        };
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
        MockServer {
            addr,
            _shutdown: tx,
            last_model,
        }
    })
}

/// Write a `.taperc` with a `judge:` block pinned at the mock server
/// and (optionally) a `relinernote.default_model` field. The
/// `judge.model` defaults to `judge-baseline` (so we can distinguish
/// "fell through to judge.model" from "consumed the relinernote
/// override"); pass `default_model` separately for the override.
fn write_taperc(home: &std::path::Path, addr: &std::net::SocketAddr, default_model: Option<&str>) {
    let relinernote_block = default_model
        .map(|m| format!("\nrelinernote:\n  default_model: {m}\n"))
        .unwrap_or_default();
    let yaml = format!(
        "judge:\n  model: judge-baseline\n  api_key_env: TAPE_RELINER_TEST_KEY\n  endpoint: http://{addr}/v1/chat/completions\n{relinernote_block}",
    );
    std::fs::write(home.join(".taperc"), yaml).unwrap();
}

fn run_relinernote(
    home: &std::path::Path,
    cassette: &std::path::Path,
    out: &std::path::Path,
    extra_args: &[&str],
) -> std::process::Output {
    let mut cmd = std::process::Command::new(binary_path());
    cmd.args([
        "relinernote",
        cassette.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ])
    .args(extra_args)
    .env_remove("HOME")
    .env("HOME", home)
    .env("TAPE_RELINER_TEST_KEY", "dummy")
    .current_dir(home);
    cmd.output().unwrap()
}

#[test]
fn taperc_default_model_consumed_when_flag_absent() {
    // AC: with `.taperc::relinernote.default_model: claude-haiku-4-5`
    // and no `--model` flag, the judge HTTP call's body.model is
    // `claude-haiku-4-5` (not the `judge.model` value).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mock = spawn_mock(&rt);
    write_taperc(dir.path(), &mock.addr, Some("claude-haiku-4-5"));

    // Use the bundled bug-investigation fixture (has a non-empty
    // task event) so the relinernote NO_TASK guard doesn't fire.
    let cassette_src = fixture("minimal-success.tape");
    let cassette = dir.path().join("input.tape");
    std::fs::copy(&cassette_src, &cassette).unwrap();
    let out = dir.path().join("relinered.tape");

    let r = run_relinernote(dir.path(), &cassette, &out, &[]);
    assert!(
        r.status.success(),
        "tape relinernote failed: stdout={} stderr={}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr),
    );
    let model = mock.last_model.lock().unwrap().clone();
    assert_eq!(
        model.as_deref(),
        Some("claude-haiku-4-5"),
        "judge HTTP body.model should reflect the relinernote.default_model fallback"
    );
}

#[test]
fn cli_model_overrides_taperc_default_model() {
    // AC: `--model gpt-5` overrides a `.taperc` that says
    // `claude-haiku-4-5`.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mock = spawn_mock(&rt);
    write_taperc(dir.path(), &mock.addr, Some("claude-haiku-4-5"));
    let cassette = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &cassette).unwrap();
    let out = dir.path().join("relinered.tape");

    let r = run_relinernote(dir.path(), &cassette, &out, &["--model", "claude-opus-4-7"]);
    assert!(r.status.success(), "{r:?}");
    assert_eq!(
        mock.last_model.lock().unwrap().clone().as_deref(),
        Some("claude-opus-4-7"),
        "CLI --model should win over .taperc::relinernote.default_model"
    );
}

#[test]
fn missing_relinernote_section_falls_through_to_judge_model() {
    // AC: no `relinernote:` block in `.taperc` → `judge.model` is
    // consumed (existing pre-#194 behavior, regression guard).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mock = spawn_mock(&rt);
    write_taperc(dir.path(), &mock.addr, None);
    let cassette = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &cassette).unwrap();
    let out = dir.path().join("relinered.tape");

    let r = run_relinernote(dir.path(), &cassette, &out, &[]);
    assert!(r.status.success(), "{r:?}");
    assert_eq!(
        mock.last_model.lock().unwrap().clone().as_deref(),
        Some("judge-baseline"),
        "with no relinernote block + no --model, judge.model should be consumed"
    );
}

#[test]
fn typo_under_relinernote_section_exits_two() {
    // AC: a typo under `relinernote:` (`default-model:` instead of
    // `default_model:`) fails config-load. The diagnostic surfaces
    // through the `RELINER_CONFIG` exit-2 path.
    let dir = tempfile::tempdir().unwrap();
    let taperc = dir.path().join(".taperc");
    std::fs::write(
        &taperc,
        "judge:\n  model: x\n  api_key_env: K\n\nrelinernote:\n  default-model: y\n",
    )
    .unwrap();
    let cassette = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &cassette).unwrap();
    let out = dir.path().join("relinered.tape");

    let r = run_relinernote(dir.path(), &cassette, &out, &[]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("RELINER_CONFIG"),
        "expected RELINER_CONFIG in diagnostic: {stderr}"
    );
}
