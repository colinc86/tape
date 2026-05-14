//! Builds the canonical test fixtures into `tests/fixtures/`.
//!
//! Run from the workspace root:
//!     cargo run --example build_fixtures -p tape-format
//!
//! Fixtures are deterministic — the same invocation produces byte-identical
//! `.tape` files (modulo zip metadata bits we don't control). They are
//! checked in alongside the source so CI can run `tape verify` against them.

use std::collections::BTreeMap;
use std::path::Path;

use tape_format::artifact::{artifact_path, blake3_hex};
use tape_format::writer::PendingTape;

fn main() -> anyhow::Result<()> {
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(|d| Path::new(&d).join("..").join("..").canonicalize())
        .map_err(anyhow::Error::from)
        .and_then(|r| r.map_err(anyhow::Error::from))?;
    let out_dir = workspace_root.join("tests/fixtures");
    let malformed_dir = out_dir.join("malformed");
    std::fs::create_dir_all(&out_dir)?;
    std::fs::create_dir_all(&malformed_dir)?;

    minimal_success(&out_dir)?;
    oversized_payload(&out_dir)?;
    with_mcp_calls(&out_dir)?;

    // Malformed: each one is paired with a `<name>.expected.json` sidecar that
    // lists the diagnostic codes verify should produce.
    malformed_missing_eject(&malformed_dir)?;
    malformed_step_gap(&malformed_dir)?;
    malformed_unknown_kind(&malformed_dir)?;
    malformed_outcome_mismatch(&malformed_dir)?;
    malformed_artifact_hash_mismatch(&malformed_dir)?;
    malformed_oversized_inline(&malformed_dir)?;
    malformed_leaked_anthropic_key(&malformed_dir)?;
    malformed_wrong_tape_version(&malformed_dir)?;
    malformed_invalid_parent_step(&malformed_dir)?;
    malformed_leaked_jwt(&malformed_dir)?;
    malformed_leaked_aws_access_key(&malformed_dir)?;
    malformed_leaked_email(&malformed_dir)?;
    malformed_reserved_kind_fork(&malformed_dir)?;
    malformed_reserved_kind_splice(&malformed_dir)?;
    malformed_duplicate_task(&malformed_dir)?;
    malformed_duplicate_eject(&malformed_dir)?;
    malformed_empty_task_prompt(&malformed_dir)?;
    malformed_meta_timestamps_inverted(&malformed_dir)?;
    malformed_meta_recap_overlong(&malformed_dir)?;

    println!("All fixtures written.");
    Ok(())
}

const STD_LINER: &str = "## What I was asked to do
Investigate a fixture-grade scenario for testing.

## What I found
The investigation completed successfully.

## Suggested next step / fix
None — fixture is for verify-pass coverage.

## What I'm uncertain about
Nothing material.
";

fn minimal_success(out_dir: &Path) -> anyhow::Result<()> {
    let meta = r#"tape_version: "tape/v0"
id: "01h8xy00-0000-7000-b8aa-000000000001"
created_at: "2026-05-06T10:00:00Z"
ejected_at: "2026-05-06T10:00:30Z"
task: "Say hello"
recorder:
  agent: "claude-code/2.1.4"
outcome: success
"#;
    let liner = "## What I was asked to do
Say hello.

## What I found
The greeting was produced.

## Suggested next step / fix
None — task completed.

## What I'm uncertain about
Nothing.
";
    let tracks = "{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-06T10:00:00Z\",\"payload\":{\"prompt\":\"Say hello\"}}\n{\"step\":2,\"kind\":\"model_call\",\"ts\":\"2026-05-06T10:00:15Z\",\"payload\":{\"vendor\":\"anthropic\",\"model\":\"claude-opus-4-7\",\"request\":{\"messages\":[{\"role\":\"user\",\"content\":\"Say hello\"}]},\"response\":{\"content\":[{\"type\":\"text\",\"text\":\"Hello!\"}]}}}\n{\"step\":3,\"kind\":\"eject\",\"ts\":\"2026-05-06T10:00:30Z\",\"payload\":{\"outcome\":\"success\"}}\n";

    let pending = PendingTape {
        meta_yaml: meta.into(),
        liner_md: liner.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out_dir.join("minimal-success.tape"))?;
    Ok(())
}

