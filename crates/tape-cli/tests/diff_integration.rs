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
    assert!(text.contains("Outcome:"), "missing 'Outcome:' line:\n{text}");
    assert!(text.contains("Tool budget:"), "missing 'Tool budget:' line:\n{text}");
    assert!(text.contains("Final answers:"), "missing 'Final answers:':\n{text}");
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

/// Issue #62: `--judge` was accepted by clap but silently ignored. Until
/// narration ships, reject it with a clear error so users get a real
/// signal instead of a no-op flag.
#[test]
fn diff_judge_flag_returns_clear_error() {
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
    assert!(!out.status.success(), "expected non-zero exit; got {:?}", out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--judge") && stderr.contains("not yet implemented"),
        "expected helpful error mentioning --judge; got:\n{stderr}"
    );
}

/// Regression: omitting `--judge` still works exactly as before.
#[test]
fn diff_without_judge_still_succeeds() {
    let out = Command::new(binary_path())
        .args([
            "diff",
            fixture("minimal-success.tape").to_str().unwrap(),
            fixture("killer-scenario-a.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "no-flag baseline broke: {:?}", out);
}
