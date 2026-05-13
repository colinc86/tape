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
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
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
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
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
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
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

/// Issue #23: an oversize string of legitimate high-entropy base64 (e.g. a
/// large attachment) used to trip the defense-in-depth scan because that scan
/// ran `generic_high_entropy` regardless of whether the engine was configured
/// to redact it. With default rules, `generic_high_entropy` is opt-in, so the
/// scan must NOT flag the artifact. Eject should succeed.
#[test]
fn eject_succeeds_when_oversize_artifact_is_high_entropy_base64() {
    // A base64-ish blob that easily exceeds 4 KiB and satisfies the
    // generic_high_entropy validator (≥4.5 bits/char).
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut blob = String::with_capacity(8192);
    for i in 0..8192 {
        blob.push(alphabet[i % alphabet.len()] as char);
    }

    let session = Session::start("base64 payload", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "x"}]},
            "response": blob,
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("base64.tape");
    let result = eject(
        &session,
        &EjectOptions {
            task: "base64 payload".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            // Default rules — generic_high_entropy NOT enabled.
            redact_engine: Some(Engine::with_default_rules()),
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
        },
    )
    .expect("eject should succeed with default rules");

    // Artifact spilled, no redactions applied (no opted-in rule matched).
    assert!(out.exists());
    assert_eq!(result.redaction_count, 0);
}

/// Issue #23 (variant): a legitimate private IPv4 buried in an oversize string
/// matches the opt-in `ipv4_private` rule. With default rules it must NOT be
/// flagged by the defense-in-depth scan.
#[test]
fn eject_succeeds_when_oversize_artifact_contains_private_ip() {
    let bait = format!(
        "{}\nhealthcheck: 192.168.1.42 ok\n{}",
        "x".repeat(2048),
        "y".repeat(2048),
    );

    let session = Session::start("private ip payload", "test/0.0.1");
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
    let out = tmp.path().join("privip.tape");
    let result = eject(
        &session,
        &EjectOptions {
            task: "private ip payload".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: Some(Engine::with_default_rules()),
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
        },
    )
    .expect("eject should succeed with default rules");

    assert!(out.exists());
    assert_eq!(result.redaction_count, 0);
}

/// Positive-coverage: when the user explicitly opts into `generic_high_entropy`,
/// the engine redacts it inline AND the defense-in-depth scan stays clean
/// because Pass 1 caught it. Symmetric enforcement, end-to-end.
#[test]
fn eject_redacts_high_entropy_when_opted_in() {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut blob = String::with_capacity(8192);
    for i in 0..8192 {
        blob.push(alphabet[i % alphabet.len()] as char);
    }

    let session = Session::start("opt-in high entropy", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "x"}]},
            "response": blob,
        }),
    );

    // Engine with default rules PLUS generic_high_entropy explicitly enabled.
    let mut engine = Engine::with_default_rules();
    let hi = tape_redact::rules::built_in()
        .into_iter()
        .find(|r| r.id == "generic_high_entropy")
        .expect("rule defined");
    engine.add_rule(hi);

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("optin.tape");
    let result = eject(
        &session,
        &EjectOptions {
            task: "opt-in high entropy".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: Some(engine),
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
        },
    )
    .expect("eject should succeed when high-entropy is redacted, not flagged");

    assert!(result.redaction_count >= 1);
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
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
        },
    )
    .unwrap();

    let raw = RawTape::open(&out).unwrap();
    let tracks = raw.tracks_jsonl.unwrap();
    assert!(tracks.contains("<API_KEY:anthropic>"));
    assert!(!tracks.contains(leak));
}

/// Issue #77: `meta.label` is user-provided and was skipped by the meta
/// redaction block. A label containing an email (or any default-enabled rule
/// match) used to crash the defense-in-depth scan. After the fix it is
/// redacted in-place like meta.task / recorder.user / recorder.agent.
#[test]
fn eject_redacts_email_in_meta_label() {
    let session = Session::start("label leak", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "x"}]},
            "response": {"content": [{"type": "text", "text": "ok"}]}
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("label.tape");
    eject(
        &session,
        &EjectOptions {
            task: "label leak".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: Some(Engine::with_default_rules()),
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: Some("investigating-bug-for-alice@example.com".into()),
        },
    )
    .expect("eject should succeed once the label is redacted before the scan");

    let raw = RawTape::open(&out).unwrap();
    let meta_yaml = raw.meta_yaml.expect("meta present");
    assert!(
        meta_yaml.contains("<EMAIL>"),
        "expected <EMAIL> in meta.yaml; got: {meta_yaml}"
    );
    assert!(
        !meta_yaml.contains("alice@example.com"),
        "raw email leaked into meta.yaml: {meta_yaml}"
    );
}

/// Negative case: a plain label that matches no rules survives unchanged.
#[test]
fn eject_preserves_plain_meta_label() {
    let session = Session::start("plain label", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "x"}]},
            "response": {"content": [{"type": "text", "text": "ok"}]}
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("plain-label.tape");
    eject(
        &session,
        &EjectOptions {
            task: "plain label".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: Some(Engine::with_default_rules()),
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: Some("plain-text-label".into()),
        },
    )
    .expect("eject should succeed for a non-matching label");

    let raw = RawTape::open(&out).unwrap();
    let meta_yaml = raw.meta_yaml.expect("meta present");
    assert!(
        meta_yaml.contains("plain-text-label"),
        "label was unexpectedly altered; meta.yaml: {meta_yaml}"
    );
}

/// `label: None` must remain a no-op — no panic, no diagnostic, no label key
/// emitted in meta.yaml (the field is `skip_serializing_if = Option::is_none`).
#[test]
fn eject_handles_absent_meta_label() {
    let session = Session::start("no label", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "x"}]},
            "response": {"content": [{"type": "text", "text": "ok"}]}
        }),
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("no-label.tape");
    eject(
        &session,
        &EjectOptions {
            task: "no label".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: Some(Engine::with_default_rules()),
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: None,
        },
    )
    .expect("eject should succeed with no label");

    let raw = RawTape::open(&out).unwrap();
    let meta_yaml = raw.meta_yaml.expect("meta present");
    assert!(
        !meta_yaml.contains("label:"),
        "meta.yaml should omit label when None; got: {meta_yaml}"
    );
}
