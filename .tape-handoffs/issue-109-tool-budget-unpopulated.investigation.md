# Investigation — #109 meta.tool_budget always None

Principal triage, 2026-05-13. Read-only — no production code touched.

## Summary

The bug report is correct and reproducible by inspection. `tool_budget` is
hard-coded `None` at the only eject site, and `tape diff` always renders
`Latency: 0 ms / 0 ms / Δ0%` for any pair of real tapes as a result.

## Verified findings

1. **Single hard-coded `None`.** `crates/tape-record/src/eject.rs:156` is the
   only construction of `Meta.tool_budget` in the workspace
   (`rg "tool_budget" crates/` → one write, several reads). Fixing this one
   site fixes every recording path, including `tape.snapshot`, because that
   path converts a transcript into a `SessionSnapshot` and then calls into
   `eject.rs` (single Meta construction).

2. **`ToolBudget` struct exists and serializes correctly.**
   `crates/tape-format/src/meta.rs:67-73` — `total_calls`, `total_tokens_in`,
   `total_tokens_out`, `wall_clock_ms` (all `u64`). Default and Eq derived;
   the YAML shape matches SPEC §3.2. No format change required.

3. **All inputs are available at eject time, in better form than the issue
   suggested.** The bug report proposed re-parsing `created_at` /
   `ejected_at` as RFC3339 strings. Cleaner: `eject.rs:98` holds `now =
   chrono::Utc::now()` and `snap.created_at` is already a
   `chrono::DateTime<Utc>` (see `Session::snapshot` at
   `crates/tape-record/src/session.rs:123`). So:

   ```rust
   let wall_clock_ms = (now - snap.created_at).num_milliseconds().max(0) as u64;
   ```

   No string round-trip, no parse failure path, no `unwrap_or(0)` swallowing
   bugs.

4. **`Kind` enum matches the report.** `crates/tape-format/src/tracks.rs:23`
   — variants `Task, ModelCall, McpCall, Shell, FileRead, FileWrite,
   Annotation, Eject`. Pattern `Kind::ModelCall | Kind::McpCall | Kind::Shell`
   is the right "billable call" set; this matches `tape-diff`'s own
   `a_calls` / `b_calls` computation at `crates/tape-diff/src/lib.rs:104-111`,
   which gives us a free consistency check (see test plan).

5. **Token-count availability.** `tokens_in` / `tokens_out` live in the
   `model_call` event payload as `serde_json::Value` keys. They're populated
   by the HTTP proxy when the upstream response carries usage. Today they're
   not always present (proxy failure path, or non-Anthropic vendors), so a
   sum of `payload.get("tokens_in").and_then(Value::as_u64).unwrap_or(0)` is
   honest: 0 tokens means we don't know, not that the model returned zero.
   No code today inspects token totals for routing decisions, so emitting 0
   is safe.

6. **`tape-diff` consumer already degrades silently.**
   `crates/tape-diff/src/lib.rs:112-113`:
   ```rust
   let a_lat = a_meta.tool_budget.map(|b| b.wall_clock_ms).unwrap_or(0);
   let b_lat = b_meta.tool_budget.map(|b| b.wall_clock_ms).unwrap_or(0);
   ```
   Once `tool_budget` is populated, `diff_integration.rs:52` (which only
   asserts the shape is an object) keeps passing; the rendered `Latency:`
   line in `crates/tape-diff/src/lib.rs:400-402` starts showing real values.

## Design call to make

The issue raises the question: when `tokens_in` / `tokens_out` are absent on
every model_call, should we (a) emit `tool_budget` with `total_tokens_*: 0`,
or (b) omit `tool_budget` entirely?

Principal recommends **(a) — always emit `tool_budget`**. Rationale:

- The "Latency" half is always meaningful — wall-clock comes from
  `created_at`/`ejected_at`, which are mandatory.
- The "tokens" half being zero is no worse than `tool_budget: None`
  (consumers already see zero), and it lets `tape diff`'s render be
  unconditional.
- Keeps the implementation a single Meta construction with no branching.

If the engineer disagrees, the alternative (omit when both token sums are 0
**and** there are no model_call events) is fine too — flag in the PR.

## Out of scope for this ticket

- Populating `tokens_in`/`tokens_out` on model_call events that don't have
  them today — that's a separate proxy-side change.
- Reworking `pct_delta`'s `0 → 0 → 0%` behaviour. `tape-diff` is already
  correct for the no-data case.
- Backfilling `tool_budget` on existing tapes — old tapes will still parse
  fine because `Meta.tool_budget` is `Option`.

## Risk

Very low. One construction site, additive only, the consuming code path
already handles both `Some(…)` and `None`. No format migration. The only
behaviour change is that `tape diff` starts producing non-zero latency
values, which is a strict improvement.

## Files of interest

- `crates/tape-record/src/eject.rs:98` — `now` and `ejected_at` constructed.
- `crates/tape-record/src/eject.rs:143-162` — Meta construction; `tool_budget: None` at :156.
- `crates/tape-record/src/eject.rs:325-353` — `summarize_models` pattern to mirror.
- `crates/tape-format/src/meta.rs:67-73` — `ToolBudget` shape.
- `crates/tape-record/src/session.rs:123` — `SessionSnapshot` (carries `created_at` as `DateTime<Utc>`).
- `crates/tape-format/src/tracks.rs:23-32` — `Kind` variants.
- `crates/tape-diff/src/lib.rs:112-113, 400-402` — consumer (Latency line).
- `crates/tape-cli/tests/diff_integration.rs:52` — existing shape assertion (will still pass).
- `SPEC.md` §3.2 — documented `tool_budget` shape.

## Recommended test plan

- New unit in `crates/tape-record/src/eject.rs` (or an integration in
  `crates/tape-record/tests/`): record a small synthetic session with two
  `model_call` events (one carrying `tokens_in: 100, tokens_out: 50`), eject,
  parse `meta.yaml`, assert `tool_budget` is populated with
  `total_calls == 2`, `total_tokens_in == 100`, `total_tokens_out == 50`,
  `wall_clock_ms >= 0`.
- Round-trip diff test in `crates/tape-cli/tests/diff_integration.rs`:
  build two tapes with known latencies (e.g., sleep between session start
  and eject), assert the rendered Latency line reflects them.
- Edge: a tape with zero `model_call` events still gets `tool_budget`
  with all zero fields and a non-zero `wall_clock_ms`.
- Consistency: assert `tool_budget.total_calls` equals the count
  `tape-diff` computes at `crates/tape-diff/src/lib.rs:104-111` for the
  same tape (guards against drift between the two definitions of
  "billable call").
