//! Issue #72: `tape record --label X` used to populate the default
//! filename only — the label never reached the produced tape. Now it
//! lands in `meta.label`.

use serde_json::json;
use tape_format::meta::{Meta, Outcome};
use tape_format::reader::RawTape;
use tape_format::tracks::Kind;
use tape_record::eject::{eject, EjectOptions};
use tape_record::session::Session;

fn run_eject(label: Option<String>) -> (std::path::PathBuf, tempfile::TempDir) {
    let session = Session::start("label test", "test/0.0.1");
    session.append(
        Kind::ModelCall,
        json!({"vendor": "anthropic", "model": "claude-opus-4-7"}),
    );
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("labelled.tape");
    eject(
        &session,
        &EjectOptions {
            task: "label test".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: None,
            inherited_artifacts: std::collections::BTreeMap::new(),
            label,
        },
    )
    .unwrap();
    (out, tmp)
}

#[test]
fn eject_with_label_lands_in_meta() {
    let (out, _tmp) = run_eject(Some("investigating-payments-bug".to_owned()));

    let raw = RawTape::open(&out).unwrap();
    let meta_yaml = raw.meta_yaml.expect("meta present");
    assert!(
        meta_yaml.contains("label: investigating-payments-bug"),
        "expected label line in meta.yaml; got:\n{meta_yaml}"
    );

    let meta = Meta::parse(&meta_yaml).unwrap();
    assert_eq!(meta.label.as_deref(), Some("investigating-payments-bug"));
}

#[test]
fn eject_without_label_omits_meta_field() {
    let (out, _tmp) = run_eject(None);

    let raw = RawTape::open(&out).unwrap();
    let meta_yaml = raw.meta_yaml.expect("meta present");
    assert!(
        !meta_yaml.contains("\nlabel:"),
        "label field should be omitted when None; got:\n{meta_yaml}"
    );

    let meta = Meta::parse(&meta_yaml).unwrap();
    assert!(meta.label.is_none());
}

/// Parser round-trip: writing a tape with a label and reading the meta
/// back should preserve the string verbatim.
#[test]
fn meta_label_roundtrips_through_yaml() {
    // A label with characters that YAML-quote: spaces, slashes, hyphens.
    let original = "team-a / 2026-Q2 investigation";
    let (out, _tmp) = run_eject(Some(original.to_owned()));

    let raw = RawTape::open(&out).unwrap();
    let meta = Meta::parse(raw.meta_yaml.as_deref().unwrap()).unwrap();
    assert_eq!(meta.label.as_deref(), Some(original));
}
