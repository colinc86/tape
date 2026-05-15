//! `tape tag` Step-1 integration coverage. Tracks Principal's scoping
//! comment on #93: `--add` / `--remove` / `--list` only, plus
//! `-o` / `--in-place` / `--dry-run`. The audit-trail, count/length
//! caps, `tape verify` constraint additions, closed-vocab `--verify`,
//! `.taperc::tag:` section, and `--auto` mode are Step 2–5 and have
//! their own follow-on tickets.

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

/// Copy `minimal-success.tape` into a fresh temp dir so each test's
/// input is isolated.
fn isolated_minimal() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &dst).unwrap();
    (dir, dst)
}

fn read_meta(path: &std::path::Path) -> tape_format::meta::Meta {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    let yaml = raw.meta_yaml.unwrap();
    tape_format::meta::Meta::parse(&yaml).unwrap()
}

#[test]
fn add_writes_tag_to_meta() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("input.tagged.tape");

    let out = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "tape tag --add failed: {out:?}");

    let meta = read_meta(&output);
    assert_eq!(meta.tags, vec!["bug-fix".to_owned()]);

    // Output passes `tape verify`.
    let v = Command::new(binary_path())
        .args(["verify", output.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(v.status.success(), "verify failed: {v:?}");
}

#[test]
fn add_multiple_tags_in_argv_order() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("input.tagged.tape");

    let out = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "--add",
            "auth",
            "--add",
            "regression-baseline",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let meta = read_meta(&output);
    assert_eq!(
        meta.tags,
        vec![
            "bug-fix".to_owned(),
            "auth".to_owned(),
            "regression-baseline".to_owned()
        ]
    );
}

#[test]
fn add_idempotent_no_duplicate_in_meta() {
    // Re-adding the same tag twice produces one entry in meta.tags.
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("input.tagged.tape");

    let out = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "--add",
            "bug-fix",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let meta = read_meta(&output);
    assert_eq!(meta.tags, vec!["bug-fix".to_owned()]);
}

#[test]
fn add_existing_tag_is_no_op() {
    // First add bug-fix → output has it. Second add against that output
    // produces no diff → no new cassette written, stderr carries
    // TAG_NO_CHANGE.
    let (_dir, input) = isolated_minimal();
    let after_first = input.with_file_name("input.tagged.tape");
    let after_second = input.with_file_name("retry.tagged.tape");

    let first = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "-o",
            after_first.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(first.status.success(), "{first:?}");
    assert!(after_first.exists());

    let second = Command::new(binary_path())
        .args([
            "tag",
            after_first.to_str().unwrap(),
            "--add",
            "bug-fix",
            "-o",
            after_second.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(second.status.success(), "no-op should still exit 0");
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains("TAG_NO_CHANGE"),
        "stderr should advertise the no-op: {stderr}"
    );
    assert!(
        !after_second.exists(),
        "no-op must not write an output cassette"
    );
}

#[test]
fn remove_drops_tag_from_meta() {
    // Seed two tags, remove one.
    let (_dir, input) = isolated_minimal();
    let seeded = input.with_file_name("seeded.tape");
    let pruned = input.with_file_name("pruned.tape");

    Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "--add",
            "auth",
            "-o",
            seeded.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let seeded_meta = read_meta(&seeded);
    assert_eq!(seeded_meta.tags.len(), 2);

    let out = Command::new(binary_path())
        .args([
            "tag",
            seeded.to_str().unwrap(),
            "--remove",
            "auth",
            "-o",
            pruned.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let pruned_meta = read_meta(&pruned);
    assert_eq!(pruned_meta.tags, vec!["bug-fix".to_owned()]);
}

#[test]
fn remove_absent_tag_is_no_op() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("noop.tape");

    let out = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--remove",
            "never-tagged",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("TAG_NO_CHANGE"), "{stderr}");
    assert!(!output.exists());
}

#[test]
fn add_and_remove_compose_in_single_call() {
    // Seed [a, b], then call --add c --remove a in one shot. Expect [b, c].
    let (_dir, input) = isolated_minimal();
    let seeded = input.with_file_name("seeded.tape");
    let mixed = input.with_file_name("mixed.tape");

    Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "a",
            "--add",
            "b",
            "-o",
            seeded.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let out = Command::new(binary_path())
        .args([
            "tag",
            seeded.to_str().unwrap(),
            "--add",
            "c",
            "--remove",
            "a",
            "-o",
            mixed.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let meta = read_meta(&mixed);
    assert_eq!(meta.tags, vec!["b".to_owned(), "c".to_owned()]);
}

#[test]
fn list_prints_tags_one_per_line() {
    let (_dir, input) = isolated_minimal();
    let seeded = input.with_file_name("seeded.tape");

    Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "--add",
            "auth",
            "-o",
            seeded.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let out = Command::new(binary_path())
        .args(["tag", seeded.to_str().unwrap(), "--list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "bug-fix\nauth");
}

#[test]
fn list_on_untagged_cassette_exits_zero_with_empty_stdout() {
    // Unlike `tape recap --list` (which exits 4 when meta.recap is None),
    // `tape tag --list` exits 0 for the empty plural-by-default field.
    let (_dir, input) = isolated_minimal();

    let out = Command::new(binary_path())
        .args(["tag", input.to_str().unwrap(), "--list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().is_empty(),
        "expected empty stdout, got {stdout:?}"
    );
}

#[test]
fn dry_run_prints_diff_and_exits_4_without_writing() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("never-written.tape");

    let out = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "--dry-run",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("prior:"), "{stdout}");
    assert!(stdout.contains("next:"), "{stdout}");
    assert!(stdout.contains("added: bug-fix"), "{stdout}");
    assert!(!output.exists(), "dry-run must not produce a cassette");
}

#[test]
fn in_place_replaces_input_atomically() {
    // The temp + rename writer is reused; the test asserts the
    // post-condition: same path, new tags.
    let (_dir, input) = isolated_minimal();

    let out = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "--in-place",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let meta = read_meta(&input);
    assert_eq!(meta.tags, vec!["bug-fix".to_owned()]);
}

#[test]
fn empty_add_value_exits_2() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("never-written.tape");

    let out = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(!output.exists());
}

#[test]
fn no_mode_flag_exits_2() {
    let (_dir, input) = isolated_minimal();
    let out = Command::new(binary_path())
        .args(["tag", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

#[test]
fn list_conflicts_with_add_exits_nonzero() {
    let (_dir, input) = isolated_minimal();
    let out = Command::new(binary_path())
        .args(["tag", input.to_str().unwrap(), "--list", "--add", "bug-fix"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn out_equal_to_input_without_in_place_exits_2() {
    let (_dir, input) = isolated_minimal();
    let out = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "-o",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--in-place"),
        "stderr should hint: {stderr}"
    );
}

#[test]
fn in_place_conflicts_with_out_exits_nonzero() {
    let (_dir, input) = isolated_minimal();
    let elsewhere = input.with_file_name("else.tape");
    let out = Command::new(binary_path())
        .args([
            "tag",
            input.to_str().unwrap(),
            "--add",
            "bug-fix",
            "--in-place",
            "-o",
            elsewhere.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(out.status.code(), Some(2));
}