fn oversized_payload(out_dir: &Path) -> anyhow::Result<()> {
    // Build an artifact that exceeds the 4 KiB inline cap, then reference it.
    let big: String = "X".repeat(8_000);
    let bytes = big.into_bytes();
    let hex = blake3_hex(&bytes);
    let path = artifact_path(&hex);

    let meta = r#"tape_version: "tape/v0"
id: "01h8xy00-0000-7000-b8aa-000000000002"
created_at: "2026-05-06T10:00:00Z"
ejected_at: "2026-05-06T10:00:30Z"
task: "Read a large log"
recorder:
  agent: "claude-code/2.1.4"
outcome: success
"#;

    let tracks = format!(
        "{}\n{}\n{}\n",
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"Read a large log"}}"#,
        format!(
            r#"{{"step":2,"kind":"file_read","ts":"2026-05-06T10:00:10Z","payload":{{"path":"/var/log/app.log","content_hash":"blake3:{hex}","content":{{"ref":"sha:{hex}"}}}},"refs":["sha:{hex}"]}}"#
        ),
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#,
    );

    let mut artifacts = BTreeMap::new();
    artifacts.insert(path, bytes);

    let pending = PendingTape {
        meta_yaml: meta.into(),
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks,
        redactions_json: None,
        artifacts,
    };
    pending.write_to(out_dir.join("oversized-payload.tape"))?;
    Ok(())
}

fn with_mcp_calls(out_dir: &Path) -> anyhow::Result<()> {
    let meta = r#"tape_version: "tape/v0"
id: "01h8xy00-0000-7000-b8aa-000000000003"
created_at: "2026-05-06T10:00:00Z"
ejected_at: "2026-05-06T10:01:00Z"
task: "Investigate payment failures for customer 4471"
recorder:
  agent: "claude-code/2.1.4"
models:
  - vendor: anthropic
    model: claude-opus-4-7
    calls: 2
tools:
  - kind: mcp
    server: "db"
    calls: 1
outcome: success
"#;

    let liner = "## What I was asked to do
Investigate why customer 4471's payments are failing.

## What I found
Smoking gun: a race condition in the refund processor at `process_refund()` in `payments.rs`. Customer ID `CUST-447139` triggers the bug when two refund requests arrive within 50ms of each other.

## Suggested next step / fix
Add an advisory lock around `process_refund()` keyed on customer_id.

## What I'm uncertain about
Whether other adjacent flows (chargeback, partial refund) share the same lock domain.
";

    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"Investigate payment failures for customer 4471"}}"#, "\n",
        r#"{"step":2,"kind":"model_call","ts":"2026-05-06T10:00:15Z","payload":{"vendor":"anthropic","model":"claude-opus-4-7","request":{"messages":[{"role":"user","content":"Investigate"}]},"response":{"content":[{"type":"text","text":"Let me query the payments table."}]}}}"#, "\n",
        r#"{"step":3,"kind":"mcp_call","ts":"2026-05-06T10:00:25Z","payload":{"server":"db","tool":"query","args":{"sql":"SELECT * FROM payments WHERE customer_id=4471 AND status='failed'"},"result":{"rows":3}}}"#, "\n",
        r#"{"step":4,"kind":"annotation","ts":"2026-05-06T10:00:40Z","payload":{"by":"agent","note":"smoking gun: race condition in process_refund() — customer CUST-447139"}}"#, "\n",
        r#"{"step":5,"kind":"model_call","ts":"2026-05-06T10:00:50Z","payload":{"vendor":"anthropic","model":"claude-opus-4-7","request":{"messages":[{"role":"user","content":"summarize"}]},"response":{"content":[{"type":"text","text":"Race condition confirmed in payments.rs"}]}}}"#, "\n",
        r#"{"step":6,"kind":"eject","ts":"2026-05-06T10:01:00Z","payload":{"outcome":"success"}}"#, "\n",
    );

    let pending = PendingTape {
        meta_yaml: meta.into(),
        liner_md: liner.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out_dir.join("killer-scenario-a.tape"))?;
    Ok(())
}

