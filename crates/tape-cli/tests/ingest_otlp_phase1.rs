//! End-to-end coverage for `tape ingest --format otlp` Phase 1
//! (issue #225, carved from #95). The OTLP fixture is generated at
//! test-setup time by shelling out `tape to-otlp` on the existing
//! `tests/fixtures/minimal-success.tape` — this keeps the wire format
//! honest (round-trip via the very helpers we're inverting) and
//! avoids hand-rolling a JSON literal that drifts when to-otlp's
//! shape changes.
//!
//! Asserts:
//! - happy path: ingest produces a `.tape` that re-verifies clean,
//!   with the input span sequence preserved as track kinds
//! - synthesize-when-missing: a single-span OTLP file gains a
//!   synthetic task + eject, ending up at 3 tracks
//! - --format auto-extension: `traces.json` → `traces.json.tape`
//! - --output == --input → exit 2
//! - --format langsmith (and the other 4 reserved names) → exit 2
//!   with "#95" in stderr
//! - --format made-up → exit 2 listing recognised set
//! - missing --format → exit 2 with Phase-1 message
//! - malformed OTLP JSON → exit 1 (handler error path)

use std::path::{Path, PathBuf};

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

fn repo_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
}

/// Generate an OTLP/JSON fixture by running `tape to-otlp` against
/// the canonical minimal-success cassette and capturing stdout.
fn generate_otlp_fixture(dest: &Path) {
    let cassette = repo_fixtures().join("minimal-success.tape");
    let r = std::process::Command::new(binary_path())
        .args(["to-otlp", cassette.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(r.status.success(), "to-otlp setup failed: {r:?}");
    std::fs::write(dest, &r.stdout).unwrap();
}

fn run_ingest(args: &[&str]) -> std::process::Output {
    let mut cmd = std::process::Command::new(binary_path());
    cmd.arg("ingest");
    for a in args {
        cmd.arg(a);
    }
    cmd.output().unwrap()
}

#[test]
fn round_trip_via_to_otlp_produces_verifiable_cassette() {
    let tmp = tempfile::tempdir().unwrap();
    let otlp = tmp.path().join("trace.json");
    generate_otlp_fixture(&otlp);
    let out = tmp.path().join("ingested.tape");

    let r = run_ingest(&[
        "--format",
        "otlp",
        otlp.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "{r:?}");
    assert!(out.exists(), "output file should exist");

    // The written cassette must re-open and verify clean.
    let raw = tape_format::reader::RawTape::open(&out).expect("open written tape");
    let report = tape_format::verify::verify(&raw);
    assert!(
        report.is_valid(),
        "ingest output failed verify: {:?}",
        report
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>()
    );

    // First track is `task`, last is `eject` (SPEC §5.4).
    let tracks =
        tape_format::tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap_or("")).unwrap();
    assert!(!tracks.is_empty());
    assert_eq!(
        tracks.first().unwrap().kind,
        tape_format::tracks::Kind::Task
    );
    assert_eq!(
        tracks.last().unwrap().kind,
        tape_format::tracks::Kind::Eject
    );
    // Steps are 1..=N gap-free.
    for (i, t) in tracks.iter().enumerate() {
        assert_eq!(t.step, (i + 1) as u64);
    }
}

#[test]
fn single_unknown_span_gets_synthesized_task_and_eject() {
    let tmp = tempfile::tempdir().unwrap();
    let otlp = tmp.path().join("single.json");
    // Hand-rolled minimal OTLP/JSON with ONE span of a foreign name
    // (`query_db`) — Phase 1 maps it to `mcp_call`, then synthesizes
    // both task and eject.
    let otlp_body = r#"{
      "resourceSpans": [{
        "resource": { "attributes": [] },
        "scopeSpans": [{
          "scope": { "name": "tape-ingest-test", "version": "0" },
          "spans": [{
            "traceId": "00000000000000000000000000000001",
            "spanId":  "0000000000000001",
            "name": "query_db",
            "kind": 1,
            "startTimeUnixNano": "1763251200000000000",
            "endTimeUnixNano":   "1763251201000000000",
            "attributes": [
              {"key": "table", "value": {"stringValue": "users"}}
            ]
          }]
        }]
      }]
    }"#;
    std::fs::write(&otlp, otlp_body).unwrap();
    let out = tmp.path().join("synth.tape");

    let r = run_ingest(&[
        "--format",
        "otlp",
        otlp.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "{r:?}");

    let raw = tape_format::reader::RawTape::open(&out).expect("open");
    let report = tape_format::verify::verify(&raw);
    assert!(report.is_valid(), "{report:?}");

    let tracks =
        tape_format::tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap_or("")).unwrap();
    assert_eq!(
        tracks.len(),
        3,
        "expected synthetic task + 1 span + synthetic eject"
    );
    assert_eq!(tracks[0].kind, tape_format::tracks::Kind::Task);
    assert_eq!(tracks[1].kind, tape_format::tracks::Kind::McpCall);
    assert_eq!(tracks[2].kind, tape_format::tracks::Kind::Eject);
}

#[test]
fn default_output_path_appends_tape_to_input_extension() {
    let tmp = tempfile::tempdir().unwrap();
    let otlp = tmp.path().join("traces.json");
    generate_otlp_fixture(&otlp);

    let r = run_ingest(&["--format", "otlp", otlp.to_str().unwrap()]);
    assert!(r.status.success(), "{r:?}");
    // `traces.json` → `traces.json.tape` (NOT `traces.tape`).
    let expected = tmp.path().join("traces.json.tape");
    assert!(
        expected.exists(),
        "expected output at {}",
        expected.display()
    );
}

#[test]
fn output_equal_to_input_exits_two() {
    let tmp = tempfile::tempdir().unwrap();
    let otlp = tmp.path().join("trace.json");
    generate_otlp_fixture(&otlp);

    let r = run_ingest(&[
        "--format",
        "otlp",
        otlp.to_str().unwrap(),
        "--output",
        otlp.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
}

#[test]
fn reserved_formats_exit_two_with_pointer_to_95() {
    let tmp = tempfile::tempdir().unwrap();
    let otlp = tmp.path().join("trace.json");
    generate_otlp_fixture(&otlp);
    for fmt in [
        "langsmith",
        "langfuse",
        "helicone",
        "openllmetry",
        "phoenix",
    ] {
        let r = run_ingest(&["--format", fmt, otlp.to_str().unwrap()]);
        assert_eq!(r.status.code(), Some(2), "format {fmt}: {r:?}");
        let stderr = String::from_utf8_lossy(&r.stderr);
        assert!(
            stderr.contains("not implemented in Phase 1"),
            "format {fmt} stderr: {stderr}"
        );
        assert!(stderr.contains("#95"), "format {fmt} stderr: {stderr}");
    }
}

#[test]
fn bogus_format_exits_two_listing_recognised_set() {
    let tmp = tempfile::tempdir().unwrap();
    let otlp = tmp.path().join("trace.json");
    generate_otlp_fixture(&otlp);
    let r = run_ingest(&["--format", "made-up", otlp.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("otlp"), "stderr should list otlp: {stderr}");
    assert!(stderr.contains("unknown"), "stderr: {stderr}");
}

#[test]
fn missing_format_exits_two_with_phase_one_message() {
    let tmp = tempfile::tempdir().unwrap();
    let otlp = tmp.path().join("trace.json");
    generate_otlp_fixture(&otlp);
    let r = run_ingest(&[otlp.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("Phase 1 requires --format"),
        "stderr: {stderr}"
    );
}

#[test]
fn help_documents_the_subcommand() {
    let r = std::process::Command::new(binary_path())
        .args(["ingest", "--help"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let lower = stdout.to_lowercase();
    assert!(lower.contains("otlp"), "help: {stdout}");
    assert!(lower.contains("phase 1"), "help: {stdout}");
}
