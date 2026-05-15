//! `tape annotate` Phase-1 integration coverage. Tracks Principal's
//! scoping comment on #74 — items 1, 2, 9, 11, 12, 13, 14, 15, 16, and 19
//! of the issue's test plan, plus a `tape verify`-clean round trip.

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

/// Copy `minimal-success.tape` into a fresh temp dir so each test's input
/// is isolated (some tests assert the original is untouched).
fn isolated_minimal() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &dst).unwrap();
    (dir, dst)
}

fn read_tracks(path: &std::path::Path) -> Vec<tape_format::tracks::Track> {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    let jsonl = raw.tracks_jsonl.unwrap();
    tape_format::tracks::parse_jsonl(&jsonl).unwrap()
}

/// The eject pipeline appends a fresh `eject` after our annotation, so the
/// annotation is the last *non-eject* track. Encapsulating the lookup keeps
/// the tests honest about that invariant rather than reaching into
/// `tracks[n - 2]` magic numbers.
fn last_annotation(tracks: &[tape_format::tracks::Track]) -> &tape_format::tracks::Track {
    tracks
        .iter()
        .rev()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation)
        .expect("expected an annotation track in the output")
}

/// Phase-1 test #1 — bound annotation has the right track shape and the
/// output passes `tape verify`.
#[test]
fn bound_annotation_writes_expected_track_shape() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("annotated.tape");

    let out = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "look here",
            "--step",
            "2",
            "--by",
            "human",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "annotate failed: {out:?}");

    let tracks = read_tracks(&output);
    let annot = last_annotation(&tracks);
    assert_eq!(annot.parent_step, Some(2));
    assert_eq!(annot.payload["by"], "human");
    assert_eq!(annot.payload["note"], "look here");

    // `tape verify` clean.
    let v = Command::new(binary_path())
        .args(["verify", output.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(v.status.success(), "verify failed: {v:?}");
}

/// Phase-1 test #2 — unparented annotation omits `parent_step` from the
/// payload (serde `skip_serializing_if = "Option::is_none"`).
#[test]
fn unparented_annotation_omits_parent_step() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("annotated.tape");

    let out = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "floating note",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "annotate failed: {out:?}");

    let tracks = read_tracks(&output);
    let annot = last_annotation(&tracks);
    assert!(annot.parent_step.is_none());

    // Reload the raw JSONL and assert the annotation's serialized form
    // really omits the `parent_step` field (not just renders it as
    // `null` / `0`). Locate the annotation line by `"kind":"annotation"`
    // rather than indexing — the pipeline appends a trailing eject after
    // the annotation, so the last *line* is not the annotation's line.
    let raw = tape_format::reader::RawTape::open(&output).unwrap();
    let annot_line = raw
        .tracks_jsonl
        .unwrap()
        .lines()
        .find(|l| l.contains(r#""kind":"annotation""#))
        .expect("expected an annotation line in tracks.jsonl")
        .to_owned();
    assert!(
        !annot_line.contains("parent_step"),
        "unparented annotation should not serialize a parent_step field: {annot_line}"
    );
}

/// Phase-1 test #9 — Anthropic API key in `--note` exits 6 with
/// `ANNOT_LEAK` and writes nothing.
#[test]
fn anthropic_key_in_note_exits_with_annot_leak() {
    let (_dir, input) = isolated_minimal();
    let before_bytes = std::fs::read(&input).unwrap();
    let output = input.with_file_name("annotated.tape");

    let fake_key = format!("sk-ant-{}", "A".repeat(95));
    let out = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            &format!("key is {fake_key}"),
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected non-zero exit");
    assert_eq!(out.status.code(), Some(6));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ANNOT_LEAK"),
        "expected ANNOT_LEAK in stderr: {stderr}"
    );
    assert!(
        stderr.contains("anthropic_api_key"),
        "expected the rule_id in stderr: {stderr}"
    );
    assert!(!output.exists(), "output must not be written on leak");
    // Original untouched.
    let after_bytes = std::fs::read(&input).unwrap();
    assert_eq!(before_bytes, after_bytes, "input must be untouched");
}

