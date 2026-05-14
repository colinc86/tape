//! `tape new --template minimal` Phase-1 integration coverage.
//! Tracks Principal's scoping comment on #99 — happy path, missing
//! `--task`, output-exists with and without `--force`, the
//! deterministic-output property, and the JSONL-safety rejection on
//! ill-formed `--task` values.

use std::process::Command;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

/// Step-1 of #99 requires byte-identical *content* for two
/// invocations with the same `--task` / `--created-at` /
/// `--recorder-agent`. Zip-file metadata (notably mtimes baked in by
/// `zip::SimpleFileOptions`) varies across runs, so we compare the
/// extracted `meta.yaml` + `liner-notes.md` + `tracks.jsonl` bytes
/// rather than the .tape file as a whole. That preserves the
/// "regenerate a fixture and diff" use case Principal's pitfall
/// callout names ("fixture-regeneration test … byte-identical output
/// across runs") without paying for a deterministic-zip writer in
/// Step 1.
fn extract_content(path: &std::path::Path) -> (String, String, String) {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    (
        raw.meta_yaml.unwrap(),
        raw.liner_md.unwrap(),
        raw.tracks_jsonl.unwrap(),
    )
}

#[test]
fn happy_path_writes_a_verify_clean_cassette() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("greeting.tape");

    let result = Command::new(binary_path())
        .args([
            "new",
            out.to_str().unwrap(),
            "--task",
            "Say hello to the new user",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "tape new failed: {result:?}");
    assert!(out.exists());

    // verify clean
    let v = Command::new(binary_path())
        .args(["verify", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(v.status.success(), "verify failed: {v:?}");

    // Track-level shape: exactly one task at step 1 + one eject as the
    // final event (SPEC §5.4).
    let raw = tape_format::reader::RawTape::open(&out).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap()).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].kind, tape_format::tracks::Kind::Task);
    assert_eq!(tracks[0].step, 1);
    assert_eq!(tracks[0].payload["prompt"], "Say hello to the new user");
    assert_eq!(tracks[1].kind, tape_format::tracks::Kind::Eject);
    assert_eq!(tracks[1].step, 2);

    // meta.new provenance.
    let meta = tape_format::meta::Meta::parse(raw.meta_yaml.as_deref().unwrap()).unwrap();
    let nb = meta
        .new_block
        .expect("meta.new must be populated by `tape new`");
    assert_eq!(nb.template_id, "minimal");
    assert_eq!(nb.template_version, "1");
    assert_eq!(nb.placeholders_filled, vec!["task".to_owned()]);
    assert!(meta.recorder.agent.contains("+new+minimal"));
}

#[test]
fn missing_task_exits_with_new_missing_placeholder() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");

    let result = Command::new(binary_path())
        .args(["new", out.to_str().unwrap()])
        .output()
        .unwrap();
    // clap exits 2 on missing required arg; the error message originates
    // from clap, but the exit code matches our explicit
    // NEW_MISSING_PLACEHOLDER slot.
    assert_eq!(result.status.code(), Some(2));
    assert!(!out.exists());
}

#[test]
fn output_exists_without_force_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("already-here.tape");
    std::fs::write(&out, b"sentinel-content").unwrap();

    let result = Command::new(binary_path())
        .args(["new", out.to_str().unwrap(), "--task", "should not write"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("NEW_OUTPUT_EXISTS"), "{stderr}");
    // Original file untouched.
    assert_eq!(std::fs::read(&out).unwrap(), b"sentinel-content");
}

#[test]
fn force_overwrites_existing_output() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("clobber.tape");
    std::fs::write(&out, b"sentinel-content").unwrap();

    let result = Command::new(binary_path())
        .args([
            "new",
            out.to_str().unwrap(),
            "--task",
            "force overwrite",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    // Output is now a real cassette.
    let v = Command::new(binary_path())
        .args(["verify", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(v.status.success(), "{v:?}");
}

#[test]
fn deterministic_output_for_same_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.tape");
    let b = dir.path().join("b.tape");

    let common = [
        "new",
        // Placeholder for the path; will swap per-invocation.
        "PATH",
        "--task",
        "Investigate flaky test",
        "--created-at",
        "2026-01-01T00:00:00.000Z",
        "--recorder-agent",
        "tape-cli/test+new+minimal",
    ];

    let mut args_a = common.to_vec();
    args_a[1] = a.to_str().unwrap();
    let res_a = Command::new(binary_path()).args(&args_a).output().unwrap();
    assert!(res_a.status.success(), "{res_a:?}");

    let mut args_b = common.to_vec();
    args_b[1] = b.to_str().unwrap();
    let res_b = Command::new(binary_path()).args(&args_b).output().unwrap();
    assert!(res_b.status.success(), "{res_b:?}");

    // See `extract_content` doc-comment for why we compare extracted
    // content rather than .tape bytes.
    assert_eq!(
        extract_content(&a),
        extract_content(&b),
        "same --created-at / --recorder-agent / --task must produce byte-identical cassette content"
    );
}

#[test]
fn task_with_quote_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");

    let result = Command::new(binary_path())
        .args([
            "new",
            out.to_str().unwrap(),
            "--task",
            r#"bad " injection attempt"#,
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(!out.exists());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("NEW_MISSING_PLACEHOLDER"),
        "expected NEW_MISSING_PLACEHOLDER: {stderr}"
    );
}

#[test]
fn task_with_newline_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");

    let result = Command::new(binary_path())
        .args(["new", out.to_str().unwrap(), "--task", "foo\nbar"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(!out.exists());
}

#[test]
fn task_with_backslash_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");

    let result = Command::new(binary_path())
        .args([
            "new",
            out.to_str().unwrap(),
            "--task",
            r"backslash \ in the task",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(!out.exists());
}

#[test]
fn empty_task_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");

    let result = Command::new(binary_path())
        .args(["new", out.to_str().unwrap(), "--task", ""])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(!out.exists());
}

#[test]
fn invalid_created_at_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");

    let result = Command::new(binary_path())
        .args([
            "new",
            out.to_str().unwrap(),
            "--task",
            "x",
            "--created-at",
            "not-a-timestamp",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(!out.exists());
}
