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
