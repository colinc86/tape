//! `tape recap --auto` + `.taperc::recap.default_model` integration
//! coverage. Step-3 of #105 / issue #198. Mocks the judge upstream
//! with axum (same pattern as `recap_auto_happy.rs` /
//! `relinernote_taperc.rs`) and inspects the request body to verify
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
                "content": "Race condition in process_refund() — repro lands in PR #142."
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
/// and (optionally) a `recap.default_model` field. The `judge.model`
/// defaults to `judge-baseline` (so we can distinguish "fell through
/// to judge.model" from "consumed the recap override"); pass
/// `default_model` separately for the override.
fn write_taperc(home: &std::path::Path, addr: &std::net::SocketAddr, default_model: Option<&str>) {
    let recap_block = default_model
        .map(|m| format!("\nrecap:\n  default_model: {m}\n"))
        .unwrap_or_default();
    let yaml = format!(
        "judge:\n  model: judge-baseline\n  api_key_env: TAPE_RECAP_TAPERC_TEST_KEY\n  endpoint: http://{addr}/v1/chat/completions\n  max_attempts: 1\n{recap_block}",
    );
    std::fs::write(home.join(".taperc"), yaml).unwrap();
}

fn run_recap(
    home: &std::path::Path,
    cassette: &std::path::Path,
    out: &std::path::Path,
    extra_args: &[&str],
) -> std::process::Output {
    let mut cmd = std::process::Command::new(binary_path());
    cmd.args([
        "recap",
        cassette.to_str().unwrap(),
        "--auto",
        "-o",
        out.to_str().unwrap(),
    ])
    .args(extra_args)
    .env_remove("HOME")
    .env("HOME", home)
    .env("TAPE_RECAP_TAPERC_TEST_KEY", "dummy")
    .current_dir(home);
    cmd.output().unwrap()
}

#[test]
fn taperc_default_model_consumed_when_flag_absent() {
    // AC: with `.taperc::recap.default_model: claude-haiku-4-5` and no
    // `--model` flag, the judge HTTP call's body.model is
    // `claude-haiku-4-5` (not the `judge.model` value).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mock = spawn_mock(&rt);
    write_taperc(dir.path(), &mock.addr, Some("claude-haiku-4-5"));

    let cassette = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &cassette).unwrap();
    let out = dir.path().join("output.recap.tape");

    let r = run_recap(dir.path(), &cassette, &out, &[]);
    assert!(
        r.status.success(),
        "tape recap --auto failed: stdout={} stderr={}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr),
    );
    let model = mock.last_model.lock().unwrap().clone();
    assert_eq!(
        model.as_deref(),
        Some("claude-haiku-4-5"),
        "judge HTTP body.model should reflect the recap.default_model fallback"
    );
}

#[test]
fn cli_model_overrides_taperc_default_model() {
    // AC: `--model claude-opus-4-7` overrides a `.taperc` that says
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
    let out = dir.path().join("output.recap.tape");

    let r = run_recap(dir.path(), &cassette, &out, &["--model", "claude-opus-4-7"]);
    assert!(r.status.success(), "{r:?}");
    assert_eq!(
        mock.last_model.lock().unwrap().clone().as_deref(),
        Some("claude-opus-4-7"),
        "CLI --model should win over .taperc::recap.default_model"
    );
}

#[test]
fn missing_recap_section_falls_through_to_judge_model() {
    // AC: no `recap:` block in `.taperc` → `judge.model` is consumed
    // (existing pre-#198 behavior, regression guard).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mock = spawn_mock(&rt);
    write_taperc(dir.path(), &mock.addr, None);
    let cassette = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &cassette).unwrap();
    let out = dir.path().join("output.recap.tape");

    let r = run_recap(dir.path(), &cassette, &out, &[]);
    assert!(r.status.success(), "{r:?}");
    assert_eq!(
        mock.last_model.lock().unwrap().clone().as_deref(),
        Some("judge-baseline"),
        "with no recap block + no --model, judge.model should be consumed"
    );
}

#[test]
fn typo_under_recap_section_exits_two() {
    // AC: a typo under `recap:` (`default-model:` instead of
    // `default_model:`) fails config-load. The diagnostic surfaces
    // through the `RECAP_AUTO_CONFIG` exit-2 path.
    let dir = tempfile::tempdir().unwrap();
    let taperc = dir.path().join(".taperc");
    std::fs::write(
        &taperc,
        "judge:\n  model: x\n  api_key_env: K\n  max_attempts: 1\n\nrecap:\n  default-model: y\n",
    )
    .unwrap();
    let cassette = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &cassette).unwrap();
    let out = dir.path().join("output.recap.tape");

    let r = run_recap(dir.path(), &cassette, &out, &[]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("RECAP_AUTO_CONFIG"),
        "expected RECAP_AUTO_CONFIG in diagnostic: {stderr}"
    );
}

#[test]
fn empty_cli_model_falls_through_to_taperc() {
    // Empty `--model ""` should NOT consume the slot — falls through
    // to `.taperc::recap.default_model`. The precedence chain's
    // `.filter(|s| !s.is_empty())` is the load-bearing piece here.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mock = spawn_mock(&rt);
    write_taperc(dir.path(), &mock.addr, Some("claude-haiku-4-5"));
    let cassette = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &cassette).unwrap();
    let out = dir.path().join("output.recap.tape");

    let r = run_recap(dir.path(), &cassette, &out, &["--model", ""]);
    assert!(r.status.success(), "{r:?}");
    assert_eq!(
        mock.last_model.lock().unwrap().clone().as_deref(),
        Some("claude-haiku-4-5"),
        "empty --model should fall through to .taperc::recap.default_model"
    );
}
