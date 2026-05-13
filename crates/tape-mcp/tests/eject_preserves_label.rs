//! Issue #80 regression: `tape.eject` on a loaded tape must inherit the
//! source `meta.label`. Before the fix, `tool_eject` passed `label: None`
//! to `eject::eject`, which builds a fresh Meta — silently dropping the
//! label on every load → eject round-trip.

use serde_json::{json, Value};
use tape_format::meta::Outcome;
use tape_format::tracks::Kind;
use tape_record::eject::{eject, EjectOptions};
use tape_record::session::Session;

/// Build a minimal valid `.tape` on disk with the given `label` and return
/// the path inside `tmp`. Used to manufacture a source tape we can `tape.load`.
fn build_labelled_tape(tmp: &std::path::Path, label: Option<&str>, name: &str) -> std::path::PathBuf {
    let session = Session::start("label preservation source", "test/0.0.1");
    session.append(Kind::Task, json!({"prompt": "label preservation source"}));

    let out = tmp.join(name);
    eject(
        &session,
        &EjectOptions {
            task: "label preservation source".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: None,
            inherited_artifacts: std::collections::BTreeMap::new(),
            label: label.map(str::to_owned),
        },
    )
    .expect("source tape eject");
    out
}

fn pump(deck: tape_mcp::Deck, requests: &[Value]) -> Vec<Value> {
    let mut input = String::new();
    for r in requests {
        input.push_str(&r.to_string());
        input.push('\n');
    }
    let mut output = Vec::<u8>::new();
    tape_mcp::server::run(input.as_bytes(), &mut output, deck).unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn load_and_eject(src: &std::path::Path, out: &std::path::Path) {
    let deck = tape_mcp::Deck::new();
    let load = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "tape.load", "arguments": {"path": src.to_str().unwrap()}}
    });
    let load_resp = pump(deck.clone(), &[load]);
    let handle = load_resp[0]["result"]["structuredContent"]["handle"]
        .as_str()
        .expect("handle in load response")
        .to_owned();

    let eject_req = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "tape.eject", "arguments": {
            "handle": handle, "out": out.to_str().unwrap()
        }}
    });
    let resp = pump(deck, &[eject_req]);
    assert_eq!(
        resp[0]["result"]["isError"].as_bool().unwrap_or(false),
        false,
        "eject should succeed; got {:?}",
        resp[0]
    );
}

fn read_label(path: &std::path::Path) -> Option<String> {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    let yaml = raw.meta_yaml.expect("meta.yaml present");
    let meta: tape_format::meta::Meta = serde_yaml::from_str(&yaml).expect("meta parses");
    meta.label
}

/// Reproducer for #80: a labelled source tape must keep its label after a
/// `tape.load` → `tape.eject` round-trip via the deck. Before the fix, the
/// re-ejected `meta.yaml` had no `label` field at all.
#[test]
fn eject_inherits_label_from_loaded_tape() {
    let tmp = tempfile::tempdir().unwrap();
    let src = build_labelled_tape(tmp.path(), Some("incident-4471"), "src.tape");
    let dst = tmp.path().join("re-ejected.tape");

    // Sanity: the source actually has the label.
    assert_eq!(read_label(&src), Some("incident-4471".to_owned()));

    load_and_eject(&src, &dst);

    assert_eq!(
        read_label(&dst),
        Some("incident-4471".to_owned()),
        "re-ejected tape dropped meta.label (issue #80)"
    );
}

/// Inverse: an unlabelled source must not gain a spurious label from the
/// inheritance path. Guards against accidentally defaulting to `Some("")` or
/// pulling a label from somewhere else.
#[test]
fn eject_leaves_label_none_when_source_unlabelled() {
    let tmp = tempfile::tempdir().unwrap();
    let src = build_labelled_tape(tmp.path(), None, "unlabelled-src.tape");
    let dst = tmp.path().join("re-ejected-unlabelled.tape");

    assert_eq!(read_label(&src), None);

    load_and_eject(&src, &dst);

    assert_eq!(
        read_label(&dst),
        None,
        "re-ejected tape gained a spurious label"
    );
}
