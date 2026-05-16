//! End-to-end integration coverage for `tape to-otlp` Phase 1 (issue
//! #88, carved per #209). Asserts the OTLP/JSON shape, structural
//! invariants (AC #2, #3), and the `--output` / refusal paths.

use serde_json::Value;

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

fn run_to_otlp(input: &std::path::Path) -> std::process::Output {
    std::process::Command::new(binary_path())
        .args(["to-otlp", input.to_str().unwrap()])
        .output()
        .unwrap()
}

fn parse_export(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).expect("stdout is valid JSON")
}

#[test]
fn happy_path_minimal_success_emits_valid_otlp_json() {
    // AC #1 + #2 — the OTLP/JSON shape is right, and the span count
    // matches the parsed track count.
    let r = run_to_otlp(&fixture("minimal-success.tape"));
    assert!(r.status.success(), "tape to-otlp failed: {r:?}");

    let doc = parse_export(&r.stdout);
    let resource_spans = doc["resourceSpans"]
        .as_array()
        .expect("resourceSpans is an array");
    assert_eq!(resource_spans.len(), 1);

    let resource = &resource_spans[0]["resource"];
    let attrs = resource["attributes"]
        .as_array()
        .expect("resource attrs is an array");
    // service.name + tape.cassette.task — both required by ticket.
    assert!(attrs
        .iter()
        .any(|a| a["key"] == "service.name" && a["value"]["stringValue"] == "tape"));
    assert!(attrs.iter().any(|a| a["key"] == "tape.cassette.task"));

    let scope_spans = &resource_spans[0]["scopeSpans"]
        .as_array()
        .expect("scopeSpans is an array")[0];
    let spans = scope_spans["spans"].as_array().expect("spans is an array");
    // minimal-success.tape has exactly 3 tracks: task, model_call, eject.
    assert_eq!(spans.len(), 3, "expected 3 spans, got {}", spans.len());
}

#[test]
fn span_ids_and_trace_id_have_correct_hex_lengths() {
    // AC #3 — 16-byte traceId (32 hex chars), 8-byte spanId (16 hex
    // chars). All spans share traceId.
    let r = run_to_otlp(&fixture("minimal-success.tape"));
    assert!(r.status.success());
    let doc = parse_export(&r.stdout);
    let spans = doc["resourceSpans"][0]["scopeSpans"][0]["spans"]
        .as_array()
        .unwrap();
    let trace_id = spans[0]["traceId"].as_str().unwrap();
    assert_eq!(trace_id.len(), 32, "traceId must be 32 hex chars");
    for s in spans {
        assert_eq!(s["traceId"], trace_id, "all spans share traceId");
        let span_id = s["spanId"].as_str().unwrap();
        assert_eq!(span_id.len(), 16, "spanId must be 16 hex chars");
        assert!(
            span_id
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "spanId must be lowercase hex"
        );
    }
}

#[test]
fn root_span_omits_parent_span_id() {
    // AC #3 — the root span (task) has no parentSpanId. Other spans
    // either have one (linked to parent_step) or have none (also root).
    let r = run_to_otlp(&fixture("minimal-success.tape"));
    assert!(r.status.success());
    let doc = parse_export(&r.stdout);
    let spans = doc["resourceSpans"][0]["scopeSpans"][0]["spans"]
        .as_array()
        .unwrap();
    let task_span = spans
        .iter()
        .find(|s| s["name"] == "task")
        .expect("task span present");
    assert!(
        task_span.get("parentSpanId").is_none(),
        "task span must not have parentSpanId (omitted via skip_serializing_if)"
    );
}

#[test]
fn two_runs_against_same_input_produce_identical_span_ids() {
    // AC #5 — re-runs of the same cassette emit identical spanIds and
    // matching attribute/shape data. The only random part is traceId;
    // strip it from both before comparing.
    let r1 = run_to_otlp(&fixture("minimal-success.tape"));
    let r2 = run_to_otlp(&fixture("minimal-success.tape"));
    assert!(r1.status.success() && r2.status.success());
    let mut d1 = parse_export(&r1.stdout);
    let mut d2 = parse_export(&r2.stdout);
    // Strip traceIds for the comparison.
    strip_trace_ids(&mut d1);
    strip_trace_ids(&mut d2);
    assert_eq!(
        d1, d2,
        "two runs must produce identical output minus traceId"
    );
}

fn strip_trace_ids(doc: &mut Value) {
    let Some(spans) = doc["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array_mut() else {
        return;
    };
    for s in spans.iter_mut() {
        if let Some(obj) = s.as_object_mut() {
            obj.insert("traceId".to_owned(), Value::String("STRIPPED".to_owned()));
        }
    }
}

#[test]
fn output_flag_writes_to_file_with_parent_dirs() {
    // AC #4 — `--output` writes file + creates parent dirs as needed.
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("deep").join("nested").join("out.json");
    let r = std::process::Command::new(binary_path())
        .args([
            "to-otlp",
            fixture("minimal-success.tape").to_str().unwrap(),
            "--output",
            nested.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(r.status.success(), "tape to-otlp --output failed: {r:?}");
    assert!(nested.exists(), "output file should exist at {nested:?}");
    // Parse the written file — must be valid JSON with the OTLP shape.
    let body = std::fs::read(&nested).unwrap();
    let doc: Value = serde_json::from_slice(&body).expect("written file is valid JSON");
    assert!(doc["resourceSpans"][0]["scopeSpans"][0]["spans"]
        .as_array()
        .is_some());
}

#[test]
fn output_equals_input_exits_two() {
    // AC #4 second clause — `--output` equal to input exits 2.
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &copy).unwrap();
    let r = std::process::Command::new(binary_path())
        .args([
            "to-otlp",
            copy.to_str().unwrap(),
            "--output",
            copy.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("--output must differ"),
        "stderr should explain the refusal: {stderr}"
    );
}

#[test]
fn works_on_every_existing_fixture() {
    // AC #7 — `tape to-otlp` works on every fixture currently covered
    // by `tape verify`. Loop over the bundled .tape files in
    // tests/fixtures/; each one should exit 0 and emit parseable JSON.
    let fixtures_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures");
    let mut count = 0;
    for entry in std::fs::read_dir(&fixtures_dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("tape") {
            continue;
        }
        let r = run_to_otlp(&path);
        assert!(
            r.status.success(),
            "tape to-otlp failed on {path:?}: stderr={}",
            String::from_utf8_lossy(&r.stderr)
        );
        let _doc: Value =
            serde_json::from_slice(&r.stdout).expect("stdout is valid JSON for every fixture");
        count += 1;
    }
    assert!(count > 0, "should have iterated at least one .tape fixture");
}
