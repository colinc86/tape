//! Issue #17: `.taperc` is honored on every recording path.
//!
//! These tests build the redaction engine via `tape_redact::engine_with_taperc`
//! exactly as the production call sites in `tape-record::run`, `tape-mcp`
//! `tool_eject`, and `tool_snapshot` now do, then drive the eject pipeline
//! and inspect the resulting tape.
//!
//! HOME is mutated to isolate from the developer's real `~/.taperc`. Tests
//! that touch HOME serialize through a process-local mutex, since the env
//! is process-global and cargo runs tests in parallel.

use std::path::Path;
use std::sync::Mutex;

use serde_json::json;
use tape_format::meta::Outcome;
use tape_format::reader::RawTape;
use tape_format::tracks::Kind;
use tape_record::eject::{eject, EjectOptions};
use tape_record::session::Session;

/// Guards every HOME / `.taperc` test. `engine_with_taperc` reads `$HOME`
/// (for the user-level fallback) and walks the workspace ancestor chain;
/// if two tests touched HOME simultaneously we'd get flakes.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

/// Bracket a closure that needs an isolated HOME. Restores the previous HOME
/// on return — including panic paths via `_guard` Drop semantics aren't
/// trivial here, so we use a simple struct.
struct HomeGuard {
    prev: Option<std::ffi::OsString>,
}

impl HomeGuard {
    fn set(new_home: &Path) -> Self {
        let prev = std::env::var_os("HOME");
        std::env::set_var("HOME", new_home);
        Self { prev }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }
}

/// Make a one-track session whose `ModelCall` response contains `payload_text`.
fn session_with_payload(task: &str, payload_text: &str) -> Session {
    let s = Session::start(task, "test/0.0.1");
    s.append(
        Kind::ModelCall,
        json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "request": {"messages": [{"role": "user", "content": "go"}]},
            "response": {"content": [{"type": "text", "text": payload_text}]},
        }),
    );
    s
}

fn eject_with_engine(
    session: &Session,
    out: &Path,
    engine: tape_redact::Engine,
) -> tape_record::eject::EjectResult {
    eject(
        session,
        &EjectOptions {
            task: "taperc test".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.to_path_buf(),
            redact_engine: Some(engine),
            preserved_artifacts: None,
        },
    )
    .expect("eject succeeded")
}

#[test]
fn custom_rule_from_workspace_taperc_is_applied() {
    let _g = env_lock().lock().unwrap();

    // Workspace dir contains `.taperc` with a custom rule.
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(
        workspace.path().join(".taperc"),
        r"
redact:
  custom:
    - id: pii_customer
      pattern: 'CUST-\d{6}'
",
    )
    .unwrap();

    // Isolate HOME so the workspace walk stops cleanly and no real ~/.taperc
    // bleeds in.
    let fake_home = tempfile::tempdir().unwrap();
    let _home = HomeGuard::set(fake_home.path());

    let engine =
        tape_redact::engine_with_taperc(workspace.path()).expect("engine built");
    let session = session_with_payload("custom", "see CUST-447139 for details");
    let out_dir = tempfile::tempdir().unwrap();
    let out = out_dir.path().join("custom.tape");
    eject_with_engine(&session, &out, engine);

    let raw = RawTape::open(&out).unwrap();
    let tracks = raw.tracks_jsonl.expect("tracks");
    assert!(
        tracks.contains("<CUSTOM:pii_customer>"),
        "expected custom placeholder in tracks; got: {tracks}"
    );
    assert!(
        !tracks.contains("CUST-447139"),
        "raw customer id leaked; got: {tracks}"
    );
}

#[test]
fn disable_default_email_lets_email_survive() {
    let _g = env_lock().lock().unwrap();

    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(
        workspace.path().join(".taperc"),
        r#"
redact:
  disable_default: ["email"]
"#,
    )
    .unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    let _home = HomeGuard::set(fake_home.path());

    let engine =
        tape_redact::engine_with_taperc(workspace.path()).expect("engine built");
    let session = session_with_payload("disable", "ping alice@example.com");
    let out_dir = tempfile::tempdir().unwrap();
    let out = out_dir.path().join("disable.tape");
    eject_with_engine(&session, &out, engine);

    let raw = RawTape::open(&out).unwrap();
    let tracks = raw.tracks_jsonl.expect("tracks");
    assert!(
        tracks.contains("alice@example.com"),
        "email should NOT be redacted when default is disabled; got: {tracks}"
    );
    assert!(
        !tracks.contains("<EMAIL>"),
        "default <EMAIL> placeholder leaked despite disable_default; got: {tracks}"
    );
}