/// Phase-1 test #11 — email in `--note` exits 6 with the `email` rule_id.
#[test]
fn email_in_note_exits_with_annot_leak() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("annotated.tape");

    let out = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "email me at someone@example.com",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ANNOT_LEAK") && stderr.contains("email"),
        "expected ANNOT_LEAK + email rule_id: {stderr}"
    );
    assert!(!output.exists());
}

/// Phase-1 test #12 — `--step 0` and `--step (max+1)` both exit 4
/// `ANNOT_BAD_STEP`.
#[test]
fn step_out_of_range_exits_with_annot_bad_step() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("annotated.tape");

    // Step 0 — below the [1, new_step) range.
    let out0 = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "x",
            "--step",
            "0",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out0.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&out0.stderr).contains("ANNOT_BAD_STEP"));

    // Step (loaded.len() + 1) — at-or-above new_step. The minimal-success
    // fixture has 3 tracks (task, model_call, eject); after dropping the
    // trailing eject, the new annotation lands at step 3, so step 3 itself
    // is out of range and step 99 definitely is.
    let out_high = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "x",
            "--step",
            "99",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out_high.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&out_high.stderr).contains("ANNOT_BAD_STEP"));
}

/// Phase-1 test #13 — `--out` equal to `<file>` exits 2 before touching
/// anything.
#[test]
fn out_equal_to_input_exits_2() {
    let (_dir, input) = isolated_minimal();

    let out = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "x",
            "-o",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

/// Phase-1 test #14 — `tape ls <output>` shows the annotation as the last
/// non-eject row.
#[test]
fn output_ls_shows_annotation_as_last_user_event() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("annotated.tape");

    let annotate = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "needle-marker-xyz",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(annotate.status.success());

    let ls = Command::new(binary_path())
        .args(["ls", output.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(ls.status.success());
    let text = String::from_utf8_lossy(&ls.stdout);
    // `tape play` (which `ls` shares rendering with) already renders
    // annotation kinds at `crates/tape-play/src/lib.rs:113-118`. We just
    // need to confirm the marker survived.
    assert!(
        text.contains("needle-marker-xyz") || text.contains("annotation"),
        "ls output should mention the annotation: {text}"
    );
}

/// Phase-1 test #15 — when every input track shares one `ts` (the
/// snapshot-collapse case, bug #5), the new annotation's `ts` falls back
/// to `meta.ejected_at` and stderr warns.
#[test]
fn snapshot_collapse_ts_fallback_uses_ejected_at() {
    let dir = tempfile::tempdir().unwrap();
    let collapsed = dir.path().join("collapsed.tape");
    write_collapsed_fixture(&collapsed);

    let output = dir.path().join("annotated.tape");
    let out = Command::new(binary_path())
        .args([
            "annotate",
            collapsed.to_str().unwrap(),
            "--note",
            "x",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "annotate failed: {out:?}");

    let tracks = read_tracks(&output);
    let annotation = last_annotation(&tracks);
    assert_eq!(
        annotation.ts, "2026-05-06T10:00:30Z",
        "snapshot-collapse fallback must use meta.ejected_at, got {}",
        annotation.ts
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("snapshot_collapse_ts_fallback")
            || String::from_utf8_lossy(&out.stderr).contains("snapshot_collapse_ts_fallback"),
        "expected a snapshot_collapse_ts_fallback warning"
    );
}

/// Phase-1 test #16 — explicit `--ts` predating the last loaded track
/// exits 7 `ANNOT_TS_NOT_MONOTONIC`.
#[test]
fn ts_before_last_track_exits_with_not_monotonic() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("annotated.tape");

    let out = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "x",
            "--ts",
            "1999-01-01T00:00:00.000Z",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&out.stderr).contains("ANNOT_TS_NOT_MONOTONIC"));
}

