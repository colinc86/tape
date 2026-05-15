//! `tape new --set <KEY=VALUE>` Step-4 integration coverage.
//! Issue #188. Drives the binary against `--set` flags and asserts
//! the rendered output, error codes, mutual-exclusion clauses, and
//! determinism property called out in the ticket.

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

fn read_tracks(path: &std::path::Path) -> Vec<tape_format::tracks::Track> {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    let jsonl = raw.tracks_jsonl.unwrap();
    tape_format::tracks::parse_jsonl(&jsonl).unwrap()
}

fn read_meta(path: &std::path::Path) -> tape_format::meta::Meta {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    tape_format::meta::Meta::parse(&raw.meta_yaml.unwrap()).unwrap()
}

const NO_TASK_MARKER: &str = "(no task supplied)";

#[test]
fn set_required_task_false_skips_task_requirement() {
    // AC #1: `--set required-task=false` makes `--task` optional. The
    // generated cassette has empty `meta.task`, the no-task marker in
    // `tracks[0].payload.prompt` (SPEC §5.5.1 forbids empty prompts;
    // see the PR body re: marker-instead-of-empty divergence from
    // the literal AC text), empty `meta.new.placeholders_filled`,
    // and passes `tape verify`.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("scratch.tape");
    let r = run(&[
        "new",
        out.to_str().unwrap(),
        "--template",
        "minimal",
        "--set",
        "required-task=false",
    ]);
    assert!(r.status.success(), "{r:?}");
    assert!(out.exists());

    let tracks = read_tracks(&out);
    assert_eq!(tracks[0].kind, tape_format::tracks::Kind::Task);
    assert_eq!(tracks[0].payload["prompt"], NO_TASK_MARKER);

    let meta = read_meta(&out);
    assert_eq!(meta.task, "");
    assert!(
        meta.new_block
            .as_ref()
            .unwrap()
            .placeholders_filled
            .is_empty(),
        "placeholders_filled should be empty when no --task is supplied: {:?}",
        meta.new_block.as_ref().unwrap().placeholders_filled
    );

    let v = run(&["verify", out.to_str().unwrap()]);
    assert!(v.status.success(), "verify failed: {v:?}");
}

#[test]
fn set_required_task_false_with_task_is_a_noop() {
    // AC #2: when `--task` IS supplied, `--set required-task=false`
    // is a no-op. The output is byte-identical to today's invocation
    // shape.
    let dir = tempfile::tempdir().unwrap();
    let out_a = dir.path().join("a.tape");
    let out_b = dir.path().join("b.tape");
    let common = &[
        "--template",
        "minimal",
        "--task",
        "hello",
        "--created-at",
        "2026-05-15T12:00:00Z",
        "--recorder-agent",
        "test-agent",
    ];
    let mut a_args = vec!["new", out_a.to_str().unwrap()];
    a_args.extend_from_slice(common);
    let mut b_args = vec!["new", out_b.to_str().unwrap()];
    b_args.extend_from_slice(common);
    b_args.push("--set");
    b_args.push("required-task=false");

    let ra = run(&a_args);
    let rb = run(&b_args);
    assert!(ra.status.success(), "{ra:?}");
    assert!(rb.status.success(), "{rb:?}");

    // Compare the three content blobs (skipping zip-level metadata).
    let raw_a = tape_format::reader::RawTape::open(&out_a).unwrap();
    let raw_b = tape_format::reader::RawTape::open(&out_b).unwrap();
    assert_eq!(raw_a.meta_yaml, raw_b.meta_yaml);
    assert_eq!(raw_a.tracks_jsonl, raw_b.tracks_jsonl);
    assert_eq!(raw_a.liner_md, raw_b.liner_md);
}