// ----- malformed -----

fn write_expected(path: &Path, codes: &[&str]) -> anyhow::Result<()> {
    let json = serde_json::json!({ "expect_codes": codes });
    std::fs::write(path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

fn malformed_missing_eject(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta("01h8xy00-0000-7000-b8aa-000000000101", "Missing eject", "success");
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"model_call","ts":"2026-05-06T10:00:05Z","payload":{"vendor":"anthropic","model":"x","request":{},"response":{}}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("missing-eject.tape"))?;
    write_expected(&out.join("missing-eject.expected.json"), &["MISSING_EJECT_EVENT"])?;
    Ok(())
}

fn malformed_step_gap(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta("01h8xy00-0000-7000-b8aa-000000000102", "Step gap", "success");
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("step-gap.tape"))?;
    write_expected(&out.join("step-gap.expected.json"), &["STEP_GAP"])?;
    Ok(())
}

fn malformed_unknown_kind(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta("01h8xy00-0000-7000-b8aa-000000000103", "Unknown kind", "success");
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"sneeze","ts":"2026-05-06T10:00:05Z","payload":{}}"#, "\n",
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("unknown-kind.tape"))?;
    write_expected(&out.join("unknown-kind.expected.json"), &["UNKNOWN_KIND"])?;
    Ok(())
}

fn malformed_outcome_mismatch(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta("01h8xy00-0000-7000-b8aa-000000000104", "Outcome mismatch", "success");
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"failure"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("outcome-mismatch.tape"))?;
    write_expected(&out.join("outcome-mismatch.expected.json"), &["OUTCOME_MISMATCH"])?;
    Ok(())
}

fn malformed_artifact_hash_mismatch(out: &Path) -> anyhow::Result<()> {
    // Claim a hash that doesn't match the bytes we actually store.
    let real_bytes = b"hello world".repeat(500); // > 4KiB so spillover is required
    let real_hex = blake3_hex(&real_bytes);
    let _real_path = artifact_path(&real_hex);

    let claimed_hex = "0".repeat(64);
    let claimed_path = artifact_path(&claimed_hex);

    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-000000000105",
        "Artifact hash mismatch",
        "success",
    );
    let tracks = format!(
        "{}\n{}\n{}\n",
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#,
        format!(
            r#"{{"step":2,"kind":"file_read","ts":"2026-05-06T10:00:10Z","payload":{{"path":"/x","content_hash":"blake3:{claimed_hex}","content":{{"ref":"sha:{claimed_hex}"}}}},"refs":["sha:{claimed_hex}"]}}"#
        ),
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#,
    );
    let mut artifacts = BTreeMap::new();
    // Store actual bytes at the *claimed* path so the file exists but its
    // hash doesn't match the path/refs. Verify should catch the mismatch.
    artifacts.insert(claimed_path, real_bytes);

    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks,
        redactions_json: None,
        artifacts,
    };
    pending.write_to(out.join("artifact-hash-mismatch.tape"))?;
    write_expected(
        &out.join("artifact-hash-mismatch.expected.json"),
        &["ARTIFACT_HASH_MISMATCH"],
    )?;
    Ok(())
}

