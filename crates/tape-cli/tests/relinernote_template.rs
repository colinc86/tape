//! `tape relinernote --template <name>` Step-3 integration coverage.
//! Issue #196. Most cases use `--dry-run` (no real judge HTTP call
//! needed; the binary prints the rendered prompt and exits 0); the
//! template_id audit test uses the axum mock pattern from
//! `relinernote_integration.rs`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

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

fn run_relinernote(args: &[&str]) -> std::process::Output {
    std::process::Command::new(binary_path())
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn default_is_the_default_template() {
    // AC #1: no `--template` flag → renders `default`. The
    // canonical "200–500 words" marker is the distinctive string in
    // the default template's instruction block.
    let r = run_relinernote(&[
        "relinernote",
        fixture("minimal-success.tape").to_str().unwrap(),
        "--dry-run",
    ]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("200–500 words"),
        "default template marker missing:\n{stdout}"
    );
    // The terse-template marker must NOT appear.
    assert!(
        !stdout.contains("100–200 words"),
        "terse marker should not appear in default render:\n{stdout}"
    );
}

#[test]
fn template_default_flag_is_equivalent_to_no_flag() {
    // AC #2: `--template default` byte-matches the no-flag render.
    let no_flag = run_relinernote(&[
        "relinernote",
        fixture("minimal-success.tape").to_str().unwrap(),
        "--dry-run",
    ]);
    let with_flag = run_relinernote(&[
        "relinernote",
        fixture("minimal-success.tape").to_str().unwrap(),
        "--dry-run",
        "--template",
        "default",
    ]);
    assert!(no_flag.status.success() && with_flag.status.success());
    assert_eq!(
        no_flag.stdout, with_flag.stdout,
        "--template default should byte-match the no-flag render"
    );
}

#[test]
fn template_terse_renders_terse_instruction_block() {
    // AC #3: `--template terse` swaps the instruction block; the
    // shared cassette-context + tracks segments stay identical.
    let default = run_relinernote(&[
        "relinernote",
        fixture("minimal-success.tape").to_str().unwrap(),
        "--dry-run",
    ]);
    let terse = run_relinernote(&[
        "relinernote",
        fixture("minimal-success.tape").to_str().unwrap(),
        "--dry-run",
        "--template",
        "terse",
    ]);
    assert!(default.status.success() && terse.status.success());
    let default_s = String::from_utf8(default.stdout).unwrap();
    let terse_s = String::from_utf8(terse.stdout).unwrap();

    assert_ne!(
        default_s, terse_s,
        "terse and default prompts should differ"
    );
    // Terse-specific markers per the issue body.
    assert!(
        terse_s.contains("100–200 words"),
        "terse template marker missing:\n{terse_s}"
    );
    assert!(
        terse_s.contains("bulleted") || terse_s.contains("bullet"),
        "terse template should reference bullets:\n{terse_s}"
    );
    // Shared four required H2 headings stay in both renders (AC #4).
    for heading in [
        "## What I was asked to do",
        "## What I found",
        "## Suggested next step / fix",
        "## What I'm uncertain about",
    ] {
        assert!(
            default_s.contains(heading),
            "default missing heading {heading:?}"
        );
        assert!(
            terse_s.contains(heading),
            "terse missing heading {heading:?}"
        );
    }
    // The cassette-context line (`Task: ...`) is shared — it's part
    // of the post-instructions body, identical for both templates.
    assert!(default_s.contains("Task: "));
    assert!(terse_s.contains("Task: "));
}

#[test]
fn unknown_template_exits_two_with_known_ids_listed() {
    // AC #5: bogus name → exit 2, `RELINER_TEMPLATE_NOT_FOUND`,
    // diagnostic enumerates the known catalog.
    let r = run_relinernote(&[
        "relinernote",
        fixture("minimal-success.tape").to_str().unwrap(),
        "--dry-run",
        "--template",
        "bogus",
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("RELINER_TEMPLATE_NOT_FOUND"), "{stderr}");
    assert!(stderr.contains("bogus"), "{stderr}");
    assert!(
        stderr.contains("default") && stderr.contains("terse"),
        "{stderr}"
    );
}

// --- AC #6: meta.relinernotes[].template_id records the resolved name ---
//
// Reaching this needs a real judge HTTP call; mock the upstream the
// way relinernote_integration.rs does and inspect the audit entry on
// the written cassette.

#[derive(Clone)]
struct MockState {
    call_count: Arc<AtomicU32>,
    response: String,
}

async fn handle(State(state): State<MockState>, Json(_body): Json<Value>) -> Json<Value> {
    state.call_count.fetch_add(1, Ordering::SeqCst);
    Json(json!({
        "choices": [{
            "message": { "role": "assistant", "content": state.response }
        }]
    }))
}

struct MockServer {
    addr: std::net::SocketAddr,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

fn spawn_mock(rt: &tokio::runtime::Runtime, response: &str) -> MockServer {
    let response = response.to_owned();
    rt.block_on(async move {
        let state = MockState {
            call_count: Arc::new(AtomicU32::new(0)),
            response,
        };
        let app = Router::new()
            .route("/v1/chat/completions", post(handle))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });
        MockServer {
            addr,
            _shutdown: tx,
        }
    })
}

const GOOD_LINER: &str = "## What I was asked to do\n\
investigate.\n\n\
## What I found\n\
a bug.\n\n\
## Suggested next step / fix\n\
add a guard.\n\n\
## What I'm uncertain about\n\
edge cases.\n";

fn write_judge_taperc(home: &std::path::Path, addr: &std::net::SocketAddr) {
    std::fs::write(
        home.join(".taperc"),
        format!(
            "judge:\n  model: judge-baseline\n  api_key_env: TAPE_RELINER_TEMPLATE_TEST_KEY\n  endpoint: http://{addr}/v1/chat/completions\n",
        ),
    )
    .unwrap();
}

fn template_id_from_meta(path: &std::path::Path) -> String {
    let raw = tape_format::reader::RawTape::open(path).unwrap();
    let meta = tape_format::meta::Meta::parse(&raw.meta_yaml.unwrap()).unwrap();
    meta.relinernotes
        .last()
        .expect("expected at least one relinernote audit entry")
        .template_id
        .clone()
}

#[test]
fn template_id_audit_records_resolved_name() {
    // AC #6: a successful relinernote round-trip with `--template
    // terse` records `template_id: "terse"` in the audit-log entry.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mock = spawn_mock(&rt, GOOD_LINER);
    write_judge_taperc(dir.path(), &mock.addr);

    let cassette = dir.path().join("input.tape");
    std::fs::copy(fixture("minimal-success.tape"), &cassette).unwrap();
    let out = dir.path().join("relinered.tape");

    let mut cmd = std::process::Command::new(binary_path());
    cmd.args([
        "relinernote",
        cassette.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--template",
        "terse",
    ])
    .env_remove("HOME")
    .env("HOME", dir.path())
    .env("TAPE_RELINER_TEMPLATE_TEST_KEY", "dummy")
    .current_dir(dir.path());
    let r = cmd.output().unwrap();
    assert!(
        r.status.success(),
        "tape relinernote failed: stdout={} stderr={}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr),
    );
    assert_eq!(
        template_id_from_meta(&out),
        "terse",
        "audit entry should reflect --template flag"
    );
}
