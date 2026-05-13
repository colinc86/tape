//! Regression test for issue #26: the eject pipeline must normalize away a
//! trailing `Eject` track on the input session before appending the new
//! terminator. The single-eject-as-final-track invariant (SPEC §5.4) holds
//! regardless of input shape — fork-handle replay, transcript snapshot, or
//! any future replay path.

use serde_json::json;
use tape_format::meta::Outcome;
use tape_format::reader::RawTape;
use tape_format::tracks::{Kind, Track};
use tape_format::verify::verify;
use tape_record::eject::{eject, EjectOptions};
use tape_record::session::Session;

#[test]
fn eject_strips_trailing_eject_before_appending() {
    // Build a session whose final track is itself an Eject — the exact shape
    // produced by `tape.fork {from_step: source.tracks.len()}` replayed into
    // the eject pipeline.
    let session = Session::start("trailing-eject regression", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": []},
            "response": {"content": []}
        }),
    );
    // Simulate the inherited terminator from a forked source tape.
    session.append(Kind::Eject, json!({"outcome": "success"}));

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("forked.tape");
    let result = eject(
        &session,
        &EjectOptions {
            task: "trailing-eject regression".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: None,
        },
    )
    .expect("eject should succeed");

    // The produced tape must verify clean — no EJECT_NOT_LAST diagnostic.
    let raw = RawTape::open(&out).unwrap();
    let report = verify(&raw);
    assert!(
        report.is_valid(),
        "ejected tape should verify; errors: {:?}",
        report.errors().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );

    // Exactly one Eject track, and it must be the final line.
    let tracks_jsonl = raw.tracks_jsonl.expect("tracks.jsonl present");
    let tracks: Vec<Track> = tracks_jsonl
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| Track::from_line(l).expect("valid track line"))
        .collect();
    let eject_count = tracks.iter().filter(|t| t.kind == Kind::Eject).count();
    assert_eq!(eject_count, 1, "expected exactly one eject track");
    assert_eq!(
        tracks.last().map(|t| t.kind),
        Some(Kind::Eject),
        "the final track must be an eject"
    );

    // The result reports the post-normalization track count.
    assert_eq!(result.track_count, tracks.len() as u64);
}