fn malformed_oversized_inline(out: &Path) -> anyhow::Result<()> {
    // An inline string > 4 KiB that is NOT a `{ref: ...}` stub.
    let big: String = "Y".repeat(8_000);
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-000000000106",
        "Oversized inline payload",
        "success",
    );
    let payload = serde_json::json!({"path": "/x", "content": big, "content_hash": "blake3:0"});
    let tracks = format!(
        "{}\n{}\n{}\n",
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#,
        serde_json::to_string(&serde_json::json!({
            "step": 2, "kind": "file_read", "ts": "2026-05-06T10:00:10Z", "payload": payload
        }))?,
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#,
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks,
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("oversized-inline.tape"))?;
    write_expected(
        &out.join("oversized-inline.expected.json"),
        &["OVERSIZED_INLINE_PAYLOAD"],
    )?;
    Ok(())
}

fn malformed_leaked_anthropic_key(out: &Path) -> anyhow::Result<()> {
    // Synthetic key: matches the rule's prefix shape but is obviously test data.
    let leak = "sk-ant-leaked0123456789012345678901234567890123testfixture";
    let liner = format!("## What I was asked to do
Demonstrate the defense-in-depth scan.

## What I found
A leaked Anthropic key was somehow embedded: {leak} (this is a synthetic test value).

## Suggested next step / fix
Flag this fixture as malformed.

## What I'm uncertain about
Nothing.
");
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-000000000107",
        "Leaked anthropic key in liner",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: liner,
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("leaked-anthropic-key.tape"))?;
    write_expected(
        &out.join("leaked-anthropic-key.expected.json"),
        &["LEAKED_SECRET_IN_LINER"],
    )?;
    Ok(())
}

fn malformed_wrong_tape_version(out: &Path) -> anyhow::Result<()> {
    let meta = r#"tape_version: "tape/v9"
id: "01h8xy00-0000-7000-b8aa-000000000108"
created_at: "2026-05-06T10:00:00Z"
ejected_at: "2026-05-06T10:00:30Z"
task: "Future version"
recorder:
  agent: "claude-code/2.1.4"
outcome: success
"#;
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta.into(),
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("wrong-tape-version.tape"))?;
    write_expected(
        &out.join("wrong-tape-version.expected.json"),
        &["WRONG_TAPE_VERSION"],
    )?;
    Ok(())
}

/// SPEC §3.3 / §4.3 / §10.5: every default-enabled built-in rule must be
/// flagged by `tape verify`'s defense-in-depth scan. Three fixtures clone
/// the `leaked-anthropic-key` template for `jwt`, `aws_access_key`, and
/// `email` — the rules that pre-#33 `tape verify` silently let through.
fn malformed_leaked_jwt(out: &Path) -> anyhow::Result<()> {
    let leak = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let liner = format!("## What I was asked to do
Demonstrate the defense-in-depth scan.

## What I found
A leaked JWT was somehow embedded: {leak} (synthetic test value).

## Suggested next step / fix
Flag this fixture as malformed.

## What I'm uncertain about
Nothing.
");
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-00000000010a",
        "Leaked JWT in liner",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: liner,
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("leaked-jwt.tape"))?;
    write_expected(
        &out.join("leaked-jwt.expected.json"),
        &["LEAKED_SECRET_IN_LINER"],
    )?;
    Ok(())
}

fn malformed_leaked_aws_access_key(out: &Path) -> anyhow::Result<()> {
    let leak = "AKIA1234567890ABCDEF";
    let liner = format!("## What I was asked to do
Demonstrate the defense-in-depth scan.

## What I found
A leaked AWS access key id: {leak} (synthetic test value).

## Suggested next step / fix
Flag this fixture as malformed.

## What I'm uncertain about
Nothing.
");
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-00000000010b",
        "Leaked AWS access key in liner",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: liner,
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("leaked-aws-access-key.tape"))?;
    write_expected(
        &out.join("leaked-aws-access-key.expected.json"),
        &["LEAKED_SECRET_IN_LINER"],
    )?;
    Ok(())
}

