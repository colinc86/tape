//! `tape export` Step-1 CLI smoke tests. Covers the
//! `--format md` happy path, the default output path, `-o` override,
//! the input-equals-output refusal, and the Step-2/3 placeholders
//! that already accept `--format html` / `--format both` at the CLI
//! surface but refuse with a structured `EXPORT_FORMAT_UNAVAILABLE`
//! diagnostic until Step 2 lands.

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

fn isolated_minimal() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &dst).unwrap();
    (dir, dst)
}

#[test]
fn export_md_default_output_path() {
    let (_dir, input) = isolated_minimal();
    let expected_out = input.with_extension("md");

    let out = Command::new(binary_path())
        .args(["export", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "tape export failed: {out:?}");
    assert!(expected_out.exists(), "default output path missing");

    let body = std::fs::read_to_string(&expected_out).unwrap();
    assert!(body.starts_with("# "), "must start with H1 title");
    assert!(
        body.contains("## Liner notes"),
        "missing liner-notes section"
    );
    assert!(body.contains("## Tracklist"), "missing tracklist section");
}

#[test]
fn export_md_with_explicit_out_path() {
    let (dir, input) = isolated_minimal();
    let custom = dir.path().join("nested").join("report.md");

    let out = Command::new(binary_path())
        .args([
            "export",
            input.to_str().unwrap(),
            "--format",
            "md",
            "-o",
            custom.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(custom.exists(), "explicit -o path must materialise");
    assert!(
        custom.parent().unwrap().is_dir(),
        "missing parent dir must be created"
    );
}

#[test]
fn export_refuses_out_equals_input() {
    let (_dir, input) = isolated_minimal();
    let out = Command::new(binary_path())
        .args([
            "export",
            input.to_str().unwrap(),
            "-o",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("must differ from <file>"), "{stderr}");
}

#[test]
fn export_format_html_is_step_2_diagnostic() {
    // The flag is accepted at parse time so Step 2 doesn't have to
    // re-do the CLI; the body emits an EXPORT_FORMAT_UNAVAILABLE
    // diagnostic and exits 2. A future Step-2 PR replaces the guard.
    let (_dir, input) = isolated_minimal();
    let out = Command::new(binary_path())
        .args(["export", input.to_str().unwrap(), "--format", "html"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("EXPORT_FORMAT_UNAVAILABLE"), "{stderr}");
    assert!(
        stderr.contains("Step 2"),
        "diagnostic names the follow-on step: {stderr}"
    );
}

#[test]
fn export_format_both_is_step_2_diagnostic() {
    let (_dir, input) = isolated_minimal();
    let out = Command::new(binary_path())
        .args(["export", input.to_str().unwrap(), "--format", "both"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("EXPORT_FORMAT_UNAVAILABLE"), "{stderr}");
}

#[test]
fn export_unknown_format_exits_2() {
    let (_dir, input) = isolated_minimal();
    let out = Command::new(binary_path())
        .args(["export", input.to_str().unwrap(), "--format", "yaml"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}
