//! SPEC §5.4 invariant: a tape has exactly one `eject` event, and it's the
//! final event. The eject pipeline must enforce this even when the input
//! session already ends in an `eject` (e.g. a forked handle that retained
//! the source's terminator). See issue #26.

use serde_json::json;
use tape_format::meta::Outcome;
use tape_format::reader::RawTape;
use tape_format::tracks::{self, Kind};
use tape_record::eject::{eject, EjectOptions};
use tape_record::session::Session;

#[test]
fn eject_pipeline_collapses_trailing_eject_into_single_terminator() {
    // Simulate a forked handle: task → model_call → (source's) eject.
    let session = Session::start("forked", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({"vendor": "anthropic", "model": "claude-opus-4-7"}),
    );
    // This is what `tool_fork`'s `truncate(from_step)` leaves behind when
    // forking at the source's last step: the source's eject is still in the
    // tracks vec. Without the defensive pop, the pipeline appends another.
    session.append(Kind::Eject, json!({"outcome": "success"}));

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("forked.tape");
    eject(
        &session,
        &EjectOptions {
            task: "forked".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: None,
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
        },
    )
    .unwrap();

    // The produced tape must verify clean.
    let raw = RawTape::open(&out).unwrap();
    let report = tape_format::verify::verify(&raw);
    assert!(
        report.is_valid(),
        "tape failed verify: {:?}",
        report
            .errors()
            .map(|d| (d.code.as_str(), &d.message))
            .collect::<Vec<_>>()
    );

    // …and contain exactly one eject, as the final event.
    let parsed = tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap()).unwrap();
    let eject_count = parsed.iter().filter(|t| t.kind == Kind::Eject).count();
    assert_eq!(
        eject_count, 1,
        "expected exactly one eject, got {eject_count}"
    );
    assert_eq!(parsed.last().map(|t| t.kind), Some(Kind::Eject));
}

#[test]
fn eject_pipeline_unchanged_for_session_without_trailing_eject() {
    // Sanity: a "normal" session (no terminator yet) still gets one eject
    // appended — the new defensive pop is a no-op in this case.
    let session = Session::start("normal", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({"vendor": "anthropic", "model": "claude-opus-4-7"}),
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("normal.tape");
    eject(
        &session,
        &EjectOptions {
            task: "normal".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: None,
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
        },
    )
    .unwrap();

    let raw = RawTape::open(&out).unwrap();
    assert!(tape_format::verify::verify(&raw).is_valid());
    let parsed = tracks::parse_jsonl(raw.tracks_jsonl.as_deref().unwrap()).unwrap();
    assert_eq!(parsed.iter().filter(|t| t.kind == Kind::Eject).count(), 1);
    // Task + model_call + eject = 3.
    assert_eq!(parsed.len(), 3);
}
