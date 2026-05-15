//! Drives `tape diff` over two fixtures and validates output shape.

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

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

#[test]
fn diff_text_output_has_summary() {
    let out = Command::new(binary_path())
        .args([
            "diff",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "tape diff failed: {:?}", out);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("Task:"), "missing 'Task:' header:\n{text}");
    assert!(
        text.contains("Outcome:"),
        "missing 'Outcome:' line:\n{text}"
    );
    assert!(
        text.contains("Tool budget:"),
        "missing 'Tool budget:' line:\n{text}"
    );
    assert!(
        text.contains("Final answers:"),
        "missing 'Final answers:':\n{text}"
    );
}

#[test]
fn diff_json_output_parses() {
    let out = Command::new(binary_path())
        .args([
            "diff",
            "--format",
            "json",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["task"].is_string());
    assert!(v["alignment"].is_array());
    assert!(v["summary"]["tool_budget"].is_object());
}

#[test]
fn diff_judge_flag_emits_config_missing_error_when_no_taperc() {
    // Issue #149 AC7: rewriting the original `diff_judge_flag_is_rejected_not_silently_ignored`
    // test (issue #62). The flag now wires real judge-narration, so the
    // missing-config path must emit an actionable error pointing the user
    // at `.taperc::judge:` — not the legacy "not yet implemented" string.
    //
    // We point HOME at a fresh empty dir and cwd at the same so the
    // `.taperc` locator finds nothing.
    let scratch = tempfile::tempdir().unwrap();
    let out = Command::new(binary_path())
        .args([
            "diff",
            "--judge",
            "gpt-4o",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .current_dir(scratch.path())
        .env("HOME", scratch.path())
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "tape diff --judge with no .taperc should exit non-zero: {out:?}"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("judge:") && stderr.contains(".taperc"),
        "stderr should mention 'judge:' and '.taperc' (config-missing, not legacy 'not yet implemented'); got:\n{stderr}"
    );
    assert!(
        !stderr.contains("not yet implemented"),
        "stderr must NOT contain the legacy 'not yet implemented' string; got:\n{stderr}"
    );
}

#[test]
fn diff_judge_happy_path_narrates_substantive_entries() {
    // Issue #149 AC7: end-to-end happy path. Spin up an `axum` mock
    // server that returns a canned narration; invoke `tape diff
    // --judge` against fixtures that produce ≥1 substantive entry;
    // verify a `judge:` line appears in the text output, exit 0, and
    // the structural diff still renders.
    //
    // The fixtures differ in `meta.task` and likely produce at least
    // one substantive payload diff — the alignment is by step-intent
    // label so identical step kinds with differing payloads land in
    // `Class::Substantive`.

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .unwrap();

    let call_count = Arc::new(AtomicU32::new(0));
    let endpoint = rt.block_on(spawn_mock_judge(call_count.clone()));

    let scratch = tempfile::tempdir().unwrap();
    // Drop a `.taperc` with a `judge:` block pointing at our mock.
    let taperc_body = format!(
        "judge:\n  model: mock-model\n  endpoint: {endpoint}\n  api_key_env: TAPE_DIFF_INTEG_TEST_KEY\n  max_attempts: 1\n",
    );
    std::fs::write(scratch.path().join(".taperc"), taperc_body).unwrap();

    let out = Command::new(binary_path())
        .args([
            "diff",
            "--judge",
            "mock-model",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .current_dir(scratch.path())
        .env("HOME", scratch.path())
        .env("TAPE_DIFF_INTEG_TEST_KEY", "fake-key")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "tape diff --judge happy path should succeed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    // Structural diff still renders (AC1 — `--judge` is additive).
    assert!(text.contains("Task:"), "missing 'Task:' header:\n{text}");
    assert!(
        text.contains("Tool budget:"),
        "structural diff summary missing:\n{text}"
    );
    // At least one `judge:` narration line landed (AC1 — stable marker).
    assert!(
        text.contains("judge: "),
        "expected at least one 'judge: ' narration line:\n{text}"
    );
    // The mock canned response should appear verbatim.
    assert!(
        text.contains("MOCK_NARRATION_FROM_JUDGE"),
        "expected the mocked judge text in the rendered output:\n{text}"
    );
    // And the mock should have been called at least once.
    assert!(
        call_count.load(Ordering::SeqCst) >= 1,
        "expected ≥1 judge call, got {}",
        call_count.load(Ordering::SeqCst)
    );
}

#[test]
fn diff_judge_budget_zero_skips_every_entry() {
    // Issue #149 AC5: `--judge-budget 0` must short-circuit every
    // substantive entry with `[narration skipped — budget exceeded]`
    // and never reach the network. We give it an endpoint that would
    // panic if hit, so any reachable request fails the test.
    let scratch = tempfile::tempdir().unwrap();
    std::fs::write(
        scratch.path().join(".taperc"),
        // Bogus endpoint — the test only passes if it's never used.
        "judge:\n  model: mock-model\n  endpoint: http://127.0.0.1:1/never\n  api_key_env: TAPE_DIFF_INTEG_TEST_KEY2\n  max_attempts: 1\n",
    )
    .unwrap();
    let out = Command::new(binary_path())
        .args([
            "diff",
            "--judge",
            "mock-model",
            "--judge-budget",
            "0",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .current_dir(scratch.path())
        .env("HOME", scratch.path())
        .env("TAPE_DIFF_INTEG_TEST_KEY2", "fake-key")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "tape diff --judge --judge-budget 0 should succeed (no calls made): stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    // Every substantive entry should carry the budget-exceeded marker.
    // The two fixtures differ in task, so at least one substantive
    // entry exists; we only assert the marker is present.
    assert!(
        text.contains("budget exceeded"),
        "expected at least one 'budget exceeded' marker:\n{text}"
    );
}

async fn spawn_mock_judge(calls: Arc<AtomicU32>) -> String {
    use axum::extract::State;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{json, Value};

    async fn handle(State(calls): State<Arc<AtomicU32>>, Json(_body): Json<Value>) -> Json<Value> {
        calls.fetch_add(1, Ordering::SeqCst);
        Json(json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "MOCK_NARRATION_FROM_JUDGE: behavioral delta one-liner."
                }
            }]
        }))
    }

    let app = Router::new()
        .route("/v1/chat/completions", post(handle))
        .with_state(calls);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}/v1/chat/completions")
}

#[test]
fn diff_without_judge_still_succeeds() {
    // Pass-through guard for #62: the no-flag case must keep working.
    let out = Command::new(binary_path())
        .args([
            "diff",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "tape diff (no --judge) should succeed: {:?}",
        out
    );
}

#[test]
fn diff_self_is_all_identical() {
    let out = Command::new(binary_path())
        .args([
            "diff",
            "--all",
            "--format",
            "json",
            fixture("killer-scenario-a.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let alignment = v["alignment"].as_array().unwrap();
    assert!(!alignment.is_empty());
    for pair in alignment {
        assert_eq!(
            pair["class"], "identical",
            "self-diff should be all identical; got {pair}"
        );
    }
    assert_eq!(v["summary"]["answers_equivalent"], true);
}
