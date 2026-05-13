//! Issue #109: `meta.tool_budget` used to always be `None`, which silently
//! broke `tape diff`'s Latency summary (it always reported 0 ms / 0 ms /
//! Δ0%). Now the eject pipeline populates `tool_budget` from the in-flight
//! session so consumers see honest call counts, token sums, and wall-clock
//! duration.

use std::collections::BTreeMap;

use serde_json::json;
use tape_format::meta::{Meta, Outcome};
use tape_format::reader::RawTape;
use tape_format::tracks::Kind;
use tape_record::eject::{eject, EjectOptions};
use tape_record::session::Session;

fn run_eject(populate: impl FnOnce(&Session)) -> (std::path::PathBuf, tempfile::TempDir) {
    let session = Session::start("budget test", "test/0.0.1");
    populate(&session);
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("budgeted.tape");
    eject(
        &session,
        &EjectOptions {
            task: "budget test".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: None,
            inherited_artifacts: BTreeMap::new(),
            label: None,
        },
    )
    .unwrap();
    (out, tmp)
}

fn read_meta(path: &std::path::Path) -> Meta {
    let raw = RawTape::open(path).unwrap();
    let yaml = raw.meta_yaml.expect("meta present");
    Meta::parse(&yaml).unwrap()
}

/// Happy path: a tape with model_call + mcp_call + shell events should
/// produce a `tool_budget` whose `total_calls` is 3 and whose token totals
/// reflect the per-event `tokens_in` / `tokens_out` values.
#[test]
fn tool_budget_counts_calls_and_tokens() {
    let (out, _tmp) = run_eject(|s| {
        s.append(
            Kind::ModelCall,
            json!({
                "vendor": "anthropic",
                "model": "claude-opus-4-7",
                "tokens_in": 1_000,
                "tokens_out": 200,
            }),
        );
        s.append(
            Kind::ModelCall,
            json!({
                "vendor": "anthropic",
                "model": "claude-opus-4-7",
                "tokens_in": 500,
                "tokens_out": 75,
            }),
        );
        s.append(Kind::McpCall, json!({"server": "fs", "tool": "read"}));
        s.append(Kind::Shell, json!({"cmd": "ls"}));
    });

    let meta = read_meta(&out);
    let budget = meta.tool_budget.expect("tool_budget should be populated");
    // 2 model_call + 1 mcp_call + 1 shell — task and eject events are excluded
    // by the filter.
    assert_eq!(budget.total_calls, 4);
    assert_eq!(budget.total_tokens_in, 1_500);
    assert_eq!(budget.total_tokens_out, 275);
}

/// Wall-clock duration uses the session's `created_at` and the eject's `now`,
/// so even a recording with no tool calls should report a non-trivial elapsed
/// time. We sleep briefly to make the assertion meaningful without flakiness.
#[test]
fn tool_budget_wall_clock_ms_reflects_session_duration() {
    let session = Session::start("idle", "test/0.0.1");
    std::thread::sleep(std::time::Duration::from_millis(25));
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("idle.tape");
    eject(
        &session,
        &EjectOptions {
            task: "idle".into(),
            recorder_agent: "test/0.0.1".into(),
            outcome: Outcome::Success,
            stub_liner_notes: true,
            out_path: out.clone(),
            redact_engine: None,
            inherited_artifacts: BTreeMap::new(),
            label: None,
        },
    )
    .unwrap();

    let meta = read_meta(&out);
    let budget = meta.tool_budget.expect("tool_budget should be populated");
    assert_eq!(budget.total_calls, 0);
    assert_eq!(budget.total_tokens_in, 0);
    assert_eq!(budget.total_tokens_out, 0);
    assert!(
        budget.wall_clock_ms >= 20,
        "expected ≥20ms elapsed; got {}",
        budget.wall_clock_ms
    );
}

/// A model_call without `tokens_in` / `tokens_out` keys (e.g. a streamed
/// response the proxy couldn't sum up) should contribute zero to the totals.
/// The budget is still emitted — zero is honest, missing would silently dead
/// the consumer's Latency line again.
#[test]
fn tool_budget_emitted_even_when_token_keys_missing() {
    let (out, _tmp) = run_eject(|s| {
        s.append(
            Kind::ModelCall,
            json!({"vendor": "anthropic", "model": "claude-opus-4-7"}),
        );
    });

    let meta = read_meta(&out);
    let budget = meta.tool_budget.expect("tool_budget should be populated");
    assert_eq!(budget.total_calls, 1);
    assert_eq!(budget.total_tokens_in, 0);
    assert_eq!(budget.total_tokens_out, 0);
}

/// A task-only recording (no tool calls at all) still gets a `tool_budget`,
/// with every count and token total at zero. The point is that the field is
/// present and parseable — downstream tooling can rely on its existence.
#[test]
fn tool_budget_emitted_for_zero_event_tape() {
    let (out, _tmp) = run_eject(|_| {});

    let meta = read_meta(&out);
    let budget = meta.tool_budget.expect("tool_budget should be populated");
    assert_eq!(budget.total_calls, 0);
    assert_eq!(budget.total_tokens_in, 0);
    assert_eq!(budget.total_tokens_out, 0);
    // wall_clock_ms is monotonically nonnegative and not asserted here — the
    // session may have ejected in the same millisecond it started.
}

/// YAML round-trip: writing a tape with a populated `tool_budget` and parsing
/// the meta back should preserve every field verbatim, and the serialised YAML
/// should contain a `tool_budget:` block (regression for "field omitted because
/// `None` round-tripped through `skip_serializing_if`").
#[test]
fn tool_budget_roundtrips_through_yaml() {
    let (out, _tmp) = run_eject(|s| {
        s.append(
            Kind::ModelCall,
            json!({
                "vendor": "anthropic",
                "model": "claude-opus-4-7",
                "tokens_in": 42,
                "tokens_out": 7,
            }),
        );
    });

    let raw = RawTape::open(&out).unwrap();
    let yaml = raw.meta_yaml.as_deref().unwrap();
    assert!(
        yaml.contains("tool_budget:"),
        "tool_budget block should be present in meta.yaml; got:\n{yaml}"
    );

    let meta = Meta::parse(yaml).unwrap();
    let budget = meta.tool_budget.expect("tool_budget should be populated");
    assert_eq!(budget.total_calls, 1);
    assert_eq!(budget.total_tokens_in, 42);
    assert_eq!(budget.total_tokens_out, 7);
}
