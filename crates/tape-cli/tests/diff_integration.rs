//! Drives `tape diff` over two fixtures and validates output shape.

use std::process::Command;

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
fn diff_judge_flag_is_rejected_not_silently_ignored() {
    // Issue #62: --judge was destructured as `judge: _` and silently dropped.
    // Until judge-narration ships, the flag must fail loudly rather than
    // pretend to work.
    let out = Command::new(binary_path())
        .args([
            "diff",
            "--judge",
            "claude-haiku-4-5",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "tape diff --judge should exit non-zero, got success: {:?}",
        out
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("judge") && stderr.contains("not yet implemented"),
        "stderr should mention 'judge' and 'not yet implemented'; got:\n{stderr}"
    );
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