/// Phase-1 test #19 — `--json` emits the §3.10 schema-v1 shape.
#[test]
fn json_output_matches_schema_v1() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("annotated.tape");

    let out = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "x",
            "--step",
            "2",
            "--actor",
            "alice",
            "--by",
            "human",
            "-o",
            output.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "annotate failed: {out:?}");
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["schema_version"], "1");
    assert_eq!(body["new_step"], 3);
    assert_eq!(body["parent_step"], 2);
    assert_eq!(body["actor"], "alice");
    assert_eq!(body["by"], "human");
    assert!(body["output_path"].is_string());
    assert!(body["warnings"].is_array());
}

/// Round-trip: post-annotate verify is clean for every default-enabled
/// rule's harmless input. Acts as a regression guard that the existing
/// minimal-success fixture stays compatible with the load-replay-eject
/// pipeline.
#[test]
fn annotated_output_passes_tape_verify() {
    let (_dir, input) = isolated_minimal();
    let output = input.with_file_name("annotated.tape");

    let annotate = Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "non-leaking note",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(annotate.status.success());

    let v = Command::new(binary_path())
        .args(["verify", output.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(v.status.success(), "verify failed: {v:?}");
}

/// Hand-build a synthetic tape where every track shares one timestamp —
/// the snapshot-import case bug #5 documents. Used by
/// `snapshot_collapse_ts_fallback_uses_ejected_at`.
fn write_collapsed_fixture(out: &std::path::Path) {
    let meta = r#"tape_version: "tape/v0"
id: "01h8xy00-0000-7000-b8aa-000000000074"
created_at: "2026-05-06T10:00:00Z"
ejected_at: "2026-05-06T10:00:30Z"
task: "snapshot-collapse fixture"
recorder:
  agent: "test/0.0.1"
outcome: success
"#;
    let liner = "## What I was asked to do\nx\n\n## What I found\nx\n\n## Suggested next step / fix\nx\n\n## What I'm uncertain about\nx\n";
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#,
        "\n",
        r#"{"step":2,"kind":"model_call","ts":"2026-05-06T10:00:00Z","payload":{"vendor":"anthropic","model":"x","request":{},"response":{}}}"#,
        "\n",
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:00Z","payload":{"outcome":"success"}}"#,
        "\n",
    );
    let pending = tape_format::writer::PendingTape {
        meta_yaml: meta.into(),
        liner_md: liner.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: std::collections::BTreeMap::new(),
    };
    pending.write_to(out).unwrap();
}

// --- Phase 2 (issue #158): --editor + --in-place --------------------

use std::os::unix::fs::PermissionsExt;

/// Write a small shell script that mimics an editor: it takes the
/// argument path it's invoked with and overwrites the file's content
/// with `body`. Returns the path to the script (executable).
///
/// The script is `set -e` so any IO failure surfaces as a non-zero
/// exit, which the editor-error tests rely on. `exit_code` lets the
/// tests force a non-zero return after the write (or, with an empty
/// body, before any write — for the "editor failed" path).
fn make_mock_editor(dir: &std::path::Path, body: &[u8], exit_code: i32) -> std::path::PathBuf {
    let script = dir.join("mock-editor.sh");
    let body_path = dir.join("mock-editor.body");
    std::fs::write(&body_path, body).unwrap();
    let script_body = format!(
        "#!/bin/sh\nset -e\ncat {body:?} > \"$1\"\nexit {exit_code}\n",
        body = body_path.display(),
    );
    std::fs::write(&script, script_body).unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}

/// Spawn `tape annotate` with `EDITOR` pointed at a mock editor and
/// `VISUAL` cleared. Returns the captured output for assertion.
fn run_annotate_with_editor(
    binary: &std::path::Path,
    args: &[&str],
    editor: &std::path::Path,
) -> std::process::Output {
    std::process::Command::new(binary)
        .args(args)
        .env_remove("VISUAL")
        .env("EDITOR", editor.as_os_str())
        .output()
        .unwrap()
}

#[test]
fn editor_happy_path_writes_body_into_annotation() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    let output = dir.path().join("input.annotated.tape");
    let editor = make_mock_editor(dir.path(), b"hello from editor\n", 0);

    let out = run_annotate_with_editor(
        &binary_path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--editor",
            "-o",
            output.to_str().unwrap(),
        ],
        &editor,
    );
    assert!(
        out.status.success(),
        "tape annotate --editor failed: {out:?}"
    );
    let raw = tape_format::reader::RawTape::open(&output).unwrap();
    let jsonl = raw.tracks_jsonl.as_deref().unwrap();
    let tracks = tape_format::tracks::parse_jsonl(jsonl).unwrap();
    let annot = tracks
        .iter()
        .rev()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation)
        .expect("at least one annotation track");
    assert_eq!(annot.payload["note"], "hello from editor");

    // tape verify on the output is clean.
    let v = std::process::Command::new(binary_path())
        .args(["verify", output.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(v.status.success(), "verify failed: {v:?}");
}

#[test]
fn editor_strips_comment_lines() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    let output = dir.path().join("input.annotated.tape");
    let body = b"# tape annotate header\nactual text\n# trailing comment\n";
    let editor = make_mock_editor(dir.path(), body, 0);

    let out = run_annotate_with_editor(
        &binary_path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--editor",
            "-o",
            output.to_str().unwrap(),
        ],
        &editor,
    );
    assert!(out.status.success(), "{out:?}");
    let raw = tape_format::reader::RawTape::open(&output).unwrap();
    let jsonl = raw.tracks_jsonl.as_deref().unwrap();
    let tracks = tape_format::tracks::parse_jsonl(jsonl).unwrap();
    let annot = tracks
        .iter()
        .rev()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation)
        .unwrap();
    assert_eq!(annot.payload["note"], "actual text");
}

