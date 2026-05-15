//! `tape stats` Step-1 CLI smoke test (issue #31). Drives the binary
//! against a checked-in fixture and asserts the report's headline
//! sections render. Mirrors the assertion pattern already in use by
//! `diff_integration.rs` / `annotate_integration.rs` /
//! `recap_integration.rs`.

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
fn stats_minimal_success_renders_expected_sections() {
    let out = Command::new(binary_path())
        .args(["stats", fixture("minimal-success.tape").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "tape stats failed: {out:?}");
    let text = String::from_utf8(out.stdout).unwrap();

    // Header section.
    assert!(text.contains("id: "), "missing id line:\n{text}");
    assert!(text.contains("task: "), "missing task line:\n{text}");
    assert!(text.contains("outcome: "), "missing outcome line:\n{text}");
    assert!(
        text.contains("2026-05-06T10:00:00Z → 2026-05-06T10:00:30Z"),
        "missing span line:\n{text}"
    );

    // Tracks histogram.
    assert!(text.contains("tracks: "), "missing tracks line:\n{text}");
    assert!(text.contains("task: 1"), "missing task count:\n{text}");
    assert!(text.contains("eject: 1"), "missing eject count:\n{text}");

    // The remaining one-line sections.
    assert!(text.contains("tokens:"), "missing tokens line:\n{text}");
    assert!(text.contains("tools:"), "missing tools line:\n{text}");
    assert!(text.contains("files:"), "missing files line:\n{text}");
    assert!(
        text.contains("redactions:"),
        "missing redactions line:\n{text}"
    );
}

#[test]
fn stats_exits_zero_on_minimal_fixture() {
    let out = Command::new(binary_path())
        .args(["stats", fixture("minimal-success.tape").to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn stats_help_is_wired() {
    let out = Command::new(binary_path())
        .args(["stats", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success(), "tape stats --help failed: {out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("stats"), "{text}");
}

/// Failure-path: a path that does not point at a readable cassette
/// must exit non-zero. Guards against accidentally swallowing IO
/// errors and reporting an empty/zero'd stats block.
#[test]
fn stats_exits_nonzero_on_missing_file() {
    let out = Command::new(binary_path())
        .args(["stats", "/this/path/does/not/exist/nope.tape"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "tape stats on a missing file should fail: {out:?}"
    );
}

// --- Phase 2 (issue #157): --format json ----------------------------

#[test]
fn stats_format_json_minimal_success_carries_schema_v1_0() {
    // Pinned schema_version is load-bearing per AC #2. The whole
    // contract of `tape stats --format json` is: once this lands,
    // consumers can pin against `1.0`.
    let out = Command::new(binary_path())
        .args([
            "stats",
            "--format",
            "json",
            fixture("minimal-success.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "tape stats --format json failed: {out:?}"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "1.0");
    assert!(v["id"].is_string());
    assert!(v["task"].is_string());
    assert_eq!(v["outcome"], "success");
    assert!(v["span"]["created_at"].is_string());
    assert!(v["span"]["ejected_at"].is_string());
    assert!(v["tracks"]["total"].is_u64());
    assert!(v["tracks"]["by_kind"].is_object());
    assert!(v["tools"]["mcp_call"].is_u64());
    assert!(v["tools"]["shell"].is_u64());
    assert!(v["files"]["read"].is_u64());
    assert!(v["files"]["write"].is_u64());
    // The fixture has no redactions.json so `redactions.recorded` is
    // false and `count` is absent (omit-not-null).
    assert_eq!(v["redactions"]["recorded"], false);
    let red = v["redactions"].as_object().unwrap();
    assert!(!red.contains_key("count"), "{red:?}");
}

#[test]
fn stats_format_json_minimal_success_has_tokens_when_model_call_present() {
    // minimal-success.tape has one model_call event — the token block
    // must be `recorded: true` with aggregate counts. Asserting against
    // the by_kind histogram tells the test reader what the fixture
    // contains without coupling to specific counts.
    let out = Command::new(binary_path())
        .args([
            "stats",
            "--format",
            "json",
            fixture("minimal-success.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let by_kind = v["tracks"]["by_kind"].as_object().unwrap();
    assert_eq!(by_kind["model_call"], 1, "fixture's model_call count");
    assert_eq!(v["tokens"]["recorded"], true);
    assert!(v["tokens"]["input"].is_u64());
    assert!(v["tokens"]["output"].is_u64());
    assert!(v["tokens"]["known_model_calls"].is_u64());
    assert!(v["tokens"]["missing_model_calls"].is_u64());
}

#[test]
fn stats_format_json_oversized_payload_passes_through() {
    // Pass-through guard: a second fixture must also emit the schema
    // cleanly (catches any per-fixture parsing accident).
    let out = Command::new(binary_path())
        .args([
            "stats",
            "--format",
            "json",
            fixture("oversized-payload.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "1.0");
    // The fixture is a snapshot-style cassette; wall_clock either
    // surfaces a Span or the snapshot-collapse marker. Either is
    // fine here — what matters is the wire shape.
    assert!(v["span"]["time_accounting"].is_string());
}

#[test]
fn stats_default_format_is_text_byte_for_byte() {
    // Phase-1 byte-for-byte preservation. The default (no --format)
    // and an explicit `--format text` must produce identical stdout.
    let no_flag = Command::new(binary_path())
        .args(["stats", fixture("minimal-success.tape").to_str().unwrap()])
        .output()
        .unwrap();
    let explicit_text = Command::new(binary_path())
        .args([
            "stats",
            "--format",
            "text",
            fixture("minimal-success.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(no_flag.status.success());
    assert!(explicit_text.status.success());
    assert_eq!(no_flag.stdout, explicit_text.stdout);
}

#[test]
fn stats_format_yaml_rejected_at_parse_time() {
    // clap value_parser rejection is the right surface — the user
    // gets a usage error, not a stack trace.
    let out = Command::new(binary_path())
        .args([
            "stats",
            "--format",
            "yaml",
            fixture("minimal-success.tape").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("yaml") || stderr.contains("possible values"),
        "clap should mention the rejection: {stderr}"
    );
}

#[test]
fn stats_format_json_exits_nonzero_on_missing_file() {
    // No partial JSON written to stdout when the cassette can't be
    // opened. Mirrors `tape verify --json`'s posture.
    let out = Command::new(binary_path())
        .args([
            "stats",
            "--format",
            "json",
            "/this/path/does/not/exist/nope.tape",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    // stdout must NOT contain a parseable JSON object — the failure
    // path should print nothing to stdout, only the error on stderr.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().is_empty()
            || serde_json::from_str::<serde_json::Value>(stdout.trim()).is_err(),
        "no JSON should be on stdout on error path: {stdout}"
    );
}
