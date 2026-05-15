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
fn task_with_double_brace_exits_2() {
    // A --task that itself names another placeholder would silently
    // cascade through the subsequent {{created_at}} / {{ejected_at}}
    // substitutions in `cmd_new`, causing meta.task and
    // tracks[0].payload.prompt to diverge. The validator rejects `{{`
    // so the "literal, grep-auditable" substitution contract holds.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");

    let result = Command::new(binary_path())
        .args(["new", out.to_str().unwrap(), "--task", "{{created_at}}"])
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

// --- Step 2 of #99 (issue #162): test-fixture + bug-investigation ---

#[test]
fn test_fixture_template_succeeds_without_task() {
    // AC #1: `tape new --template test-fixture out.tape` with no
    // --task flag exits 0 and produces a valid cassette.
    let dir = tempfile::tempdir().unwrap();
    let out_path = dir.path().join("out.tape");

    let result = std::process::Command::new(binary_path())
        .args([
            "new",
            out_path.to_str().unwrap(),
            "--template",
            "test-fixture",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    assert!(out_path.exists());

    // verify the output.
    let v = std::process::Command::new(binary_path())
        .args(["verify", out_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(v.status.success(), "{v:?}");

    // The meta.task is the template's literal default; the
    // template_id round-trips through meta.new.
    let raw = tape_format::reader::RawTape::open(&out_path).unwrap();
    let meta = tape_format::meta::Meta::parse(raw.meta_yaml.as_deref().unwrap()).unwrap();
    assert_eq!(meta.task, "test fixture");
    let new_block = meta.new_block.as_ref().expect("meta.new present");
    assert_eq!(new_block.template_id, "test-fixture");
    assert_eq!(new_block.template_version, "1");
    assert!(
        new_block.placeholders_filled.is_empty(),
        "test-fixture has no required placeholders"
    );
}

#[test]
fn test_fixture_template_is_deterministic() {
    // AC #2: two runs with the same `--created-at` and
    // `--recorder-agent` produce byte-identical meta.yaml +
    // liner-notes.md + tracks.jsonl.
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.tape");
    let b = dir.path().join("b.tape");
    let created = "2026-01-01T00:00:00Z";
    let agent = "tape-cli/test+new+test-fixture";

    for out in [&a, &b] {
        let res = std::process::Command::new(binary_path())
            .args([
                "new",
                out.to_str().unwrap(),
                "--template",
                "test-fixture",
                "--created-at",
                created,
                "--recorder-agent",
                agent,
            ])
            .output()
            .unwrap();
        assert!(res.status.success(), "{res:?}");
    }
    let raw_a = tape_format::reader::RawTape::open(&a).unwrap();
    let raw_b = tape_format::reader::RawTape::open(&b).unwrap();
    assert_eq!(raw_a.meta_yaml, raw_b.meta_yaml, "meta.yaml must match");
    assert_eq!(raw_a.liner_md, raw_b.liner_md, "liner-notes.md must match");
    assert_eq!(
        raw_a.tracks_jsonl, raw_b.tracks_jsonl,
        "tracks.jsonl must match"
    );
}

#[test]
fn test_fixture_has_five_tracks_and_recorded_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");
    let res = std::process::Command::new(binary_path())
        .args(["new", out.to_str().unwrap(), "--template", "test-fixture"])
        .output()
        .unwrap();
    assert!(res.status.success(), "{res:?}");

    let raw = tape_format::reader::RawTape::open(&out).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap()).unwrap();
    assert_eq!(tracks.len(), 5);
    // Three model_calls all carry token counts → token_totals will
    // see input == 240, output == 100, missing_model_calls == 0.
    let model_calls = tracks
        .iter()
        .filter(|t| t.kind == tape_format::tracks::Kind::ModelCall)
        .count();
    assert_eq!(model_calls, 3);
}

#[test]
fn bug_investigation_template_requires_task() {
    // The template carries `placeholders.task.required: true`, so
    // omitting --task must surface NEW_MISSING_PLACEHOLDER, exit 2,
    // and write no output.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");
    let res = std::process::Command::new(binary_path())
        .args([
            "new",
            out.to_str().unwrap(),
            "--template",
            "bug-investigation",
        ])
        .output()
        .unwrap();
    assert_eq!(res.status.code(), Some(2), "{res:?}");
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(stderr.contains("NEW_MISSING_PLACEHOLDER"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn bug_investigation_template_produces_twelve_track_cassette() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");
    let res = std::process::Command::new(binary_path())
        .args([
            "new",
            out.to_str().unwrap(),
            "--template",
            "bug-investigation",
            "--task",
            "Investigate refund-race for CUST-447139",
        ])
        .output()
        .unwrap();
    assert!(res.status.success(), "{res:?}");

    let v = std::process::Command::new(binary_path())
        .args(["verify", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(v.status.success(), "verify failed: {v:?}");

    let raw = tape_format::reader::RawTape::open(&out).unwrap();
    let tracks = tape_format::tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap()).unwrap();
    assert_eq!(tracks.len(), 12);
    let meta = tape_format::meta::Meta::parse(raw.meta_yaml.as_deref().unwrap()).unwrap();
    assert_eq!(meta.task, "Investigate refund-race for CUST-447139");
    let new_block = meta.new_block.as_ref().unwrap();
    assert_eq!(new_block.template_id, "bug-investigation");
}

#[test]
fn unknown_template_exits_with_new_template_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");
    let res = std::process::Command::new(binary_path())
        .args([
            "new",
            out.to_str().unwrap(),
            "--template",
            "not-a-real-template",
            "--task",
            "x",
        ])
        .output()
        .unwrap();
    assert_eq!(res.status.code(), Some(2), "{res:?}");
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(stderr.contains("NEW_TEMPLATE_NOT_FOUND"), "{stderr}");
    // The diagnostic should include all valid ids so the user can
    // recover without consulting --help.
    assert!(stderr.contains("minimal"), "{stderr}");
    assert!(stderr.contains("test-fixture"), "{stderr}");
    assert!(stderr.contains("bug-investigation"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn default_template_is_minimal() {
    // Backwards-compat guard: omitting --template still resolves to
    // `minimal`, with --task required and the template_id recorded.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.tape");
    let res = std::process::Command::new(binary_path())
        .args([
            "new",
            out.to_str().unwrap(),
            "--task",
            "Smoke the default template",
        ])
        .output()
        .unwrap();
    assert!(res.status.success(), "{res:?}");
    let raw = tape_format::reader::RawTape::open(&out).unwrap();
    let meta = tape_format::meta::Meta::parse(raw.meta_yaml.as_deref().unwrap()).unwrap();
    let new_block = meta.new_block.as_ref().unwrap();
    assert_eq!(new_block.template_id, "minimal");
}