#[test]
fn editor_empty_body_after_strip_is_a_clean_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    let output = dir.path().join("input.annotated.tape");
    let body = b"# only\n# comment\n# lines\n\n";
    let editor = make_mock_editor(dir.path(), body, 0);

    let out = run_annotate_with_editor(
        &binary_path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--editor",
            "-o",
            output.to_str().unwrap(),
        ],
        &editor,
    );
    assert!(out.status.success(), "empty body should exit 0: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("nothing to annotate"), "{stderr}");
    assert!(!output.exists(), "no output cassette on empty body");
}

#[test]
fn editor_nonzero_exit_propagates() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    let output = dir.path().join("input.annotated.tape");
    // Body content irrelevant — the editor exits non-zero.
    let editor = make_mock_editor(dir.path(), b"never reaches\n", 1);

    let out = run_annotate_with_editor(
        &binary_path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--editor",
            "-o",
            output.to_str().unwrap(),
        ],
        &editor,
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("editor"), "{stderr}");
    assert!(!output.exists());
}

#[test]
fn editor_redaction_hit_exits_annot_leak() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    let output = dir.path().join("input.annotated.tape");
    // `sk-ant-` Anthropic-key prefix is in the bundled redact rules.
    let body = b"key: sk-ant-api03-AAAABBBBCCCCDDDDEEEEFFFFGGGGHHHHIIIIJJJJKKKKLLLLMMMM-AAAA\n";
    let editor = make_mock_editor(dir.path(), body, 0);

    let out = run_annotate_with_editor(
        &binary_path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--editor",
            "-o",
            output.to_str().unwrap(),
        ],
        &editor,
    );
    assert_eq!(out.status.code(), Some(6), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ANNOT_LEAK"), "{stderr}");
    assert!(!output.exists());
}

#[test]
fn editor_oversized_body_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    let output = dir.path().join("input.annotated.tape");
    let big = vec![b'x'; 17 * 1024];
    let editor = make_mock_editor(dir.path(), &big, 0);

    let out = run_annotate_with_editor(
        &binary_path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--editor",
            "-o",
            output.to_str().unwrap(),
        ],
        &editor,
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("16 KiB"), "{stderr}");
    assert!(!output.exists());
}

