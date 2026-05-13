# Investigation: #26 — `tape.fork` at last step + `tape.eject` produces invalid tape (two eject events)

**Date:** 2026-05-13
**Investigator:** Principal (automated tick)
**Issue:** https://github.com/colinc86/tape/issues/26

## Confirmation of bug report

The bug report at #26 is accurate. I re-read the relevant code paths against `main @ 926c5c3` and confirm both the diagnosis and the fix-shape options. The only drift from the original report is line numbers (the report's references were captured against an earlier commit).

## Updated code references (against `main @ 926c5c3`)

- `crates/tape-mcp/src/tools.rs:578-602` — `tool_fork`. Truncates `forked.tracks` to `from_step` and sets `forked.recording = false`. No check that the kept tail terminates in `Kind::Eject`.
  - L590-596: `from_step` bounds check accepts `from_step == source.tracks.len()`.
  - L599: `forked.tracks.truncate(from_step as usize);` — `truncate(N)` keeps `[0..N)`; for a source whose final track is an eject and `from_step == len`, the eject is retained.
  - L600: `forked.recording = false;` — separate UX question (see "Adjacent point").
- `crates/tape-record/src/eject.rs:95-109` — eject pipeline appends a new `Kind::Eject` event at `step = snap.tracks.len() + 1` unconditionally. No check for an existing terminator on the input snapshot.
- `crates/tape-format/src/verify.rs:341-345` — `EJECT_NOT_LAST` fires when an eject is found in a non-final position.
- `crates/tape-format/src/verify.rs:361-367` — `EJECT_NOT_LAST` also fires when `eject_count > 1` (added in PR #87 to enforce SPEC §5.4 cardinality). Both paths catch the corrupt tape; the user sees the diagnostic either way.
- `SPEC.md` §5.4 — "Exactly one [eject], MUST be the final line."

## Reproduction (confirmed, not run end-to-end)

The original issue's unit-test sketch is correct and would fail today. Mental walkthrough:

1. `tape.load` a 3-track tape `[Task, ModelCall, Eject]` → `Loaded { tracks: 3, recording: false }`.
2. `tape.fork { from_step: 3 }` → passes bounds check (3 <= 3). `truncate(3)` is a no-op; forked has `[Task, ModelCall, Eject]`.
3. `tape.eject { handle: h1, out: ... }`:
   - `eject::eject` runs redaction, then appends a new Eject at `step = 4`.
   - tracks become `[Task(1), ModelCall(2), Eject(3), Eject(4)]`.
   - Write succeeds. No error surfaced to caller.
4. `tape verify` on the produced file:
   - `eject_count == 2` → `EJECT_NOT_LAST: tape contains 2 eject events; SPEC §5.4 requires exactly one`.
   - Also `EJECT_NOT_LAST: eject event at step 3 is not the last event`.

## Principal decision: take Option B (defensive normalisation in the eject pipeline)

The issue offered Option A (strip in `tool_fork`) and Option B (strip in the eject pipeline) and recommended B as the durable backstop. I confirm B as the canonical fix:

- **B is the choke point.** All `.tape` files exit through `crates/tape-record/src/eject.rs::eject`. Any future path that produces tracks ending in an eject (`tool_fork`, `tool_splice`, custom socket clients via `socket.rs` `WireKind::Eject`, hand-built `Session`s in tests) gets normalised for free.
- **A alone is insufficient.** Even with A, a recorder client that pushes `{"kind":"eject"}` over the recorder socket (allowed today per `crates/tape-record/src/socket.rs`'s `WireKind::Eject` accept-list) and then triggers an eject would re-introduce the corruption.
- **Both is fine, but A becomes a nice-to-have.** A is still defensible as a "fail fast at fork time" UX choice, but it's not load-bearing once B exists.

### Adjacent point: `forked.recording = false` is correct, do not change as part of this issue

The original issue questioned whether `forked.recording = false` should be flipped to `true`. After reading the deck's recording-contract enforcement (`tools.rs:610` — `state.any_recording()` refuses a second concurrent recording per session), I believe `false` is correct: a fork is a read/edit handle; the deck contract makes a future `tape.record` start its own recording. Flipping to `true` would conflict with that contract and require additional changes. **Out of scope for this ticket.**

### Adjacent point: outcome handling is already correct

The original issue speculated about `tool_eject`'s hardcoded `Outcome::Success`. That was fixed by PR #35 / #36 — `tool_eject` now accepts an optional `outcome` arg defaulting to `Outcome::Unknown` (`tools.rs:692-705`). **Resolved upstream; nothing to do here.**

## Suggested patch shape

In `crates/tape-record/src/eject.rs`, immediately before the existing `next_step` calculation at L100:

```rust
// SPEC §5.4: exactly one eject, last event. If the snapshot already
// ends with an eject (e.g. from a forked handle that retained the source's
// terminator, or a recorder socket client that posted Kind::Eject), drop
// it so we don't emit two terminators. The freshly-built eject below is
// authoritative for this recording's outcome/ts.
if matches!(snap.tracks.last().map(|t| t.kind), Some(Kind::Eject)) {
    snap.tracks.pop();
}
```

The patch is local; no signature changes; no callers affected. Existing tests stay green because no production path today produces a snapshot ending in `Kind::Eject` outside the bug scenario.

## Test plan

1. **Unit test (eject pipeline-level).** Build a `Session`-equivalent snapshot whose tail is `[Task, ModelCall, Eject]`, hand it to `eject::eject`, assert the produced tape has exactly one eject and passes `verify`.
2. **Integration test (deck-level).** Drive the JSON-RPC surface: `tape.load` a fixture → `tape.fork from_step=N` (N = source.tracks.len()) → `tape.eject` to a tempfile → open with `RawTape` → assert `verify` clean and `eject_count == 1`.
3. **Defensive test (socket-level).** A `Session` that receives a `Kind::Eject` over the recorder socket then triggers eject — same assertion. (Optional; lower priority because it's a defence-in-depth scenario.)
4. **Regression.** All existing eject tests stay green; no change to non-bug paths.

## Out of scope (for this ticket)

- `forked.recording = false` vs `true` — file separately if you want to revisit.
- `WireKind::Eject` acceptance on the recorder socket — Option B subsumes the problem; tightening the socket API is a separate hardening ticket.
- Per-event `outcome` merge rules when stripping the source's eject (the source's outcome is silently dropped; the new eject's outcome is the caller's `tape.eject` arg). Document if needed, but no behavioural change required.