#[test]
fn enable_optional_redacts_private_ipv4() {
    let _g = env_lock().lock().unwrap();

    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(
        workspace.path().join(".taperc"),
        r#"
redact:
  enable_optional: ["ipv4_private"]
"#,
    )
    .unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    let _home = HomeGuard::set(fake_home.path());

    let engine =
        tape_redact::engine_with_taperc(workspace.path()).expect("engine built");
    let session = session_with_payload("opt-in", "internal host 10.0.0.1 listening");
    let out_dir = tempfile::tempdir().unwrap();
    let out = out_dir.path().join("optin.tape");
    eject_with_engine(&session, &out, engine);

    let raw = RawTape::open(&out).unwrap();
    let tracks = raw.tracks_jsonl.expect("tracks");
    assert!(
        tracks.contains("<IP:private>"),
        "expected <IP:private> placeholder; got: {tracks}"
    );
    assert!(
        !tracks.contains("10.0.0.1"),
        "private IP leaked despite enable_optional; got: {tracks}"
    );
}

#[test]
fn workspace_taperc_wins_over_user_taperc() {
    let _g = env_lock().lock().unwrap();

    // Workspace defines pattern A → <CUSTOM:workspace>.
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(
        workspace.path().join(".taperc"),
        r"
redact:
  custom:
    - id: workspace
      pattern: 'WIN-\d{3}'
",
    )
    .unwrap();

    // $HOME/.taperc defines a *different* rule that should be ignored.
    let fake_home = tempfile::tempdir().unwrap();
    std::fs::write(
        fake_home.path().join(".taperc"),
        r"
redact:
  custom:
    - id: home
      pattern: 'LOSE-\d{3}'
",
    )
    .unwrap();
    let _home = HomeGuard::set(fake_home.path());

    // Sanity: the workspace dir must NOT be under HOME, otherwise the
    // ancestor walk would terminate at HOME before finding the workspace
    // file. tempdir() returns a path outside the test HOME, so we're fine.
    assert!(!workspace.path().starts_with(fake_home.path()));

    let engine =
        tape_redact::engine_with_taperc(workspace.path()).expect("engine built");
    let session = session_with_payload(
        "precedence",
        "marker WIN-123 and runner-up LOSE-456 in body",
    );
    let out_dir = tempfile::tempdir().unwrap();
    let out = out_dir.path().join("precedence.tape");
    eject_with_engine(&session, &out, engine);

    let raw = RawTape::open(&out).unwrap();
    let tracks = raw.tracks_jsonl.expect("tracks");
    // Workspace rule fired.
    assert!(
        tracks.contains("<CUSTOM:workspace>"),
        "workspace rule did not apply; got: {tracks}"
    );
    assert!(!tracks.contains("WIN-123"));
    // User-level rule was NOT merged — its pattern is untouched. (SPEC §9:
    // CWD wins; no merge.)
    assert!(
        tracks.contains("LOSE-456"),
        "user-level rule was applied even though workspace .taperc existed; got: {tracks}"
    );
    assert!(!tracks.contains("<CUSTOM:home>"));
}

#[test]
fn invalid_replacement_in_taperc_aborts_engine_build() {
    let _g = env_lock().lock().unwrap();

    // SPEC §6.2: replacement must be a typed placeholder. A bare literal
    // must be rejected at config-load time so recording can't proceed with
    // a leaky rule silently downgraded to defaults.
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(
        workspace.path().join(".taperc"),
        r"
redact:
  custom:
    - id: leaky
      pattern: 'CUST-\d{6}'
      replacement: 'literal_value'
",
    )
    .unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    let _home = HomeGuard::set(fake_home.path());

    let err = tape_redact::engine_with_taperc(workspace.path())
        .expect_err("invalid .taperc must abort, not silently fall back");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("typed placeholder"),
        "expected typed-placeholder error, got: {msg}"
    );
}

#[test]
fn missing_taperc_falls_back_to_defaults() {
    let _g = env_lock().lock().unwrap();

    // No .taperc anywhere — engine must still build and apply default rules
    // (e.g. <EMAIL>).
    let workspace = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    let _home = HomeGuard::set(fake_home.path());

    let engine =
        tape_redact::engine_with_taperc(workspace.path()).expect("engine built");
    let session = session_with_payload("default", "reach me at bob@example.com");
    let out_dir = tempfile::tempdir().unwrap();
    let out = out_dir.path().join("default.tape");
    eject_with_engine(&session, &out, engine);

    let raw = RawTape::open(&out).unwrap();
    let tracks = raw.tracks_jsonl.expect("tracks");
    assert!(
        tracks.contains("<EMAIL>"),
        "default email rule should apply when no .taperc exists; got: {tracks}"
    );
    assert!(!tracks.contains("bob@example.com"));
}