fn malformed_leaked_email(out: &Path) -> anyhow::Result<()> {
    let liner = "## What I was asked to do
Demonstrate the defense-in-depth scan.

## What I found
Ping the team at alice@example.com for follow-up.

## Suggested next step / fix
Flag this fixture as malformed.

## What I'm uncertain about
Nothing.
";
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-00000000010c",
        "Leaked email in liner",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: liner.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("leaked-email.tape"))?;
    write_expected(
        &out.join("leaked-email.expected.json"),
        &["LEAKED_SECRET_IN_LINER"],
    )?;
    Ok(())
}

/// Three back-to-back `parent_step` violations on one tape: an out-of-range
/// reference, a `parent_step == step` (violates the `< step` rule), and a
/// `parent_step == 0` (out of the `[1, step)` range). All three must fire
/// `INVALID_PARENT_STEP`. See SPEC §5.3 and issue #3.
fn malformed_invalid_parent_step(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-000000000109",
        "Invalid parent_step",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#,
        "\n",
        r#"{"step":2,"kind":"annotation","ts":"2026-05-06T10:00:01Z","payload":{"by":"agent","note":"out of range"},"parent_step":9999}"#,
        "\n",
        r#"{"step":3,"kind":"annotation","ts":"2026-05-06T10:00:02Z","payload":{"by":"agent","note":"self-ref"},"parent_step":3}"#,
        "\n",
        r#"{"step":4,"kind":"annotation","ts":"2026-05-06T10:00:03Z","payload":{"by":"agent","note":"zero"},"parent_step":0}"#,
        "\n",
        r#"{"step":5,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#,
        "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("invalid-parent-step.tape"))?;
    write_expected(
        &out.join("invalid-parent-step.expected.json"),
        &["INVALID_PARENT_STEP"],
    )?;
    Ok(())
}

/// SPEC §5.4 / §11: the event kinds `fork` and `splice` are RESERVED for
/// future revisions and v0 readers MUST reject them. Serde's closed enum
/// would otherwise surface a generic `INVALID_TRACKS_JSON`; the verifier
/// peeks at `kind` and emits `RESERVED_KIND` instead. See issue #60.
fn malformed_reserved_kind_fork(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-00000000010d",
        "Reserved kind: fork",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"fork","ts":"2026-05-06T10:00:05Z","payload":{}}"#, "\n",
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("reserved-kind-fork.tape"))?;
    write_expected(
        &out.join("reserved-kind-fork.expected.json"),
        &["RESERVED_KIND"],
    )?;
    Ok(())
}

fn malformed_reserved_kind_splice(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-00000000010e",
        "Reserved kind: splice",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"splice","ts":"2026-05-06T10:00:05Z","payload":{}}"#, "\n",
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("reserved-kind-splice.tape"))?;
    write_expected(
        &out.join("reserved-kind-splice.expected.json"),
        &["RESERVED_KIND"],
    )?;
    Ok(())
}

/// SPEC §5.4: exactly one `task` event per tape. This fixture has two.
/// Verify must emit `MISSING_TASK_EVENT` for the cardinality violation
/// (the diagnostic code is reused for both "missing" and "too many" —
/// the message names the actual count). See issue #86.
fn malformed_duplicate_task(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-00000000010f",
        "Duplicate task event",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"first"}}"#,
        "\n",
        r#"{"step":2,"kind":"task","ts":"2026-05-06T10:00:01Z","payload":{"prompt":"second"}}"#,
        "\n",
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#,
        "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("duplicate-task.tape"))?;
    write_expected(
        &out.join("duplicate-task.expected.json"),
        &["MISSING_TASK_EVENT"],
    )?;
    Ok(())
}

/// SPEC §5.4: exactly one `eject` event per tape. The existing
/// `EjectNotLast` check fires for any non-final eject; this fixture has
/// two ejects where the final one is the second. The non-final eject
/// also trips `EjectNotLast`, but the duplicate-count check makes the
/// cardinality violation explicit. See issue #86.
fn malformed_duplicate_eject(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-000000000110",
        "Duplicate eject event",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#,
        "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:10Z","payload":{"outcome":"success"}}"#,
        "\n",
        r#"{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#,
        "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("duplicate-eject.tape"))?;
    write_expected(
        &out.join("duplicate-eject.expected.json"),
        &["EJECT_NOT_LAST"],
    )?;
    Ok(())
}

