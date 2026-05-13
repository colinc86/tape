//! Verifies that the eject pipeline applies redaction to track payloads
//! and produces a valid `redactions.json` consistent with `meta.redaction_summary`.

use serde_json::json;
use tape_format::meta::Outcome;
use tape_format::reader::RawTape;
use tape_format::tracks::Kind;
use tape_record::eject::{eject, EjectOptions};
use tape_record::session::Session;
use tape_redact::Engine;

#[test]
fn eject_redacts_email_in_track_payload() {
    let session = Session::start("redact test", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "Email me at alice@example.com"}]},
            "response": {"content": [{"type": "text", "text": "Will do."}]}
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("redacted.tape");
    let result = eject(
        &session,
        &EjectOptions {
            task: "redact test".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: Some(Engine::with_default_rules()),
        },
    )
    .unwrap();

    assert!(result.redaction_count >= 1);

    // Read it back and check the email was replaced.
    let raw = RawTape::open(&out).unwrap();
    let tracks = raw.tracks_jsonl.expect("tracks present");
    assert!(
        tracks.contains("<EMAIL>"),
        "expected <EMAIL> in tracks; got: {tracks}"
    );
    assert!(!tracks.contains("alice@example.com"));

    // redactions.json should exist and parse.
    let redactions_json = raw.redactions_json.expect("redactions.json present");
    let recs: Vec<serde_json::Value> = serde_json::from_str(&redactions_json).unwrap();
    assert!(!recs.is_empty());
    assert!(recs.iter().any(|r| r["rule_id"] == "email"));

    // meta.yaml's redaction_summary should agree.
    let meta_yaml = raw.meta_yaml.expect("meta present");
    assert!(meta_yaml.contains("redaction_summary"));
    assert!(meta_yaml.contains("email"));
}

/// Issue #11: when an oversize string contains a secret, spillover used to
/// `mem::take` the string into an artifact *before* the redaction engine ran,
/// so the artifact bytes leaked the secret in plaintext. After the fix,
/// redaction runs first; the spilled bytes are post-redaction.
#[test]
fn spilled_payloads_are_redacted_in_artifacts() {
    use std::io::Read;
    let session = Session::start("spill leak", "test/0.0.1");
    let bait = format!(
        "{}\nlog: AKIA{}\n{}",
        "x".repeat(2048),
        "1234567890ABCDEF",
        "y".repeat(2048),
    );
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "x"}]},
            "response": bait,
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("spill.tape");
    eject(
        &session,
        &EjectOptions {
            task: "spill leak".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: Some(Engine::with_default_rules()),
        },
    )
    .unwrap();

    // Walk every entry under artifacts/ and assert the literal AWS access-key
    // prefix is nowhere to be found.
    let zip_file = std::fs::File::open(&out).unwrap();
    let mut archive = zip::ZipArchive::new(zip_file).unwrap();
    let mut found_artifacts = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        if !name.starts_with("artifacts/") {
            continue;
        }
        found_artifacts += 1;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).unwrap();
        assert!(
            !buf.windows(20).any(|w| w == b"AKIA1234567890ABCDEF"),
            "artifact {name} contains unredacted AWS access key"
        );
    }
    assert!(found_artifacts > 0, "expected the oversize string to spill");
}

/// A bearer token embedded in an oversize payload should be replaced before
/// the bytes reach `artifacts/`.
#[test]
fn spilled_payloads_redact_bearer_tokens() {
    use std::io::Read;
    let session = Session::start("bearer leak", "test/0.0.1");
    let padding = "x".repeat(4096);
    let leak = "Bearer abcdefghijklmnopqrstuvwxyz0123456789";
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "x"}]},
            "response": format!("{padding}\nAuthorization: {leak}\n{padding}"),
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("bearer.tape");
    eject(
        &session,
        &EjectOptions {
            task: "bearer leak".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: Some(Engine::with_default_rules()),
        },
    )
    .unwrap();

    let zip_file = std::fs::File::open(&out).unwrap();
    let mut archive = zip::ZipArchive::new(zip_file).unwrap();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        if !name.starts_with("artifacts/") {
            continue;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(
            !s.contains(leak),
            "artifact {name} contains unredacted bearer token"
        );
    }
}

#[test]
fn eject_redacts_anthropic_key_in_response() {
    let session = Session::start("key leak", "test/0.0.1");
    let leak = "sk-ant-api03-AbCdEf1234567890abcdef1234567890aBcDeF12_-XX";
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "x"}]},
            "response": {"content": [{"type": "text", "text": format!("auth: {leak}")}]}
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("redacted.tape");
    eject(
        &session,
        &EjectOptions {
            task: "key leak".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: Some(Engine::with_default_rules()),
        },
    )
    .unwrap();

    let raw = RawTape::open(&out).unwrap();
    let tracks = raw.tracks_jsonl.unwrap();
    assert!(tracks.contains("<API_KEY:anthropic>"));
    assert!(!tracks.contains(leak));
}
