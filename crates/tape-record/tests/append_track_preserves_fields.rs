//! Regression tests for issue #49: `Session::append_at` silently dropped
//! `parent_step`, `refs`, and `annotations` on every replay path. The fix
//! adds `Session::append_track(track)`, which keeps every field on the
//! incoming `Track` and only reassigns `step`.
//!
//! The unit test in this file mirrors the issue's reproducer. The deck-
//! round-trip cases that exercise `tool_eject` / `tool_annotate` live in
//! `crates/tape-mcp/tests/` (this crate doesn't depend on `tape-mcp`).

use tape_format::tracks::{Annotation, Kind, Track};
use tape_record::session::Session;

/// Issue #49 reproducer: a Track with non-default `parent_step`, `refs`, and
/// `annotations` must round-trip through `append_track` with all three fields
/// intact. Before the fix, the replay paths called `append_at(kind, payload,
/// ts)` which hardcoded all three to defaults.
#[test]
fn append_track_preserves_parent_step_refs_and_annotations() {
    let s = Session::start("repro", "test/0.0.1");

    let incoming = Track {
        // Will be reassigned by `append_track` — proves the method only
        // overwrites `step` and nothing else.
        step: 999,
        kind: Kind::FileRead,
        ts: "2026-05-13T12:34:56.000Z".to_string(),
        payload: serde_json::json!({"path": "/var/log/app.log"}),
        parent_step: Some(1),
        refs: vec!["sha:abc123".to_string()],
        annotations: vec![Annotation {
            by: "human".to_string(),
            note: "load-bearing field".to_string(),
        }],
    };

    let assigned = s.append_track(incoming.clone());
    assert_eq!(assigned, 2, "step is assigned, not preserved");

    let snap = s.snapshot();
    let appended = &snap.tracks[1];
    assert_eq!(appended.step, 2, "step is the session-assigned value");
    assert_eq!(appended.kind, Kind::FileRead);
    assert_eq!(
        appended.ts, "2026-05-13T12:34:56.000Z",
        "ts is preserved verbatim"
    );
    assert_eq!(appended.payload, incoming.payload, "payload is preserved");
    assert_eq!(
        appended.parent_step,
        Some(1),
        "parent_step survives — was dropped before the fix"
    );
    assert_eq!(
        appended.refs,
        vec!["sha:abc123".to_string()],
        "refs survive — was dropped before the fix (orphan artifacts)"
    );
    assert_eq!(
        appended.annotations.len(),
        1,
        "annotations survive — was dropped before the fix"
    );
    assert_eq!(appended.annotations[0].by, "human");
    assert_eq!(appended.annotations[0].note, "load-bearing field");
}

/// `append_track` must still reassign `step` monotonically, ignoring whatever
/// step was on the incoming track.
#[test]
fn append_track_reassigns_step_monotonically() {
    let s = Session::start("step-mono", "test/0.0.1");
    let a = s.append_track(Track {
        step: 17,
        kind: Kind::ModelCall,
        ts: "2026-05-13T12:00:00.000Z".to_string(),
        payload: serde_json::json!({"vendor": "anthropic", "model": "x"}),
        parent_step: None,
        refs: vec![],
        annotations: vec![],
    });
    let b = s.append_track(Track {
        step: 99,
        kind: Kind::Shell,
        ts: "2026-05-13T12:00:01.000Z".to_string(),
        payload: serde_json::json!({"cmd": "ls"}),
        parent_step: None,
        refs: vec![],
        annotations: vec![],
    });
    assert_eq!(a, 2, "first append after task gets step 2");
    assert_eq!(b, 3, "second append gets step 3");
}