/// SPEC §5.5.1 / issue #96: the `task` event's payload MUST carry a
/// non-empty `prompt`. This fixture's step-1 task event has an explicit
/// empty `prompt`; verify must emit `INVALID_PAYLOAD`.
fn malformed_empty_task_prompt(out: &Path) -> anyhow::Result<()> {
    let meta = std_meta(
        "01h8xy00-0000-7000-b8aa-000000000111",
        "Empty task prompt",
        "success",
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":""}}"#,
        "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#,
        "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("empty-task-prompt.tape"))?;
    write_expected(
        &out.join("empty-task-prompt.expected.json"),
        &["INVALID_PAYLOAD"],
    )?;
    Ok(())
}

/// SPEC §3.1 / issue #68: `created_at` MUST be lexicographically ≤
/// `ejected_at`. This fixture inverts them. The track timestamps remain
/// monotonic so the meta-level check is isolated; verify must emit
/// `BAD_TIMESTAMP`.
fn malformed_meta_timestamps_inverted(out: &Path) -> anyhow::Result<()> {
    // Hand-rolled meta — `std_meta` hardcodes a valid (created < ejected)
    // pair, so we write the YAML directly here.
    let meta = r#"tape_version: "tape/v0"
id: "01h8xy00-0000-7000-b8aa-000000000112"
created_at: "2026-05-06T10:00:30Z"
ejected_at: "2026-05-06T10:00:00Z"
task: "Inverted meta timestamps"
recorder:
  agent: "claude-code/2.1.4"
outcome: success
"#;
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#, "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:01Z","payload":{"outcome":"success"}}"#, "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta.into(),
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("meta-timestamps-inverted.tape"))?;
    write_expected(
        &out.join("meta-timestamps-inverted.expected.json"),
        &["BAD_TIMESTAMP"],
    )?;
    Ok(())
}

/// Issue #105: `meta.recap` is bounded at 280 chars; this fixture writes
/// a 281-char value so `tape verify` emits `META_RECAP_INVALID`. The
/// `recap` field is YAML-quoted to preserve trailing whitespace and to
/// dodge any block-scalar folding shenanigans serde-yaml might apply.
fn malformed_meta_recap_overlong(out: &Path) -> anyhow::Result<()> {
    let overlong: String = "x".repeat(281);
    let meta = format!(
        r#"tape_version: "tape/v0"
id: "01h8xy00-0000-7000-b8aa-000000000113"
created_at: "2026-05-06T10:00:00Z"
ejected_at: "2026-05-06T10:00:30Z"
task: "Overlong recap"
recorder:
  agent: "claude-code/2.1.4"
outcome: success
recap: "{overlong}"
"#
    );
    let tracks = concat!(
        r#"{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"x"}}"#,
        "\n",
        r#"{"step":2,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}"#,
        "\n",
    );
    let pending = PendingTape {
        meta_yaml: meta,
        liner_md: STD_LINER.into(),
        tracks_jsonl: tracks.into(),
        redactions_json: None,
        artifacts: BTreeMap::new(),
    };
    pending.write_to(out.join("meta-recap-overlong.tape"))?;
    write_expected(
        &out.join("meta-recap-overlong.expected.json"),
        &["META_RECAP_INVALID"],
    )?;
    Ok(())
}

fn std_meta(id: &str, task: &str, outcome: &str) -> String {
    format!(
        r#"tape_version: "tape/v0"
id: "{id}"
created_at: "2026-05-06T10:00:00Z"
ejected_at: "2026-05-06T10:00:30Z"
task: "{task}"
recorder:
  agent: "claude-code/2.1.4"
outcome: {outcome}
"#
    )
}
