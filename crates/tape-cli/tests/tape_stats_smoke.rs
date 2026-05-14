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