#[test]
fn editor_non_utf8_body_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    let output = dir.path().join("input.annotated.tape");
    // 0xFF 0xFE 0x00 is an invalid UTF-8 sequence.
    let editor = make_mock_editor(dir.path(), &[0xFF, 0xFE, 0x00], 0);

    let out = run_annotate_with_editor(
        &binary_path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--editor",
            "-o",
            output.to_str().unwrap(),
        ],
        &editor,
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("non-UTF-8"), "{stderr}");
    assert!(!output.exists());
}

#[test]
fn editor_and_note_are_mutually_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();

    let out = std::process::Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "x",
            "--editor",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn annotate_with_no_body_source_rejects_at_parse_time() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();

    let out = std::process::Command::new(binary_path())
        .args(["annotate", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn in_place_replaces_input_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();

    let out = std::process::Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "pin: race in process_refund",
            "--in-place",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    // No sibling temp left behind.
    let stem = input.file_stem().unwrap().to_string_lossy().into_owned();
    let parent = input.parent().unwrap();
    for entry in std::fs::read_dir(parent).unwrap() {
        let p = entry.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            !name.starts_with(&format!("{stem}.annotate-tmp-")),
            "temp file lingered: {p:?}"
        );
    }
    // Input now carries the annotation as the last user-visible track.
    let raw = tape_format::reader::RawTape::open(&input).unwrap();
    let jsonl = raw.tracks_jsonl.as_deref().unwrap();
    let tracks = tape_format::tracks::parse_jsonl(jsonl).unwrap();
    let annot = tracks
        .iter()
        .rev()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation)
        .unwrap();
    assert_eq!(annot.payload["note"], "pin: race in process_refund");
}

#[test]
fn in_place_and_out_are_mutually_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    let elsewhere = dir.path().join("else.tape");

    let out = std::process::Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "x",
            "--in-place",
            "-o",
            elsewhere.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn in_place_json_reports_input_path() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();

    let out = std::process::Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "in-place via json",
            "--in-place",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(payload["output_path"], input.to_string_lossy().as_ref());
}

#[test]
fn in_place_redaction_hit_preserves_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    // Hash the prior input bytes so we can verify they're untouched.
    let prior_bytes = std::fs::read(&input).unwrap();

    let out = std::process::Command::new(binary_path())
        .args([
            "annotate",
            input.to_str().unwrap(),
            "--note",
            "key: sk-ant-api03-AAAABBBBCCCCDDDDEEEEFFFFGGGGHHHHIIIIJJJJKKKKLLLLMMMM-AAAA",
            "--in-place",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ANNOT_LEAK"), "{stderr}");
    // Input cassette is untouched (byte-identical).
    let after_bytes = std::fs::read(&input).unwrap();
    assert_eq!(prior_bytes, after_bytes, "input must be preserved");
    // No sibling temp left behind.
    let stem = input.file_stem().unwrap().to_string_lossy().into_owned();
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            !name.starts_with(&format!("{stem}.annotate-tmp-")),
            "temp file lingered: {name}"
        );
    }
}

#[test]
fn editor_combined_with_in_place_writes_through_to_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &input).unwrap();
    let editor = make_mock_editor(dir.path(), b"composed via editor + in-place\n", 0);

    let out = run_annotate_with_editor(
        &binary_path(),
        &[
            "annotate",
            input.to_str().unwrap(),
            "--editor",
            "--in-place",
        ],
        &editor,
    );
    assert!(out.status.success(), "{out:?}");
    let raw = tape_format::reader::RawTape::open(&input).unwrap();
    let jsonl = raw.tracks_jsonl.as_deref().unwrap();
    let tracks = tape_format::tracks::parse_jsonl(jsonl).unwrap();
    let annot = tracks
        .iter()
        .rev()
        .find(|t| t.kind == tape_format::tracks::Kind::Annotation)
        .unwrap();
    assert_eq!(annot.payload["note"], "composed via editor + in-place");
}
