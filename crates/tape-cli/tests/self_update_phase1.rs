//! End-to-end coverage for `tape self-update --check` (issue #234,
//! carved from #108 Phase 1). Spins up an `axum` mock server on
//! `127.0.0.1:0` and injects its URL into the binary via the
//! `TAPE_SELF_UPDATE_URL` env override. No live network calls in CI.
//!
//! Asserts:
//! - `tape self-update` (no `--check`) → exit 2 with "Phase 2" message
//! - `--check --format json` against a mock returning an
//!   "update available" envelope → exit 0, JSON schema 1.0
//! - `--check` (text default) against an "up-to-date" mock (tag ==
//!   the binary's compile-time version) → exit 0, three text lines
//! - mock returning HTTP 500 → exit 0 with `status: unknown`
//! - mock URL pointing at an unreachable address → exit 0, unknown
//! - `--help` documents `--check` and `--format`
//! - bogus `--format` → exit 2

use axum::routing::get;
use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

struct MockServer {
    url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    _rt: Arc<Runtime>,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

fn spawn_mock(handler: Router) -> MockServer {
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let rt_clone = rt.clone();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let url = rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        rt_clone.spawn(async move {
            let _ = axum::serve(listener, handler)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });
        format!("http://{addr}/releases/latest")
    });
    MockServer {
        url,
        shutdown: Some(tx),
        _rt: rt,
    }
}

fn run(args: &[&str], env: Option<(&str, &str)>) -> std::process::Output {
    let mut cmd = std::process::Command::new(binary_path());
    cmd.arg("self-update");
    for a in args {
        cmd.arg(a);
    }
    if let Some((k, v)) = env {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

#[test]
fn no_check_flag_exits_two_with_phase_two_message() {
    let r = run(&[], None);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("Phase 2"), "stderr: {stderr}");
    assert!(stderr.contains("#108"), "stderr: {stderr}");
}

#[test]
fn check_json_against_update_available_mock() {
    let body = r#"{
        "tag_name": "v999.999.999",
        "html_url": "https://example.invalid/releases/tag/v999.999.999"
    }"#;
    let app = Router::new().route(
        "/releases/latest",
        get(move || async move {
            axum::response::Json(serde_json::from_str::<serde_json::Value>(body).unwrap())
        }),
    );
    let mock = spawn_mock(app);
    let r = run(
        &["--check", "--format", "json"],
        Some(("TAPE_SELF_UPDATE_URL", &mock.url)),
    );
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["schema_version"], "1.0");
    assert_eq!(v["status"], "update_available");
    assert_eq!(v["latest"], "999.999.999");
    assert_eq!(
        v["release_url"],
        "https://example.invalid/releases/tag/v999.999.999"
    );
}

#[test]
fn check_text_against_up_to_date_mock() {
    // Use the binary's own compile-time version to force the
    // up-to-date branch.
    let current = env!("CARGO_PKG_VERSION");
    let body = format!(
        r#"{{ "tag_name": "v{current}", "html_url": "https://example.invalid/tag/v{current}" }}"#
    );
    let app = Router::new().route(
        "/releases/latest",
        get(move || async move {
            axum::response::Json(serde_json::from_str::<serde_json::Value>(&body).unwrap())
        }),
    );
    let mock = spawn_mock(app);
    let r = run(&["--check"], Some(("TAPE_SELF_UPDATE_URL", &mock.url)));
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("status:   up-to-date"), "stdout: {stdout}");
    assert!(!stdout.contains("release:"), "stdout: {stdout}");
}

#[test]
fn check_against_500_mock_exits_zero_with_unknown_status() {
    let app = Router::new().route(
        "/releases/latest",
        get(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
    );
    let mock = spawn_mock(app);
    let r = run(&["--check"], Some(("TAPE_SELF_UPDATE_URL", &mock.url)));
    assert!(r.status.success(), "exit must be 0 on net failure: {r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("status:   unknown"), "stdout: {stdout}");
    assert!(stdout.contains("HTTP 500"), "stdout: {stdout}");
}

#[test]
fn check_against_unreachable_address_exits_zero_with_unknown() {
    // 127.0.0.1:1 is a port that nothing should ever bind. Combined
    // with the 5s connect timeout, this should reliably fail fast.
    let r = run(
        &["--check"],
        Some(("TAPE_SELF_UPDATE_URL", "http://127.0.0.1:1/releases/latest")),
    );
    assert!(r.status.success(), "exit must be 0 on net failure: {r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("status:   unknown"), "stdout: {stdout}");
}

#[test]
fn bogus_format_exits_two() {
    let r = run(&["--check", "--format", "yaml"], None);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("unknown --format"), "stderr: {stderr}");
}

#[test]
fn help_documents_check_and_format_flags() {
    let r = std::process::Command::new(binary_path())
        .args(["self-update", "--help"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let lower = stdout.to_lowercase();
    assert!(lower.contains("--check"), "help: {stdout}");
    assert!(lower.contains("--format"), "help: {stdout}");
    assert!(lower.contains("phase 1"), "help: {stdout}");
}
