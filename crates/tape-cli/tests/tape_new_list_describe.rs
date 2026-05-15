//! `tape new --list-templates` / `--describe-template <id>` Step-3
//! integration coverage (issue #179). Read-only introspection only;
//! no cassette generation. Mirrors the AC bullets 1-8 from the issue
//! body.

use std::process::Command;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(binary_path())
        .args(args)
        .output()
        .expect("spawn tape")
}

#[test]
fn list_templates_exits_zero_and_prints_three_lines_in_catalog_order() {
    let out = run(&["new", "--list-templates"]);
    assert!(out.status.success(), "list failed: {out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = s.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 3, "expected 3 template rows; got:\n{s}");
    // Catalog order is locked by BUILTIN_TEMPLATES.
    assert!(lines[0].starts_with("minimal "), "{}", lines[0]);
    assert!(lines[1].starts_with("test-fixture "), "{}", lines[1]);
    assert!(lines[2].starts_with("bug-investigation "), "{}", lines[2]);
    // Each row carries the version + required-task marker + description.
    assert!(lines[0].contains(" v1 "));
    assert!(lines[0].contains("required-task"));
    assert!(lines[1].contains("no-task"));
    assert!(lines[2].contains("required-task"));
    assert!(lines[0].contains("Smallest valid v0 cassette"));
    assert!(lines[1].contains("Deterministic 5-track fixture"));
    assert!(lines[2].contains("12-track bug-hunt archetype"));
}

#[test]
fn describe_minimal_renders_full_block_with_required_task() {
    let out = run(&["new", "--describe-template", "minimal"]);
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("template: minimal"));
    assert!(s.contains("version:  v1"));
    assert!(s.contains("required: --task"));
    assert!(s.contains("tracks:   2"), "minimal has 2 tracks:\n{s}");
    assert!(s.contains("description:"));
    assert!(s.contains("placeholders:"));
    assert!(s.contains("task (required)"));
    assert!(s.contains("liner-notes preview:"));
    assert!(
        s.contains("{{task}}"),
        "preview should include the raw placeholder: {s}"
    );
}

#[test]
fn describe_test_fixture_shows_required_none_and_default_meta_task() {
    let out = run(&["new", "--describe-template", "test-fixture"]);
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("template: test-fixture"));
    assert!(s.contains("required: (none)"));
    assert!(s.contains("tracks:   5"), "test-fixture has 5 tracks:\n{s}");
    assert!(
        s.contains("\"test fixture\""),
        "expected the default_meta_task literal in the placeholders block: {s}"
    );
}

#[test]
fn describe_bug_investigation_shows_twelve_tracks_and_required_task() {
    let out = run(&["new", "--describe-template", "bug-investigation"]);
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("template: bug-investigation"));
    assert!(s.contains("required: --task"));
    assert!(
        s.contains("tracks:   12"),
        "bug-investigation has 12 tracks:\n{s}"
    );
}

#[test]
fn describe_unknown_template_exits_two_with_known_ids_listed() {
    let out = run(&["new", "--describe-template", "nonexistent"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown template 'nonexistent'"),
        "{stderr}"
    );
    assert!(
        stderr.contains("minimal")
            && stderr.contains("test-fixture")
            && stderr.contains("bug-investigation"),
        "all three known ids must appear in the diagnostic: {stderr}"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).is_empty(),
        "no stdout on error"
    );
}

#[test]
fn list_templates_writes_nothing_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let before: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert!(before.is_empty(), "tempdir starts empty");

    let out = Command::new(binary_path())
        .args(["new", "--list-templates"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");

    let after: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert!(after.is_empty(), "introspection must not write to disk");
}

#[test]
fn describe_template_writes_nothing_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(binary_path())
        .args(["new", "--describe-template", "minimal"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");

    let after: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert!(after.is_empty(), "introspection must not write to disk");
}

#[test]
fn list_templates_rejects_combination_with_generation_flags() {
    let dir = tempfile::tempdir().unwrap();
    let out_path = dir.path().join("x.tape");
    let out = Command::new(binary_path())
        .args([
            "new",
            "--list-templates",
            "--template",
            "minimal",
            out_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "clap should reject: {out:?}");
    assert!(!out_path.exists(), "no cassette should be written");
}

#[test]
fn describe_template_rejects_combination_with_task_flag() {
    let out = run(&["new", "--describe-template", "minimal", "--task", "foo"]);
    assert!(!out.status.success(), "clap should reject: {out:?}");
}

#[test]
fn new_without_out_or_introspection_flags_is_a_clear_error() {
    let out = run(&["new"]);
    assert!(
        !out.status.success(),
        "should require <out> or an introspection flag: {out:?}"
    );
}
