//! `tape new` + `.taperc::new.default_template` integration coverage.
//! Step-5 of #99 / issue #190. Drives a hermetic `$HOME` so each
//! test owns its `.taperc`.

use std::process::Command;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

/// Run `tape <args>` with `$HOME=<home>` and `current_dir=<home>` so
/// the workspace `.taperc` walk lands inside the caller's tempdir.
fn run(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(binary_path());
    cmd.args(args)
        .env_remove("HOME")
        .env("HOME", home)
        .current_dir(home);
    cmd.output().unwrap()
}

fn template_id_from_meta(path: &std::path::Path) -> String {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    let meta = tape_format::meta::Meta::parse(&raw.meta_yaml.unwrap()).unwrap();
    meta.new_block.unwrap().template_id
}

#[test]
fn taperc_default_template_consumed_when_flag_absent() {
    // AC: with `new.default_template: bug-investigation` in `.taperc`
    // and no `--template` flag, the rendered cassette's
    // `meta.new.template_id` is `bug-investigation` (not the pre-#190
    // implicit `minimal` default).
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "new:\n  default_template: bug-investigation\n",
    )
    .unwrap();
    let out = dir.path().join("cassette.tape");
    let r = run(
        dir.path(),
        &["new", out.to_str().unwrap(), "--task", "hello from taperc"],
    );
    assert!(r.status.success(), "{r:?}");
    assert_eq!(template_id_from_meta(&out), "bug-investigation");
}

#[test]
fn cli_flag_overrides_taperc_default_template() {
    // AC: `--template minimal` overrides a `.taperc` that says
    // `bug-investigation`.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "new:\n  default_template: bug-investigation\n",
    )
    .unwrap();
    let out = dir.path().join("cassette.tape");
    let r = run(
        dir.path(),
        &[
            "new",
            out.to_str().unwrap(),
            "--template",
            "minimal",
            "--task",
            "hello",
        ],
    );
    assert!(r.status.success(), "{r:?}");
    assert_eq!(template_id_from_meta(&out), "minimal");
}

#[test]
fn missing_taperc_falls_through_to_minimal_terminal_default() {
    // Path (b) per #190 ACs: no `.taperc` at all → falls back to
    // `minimal` so pre-#190 invocations stay byte-stable.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("cassette.tape");
    let r = run(
        dir.path(),
        &["new", out.to_str().unwrap(), "--task", "hello"],
    );
    assert!(r.status.success(), "{r:?}");
    assert_eq!(template_id_from_meta(&out), "minimal");
}

#[test]
fn taperc_without_new_section_falls_through_to_minimal() {
    // `.taperc` exists but has no `new:` block — terminal default
    // is `minimal`.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "redact:\n  disable_default: []\n",
    )
    .unwrap();
    let out = dir.path().join("cassette.tape");
    let r = run(
        dir.path(),
        &["new", out.to_str().unwrap(), "--task", "hello"],
    );
    assert!(r.status.success(), "{r:?}");
    assert_eq!(template_id_from_meta(&out), "minimal");
}

#[test]
fn unknown_template_id_in_taperc_exits_two_with_new_template_not_found() {
    // AC: an unknown id resolved via `.taperc` surfaces the same
    // `NEW_TEMPLATE_NOT_FOUND` diagnostic that `--template <unknown>`
    // emits — no parallel error type.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "new:\n  default_template: does-not-exist\n",
    )
    .unwrap();
    let out = dir.path().join("cassette.tape");
    let r = run(
        dir.path(),
        &["new", out.to_str().unwrap(), "--task", "hello"],
    );
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("NEW_TEMPLATE_NOT_FOUND"), "{stderr}");
    assert!(stderr.contains("does-not-exist"), "{stderr}");
}

#[test]
fn typo_under_new_section_exits_two_at_config_parse() {
    // AC: typos under `new:` fail config-load with a clear error.
    let dir = tempfile::tempdir().unwrap();
    let taperc = dir.path().join(".taperc");
    std::fs::write(&taperc, "new:\n  default-template: minimal\n").unwrap();
    let out = dir.path().join("cassette.tape");
    let r = run(
        dir.path(),
        &["new", out.to_str().unwrap(), "--task", "hello"],
    );
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains(taperc.to_string_lossy().as_ref()),
        ".taperc path must appear in diagnostic: {stderr}"
    );
}

#[test]
fn taperc_pointing_at_test_fixture_works_without_task_flag() {
    // The catalog's `test-fixture` template doesn't require `--task`,
    // so a `.taperc` that pins it works without any other args.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".taperc"),
        "new:\n  default_template: test-fixture\n",
    )
    .unwrap();
    let out = dir.path().join("cassette.tape");
    let r = run(dir.path(), &["new", out.to_str().unwrap()]);
    assert!(r.status.success(), "{r:?}");
    assert_eq!(template_id_from_meta(&out), "test-fixture");
}