#[test]
fn set_unknown_key_exits_two_with_diagnostic() {
    // AC #3: unknown key on a template with known keys lists them.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("x.tape");
    let r = run(&[
        "new",
        out.to_str().unwrap(),
        "--template",
        "minimal",
        "--set",
        "foo=bar",
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("NEW_UNKNOWN_OVERRIDE_KEY"), "{stderr}");
    assert!(stderr.contains("\"foo\""), "{stderr}");
    assert!(stderr.contains("\"minimal\""), "{stderr}");
    assert!(stderr.contains("required-task"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn set_unknown_key_on_test_fixture_says_known_none() {
    // AC #4: `test-fixture` has no recognized override keys.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("x.tape");
    let r = run(&[
        "new",
        out.to_str().unwrap(),
        "--template",
        "test-fixture",
        "--set",
        "required-task=true",
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("NEW_UNKNOWN_OVERRIDE_KEY"), "{stderr}");
    assert!(stderr.contains("(known: <none>)"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn set_known_key_bad_value_exits_two() {
    // AC #5: `required-task=maybe` is rejected with an explicit
    // diagnostic naming the expected values.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("x.tape");
    let r = run(&[
        "new",
        out.to_str().unwrap(),
        "--template",
        "minimal",
        "--set",
        "required-task=maybe",
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("required-task"), "{stderr}");
    assert!(
        stderr.contains("'true'") && stderr.contains("'false'"),
        "{stderr}"
    );
}

#[test]
fn set_missing_equals_is_clap_error() {
    // AC #6: `--set required-task` (no `=`) → clap usage error.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("x.tape");
    let r = run(&[
        "new",
        out.to_str().unwrap(),
        "--template",
        "minimal",
        "--set",
        "required-task",
    ]);
    assert!(!r.status.success(), "{r:?}");
}

#[test]
fn set_empty_key_is_clap_error() {
    // AC #7: empty KEY → clap usage error.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("x.tape");
    let r = run(&[
        "new",
        out.to_str().unwrap(),
        "--template",
        "minimal",
        "--set",
        "=false",
    ]);
    assert!(!r.status.success(), "{r:?}");
}

#[test]
fn set_duplicate_key_last_wins_with_warning() {
    // AC #8: duplicate keys → last-wins + stderr warning. The two
    // `required-task` values resolve to `true`, so `--task` is then
    // required; supply it.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("x.tape");
    let r = run(&[
        "new",
        out.to_str().unwrap(),
        "--template",
        "minimal",
        "--set",
        "required-task=false",
        "--set",
        "required-task=true",
        "--task",
        "t",
    ]);
    assert!(r.status.success(), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("required-task specified twice"),
        "expected last-wins warning: {stderr}"
    );
}

#[test]
fn set_conflicts_with_list_templates() {
    // AC #9: `--set` with `--list-templates` → clap mutex error.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("x.tape");
    let r = run(&[
        "new",
        "--list-templates",
        "--set",
        "required-task=false",
        out.to_str().unwrap(),
    ]);
    assert!(!r.status.success(), "{r:?}");
}

#[test]
fn set_conflicts_with_describe_template() {
    let r = run(&[
        "new",
        "--describe-template",
        "minimal",
        "--set",
        "required-task=false",
    ]);
    assert!(!r.status.success(), "{r:?}");
}

#[test]
fn set_no_op_when_absent_is_byte_identical_minimal() {
    // AC #10: absence of `--set` produces byte-identical output to
    // the pre-#188 invocation. We test by running the same command
    // twice (one with no `--set`, one as a reference). Both
    // invocations pin `--created-at` + `--recorder-agent` so the
    // derived UUID + timestamps match.
    let dir = tempfile::tempdir().unwrap();
    let out_a = dir.path().join("a.tape");
    let out_b = dir.path().join("b.tape");
    for (name, p) in [("a", &out_a), ("b", &out_b)] {
        let r = run(&[
            "new",
            p.to_str().unwrap(),
            "--template",
            "minimal",
            "--task",
            "hello",
            "--created-at",
            "2026-05-15T12:00:00Z",
            "--recorder-agent",
            "test-agent",
        ]);
        assert!(r.status.success(), "{name}: {r:?}");
    }
    let raw_a = tape_format::reader::RawTape::open(&out_a).unwrap();
    let raw_b = tape_format::reader::RawTape::open(&out_b).unwrap();
    assert_eq!(raw_a.meta_yaml, raw_b.meta_yaml);
    assert_eq!(raw_a.tracks_jsonl, raw_b.tracks_jsonl);
    assert_eq!(raw_a.liner_md, raw_b.liner_md);
}

#[test]
fn set_required_task_false_is_deterministic() {
    // AC #11: two runs with pinned `--created-at` + `--recorder-agent`
    // produce byte-identical cassette content (sans zip metadata).
    let dir = tempfile::tempdir().unwrap();
    let out_a = dir.path().join("a.tape");
    let out_b = dir.path().join("b.tape");
    for p in [&out_a, &out_b] {
        let r = run(&[
            "new",
            p.to_str().unwrap(),
            "--template",
            "minimal",
            "--set",
            "required-task=false",
            "--created-at",
            "2026-05-15T12:00:00Z",
            "--recorder-agent",
            "test-agent",
        ]);
        assert!(r.status.success(), "{r:?}");
    }
    let raw_a = tape_format::reader::RawTape::open(&out_a).unwrap();
    let raw_b = tape_format::reader::RawTape::open(&out_b).unwrap();
    assert_eq!(
        raw_a.meta_yaml, raw_b.meta_yaml,
        "meta.yaml should be byte-identical across runs with pinned inputs"
    );
    assert_eq!(raw_a.tracks_jsonl, raw_b.tracks_jsonl);
    assert_eq!(raw_a.liner_md, raw_b.liner_md);
}
