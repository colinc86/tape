# Investigation — Issue #66

**Title:** SPEC §10.6 missing `LINER_SECTIONS_OUT_OF_ORDER` and `UNKNOWN_ENTRY` — both emitted by `verify` today

**Author:** Principal — 2026-05-13 23:18

## TL;DR

Pure SPEC-text drift. `tape verify` emits two diagnostic codes that SPEC §10.6
never enumerates. Fix is to add both entries to the §10.6 list — no code change
needed today. An engineer-authored PR (#70) already does this, but it is
**CONFLICTING** against `main` and carries no workflow labels, so it needs a
rebase (or supersession) before it can land.

## State of the world on `main` (verified 2026-05-13)

- `crates/tape-format/src/verify.rs:55` — code string `"LINER_SECTIONS_OUT_OF_ORDER"` (Error).
  - Variant declared at line 22, emission at lines 218-221 (post-§5.4 PR #65 the
    line numbers shifted from #66's original reference; the symbol is unchanged).
- `crates/tape-format/src/verify.rs:73` — code string `"UNKNOWN_ENTRY"` (Warning).
  - Variant declared at line 40, emission at lines 166-171.
- `SPEC.md:473` — §10.6 list. Currently:
  ```
  MALFORMED_ZIP, MISSING_REQUIRED_ENTRY, INVALID_META_YAML, WRONG_TAPE_VERSION,
  INVALID_LINER_NOTES, MISSING_LINER_SECTION, INVALID_TRACKS_JSON, STEP_GAP,
  UNKNOWN_KIND, RESERVED_KIND, MISSING_TASK_EVENT, MISSING_EJECT_EVENT,
  EJECT_NOT_LAST, BAD_TIMESTAMP, TS_NOT_MONOTONIC, INVALID_PAYLOAD,
  INVALID_PARENT_STEP, MISSING_ARTIFACT, ARTIFACT_HASH_MISMATCH,
  OVERSIZED_INLINE_PAYLOAD, OUTCOME_MISMATCH, REDACTION_SUMMARY_MISMATCH,
  LEAKED_SECRET_IN_META, LEAKED_SECRET_IN_LINER.
  ```
  - Missing: `LINER_SECTIONS_OUT_OF_ORDER`, `UNKNOWN_ENTRY`.
  - The list already mixes Error and Warning codes — `UNKNOWN_ENTRY` is the only
    confirmed Warning emitted today, so tagging it inline (or restructuring the
    section into Errors/Warnings subsections) is editorial.
- `crates/tape-format/src/verify.rs:42, 75` — `UnsafePath` / `"UNSAFE_PATH"`
  variant is **defined but never emitted**. Reader-level rejection
  (`reader.rs`) blocks unsafe paths before verify runs. **Out of scope for #66
  and #70** — that is the leftover thread from #60 that wants its own cleanup
  ticket (remove from enum + SPEC, or wire emission). Mentioned here so the
  next pass doesn't conflate it with this fix.

## Diff between #66's original suggested patch and what should land now

#66 was filed before PR #65 (RESERVED_KIND) landed, so its diff snippet has
already drifted. The correct patch today:

```diff
 ### 10.6 Diagnostic codes

 Validators that fail SHOULD emit one or more structured diagnostics with these stable codes:

-`MALFORMED_ZIP`, `MISSING_REQUIRED_ENTRY`, `INVALID_META_YAML`, `WRONG_TAPE_VERSION`, `INVALID_LINER_NOTES`, `MISSING_LINER_SECTION`, `INVALID_TRACKS_JSON`, `STEP_GAP`, `UNKNOWN_KIND`, `RESERVED_KIND`, `MISSING_TASK_EVENT`, `MISSING_EJECT_EVENT`, `EJECT_NOT_LAST`, `BAD_TIMESTAMP`, `TS_NOT_MONOTONIC`, `INVALID_PAYLOAD`, `INVALID_PARENT_STEP`, `MISSING_ARTIFACT`, `ARTIFACT_HASH_MISMATCH`, `OVERSIZED_INLINE_PAYLOAD`, `OUTCOME_MISMATCH`, `REDACTION_SUMMARY_MISMATCH`, `LEAKED_SECRET_IN_META`, `LEAKED_SECRET_IN_LINER`.
+`MALFORMED_ZIP`, `MISSING_REQUIRED_ENTRY`, `INVALID_META_YAML`, `WRONG_TAPE_VERSION`, `INVALID_LINER_NOTES`, `MISSING_LINER_SECTION`, `LINER_SECTIONS_OUT_OF_ORDER`, `INVALID_TRACKS_JSON`, `STEP_GAP`, `UNKNOWN_KIND`, `RESERVED_KIND`, `MISSING_TASK_EVENT`, `MISSING_EJECT_EVENT`, `EJECT_NOT_LAST`, `BAD_TIMESTAMP`, `TS_NOT_MONOTONIC`, `INVALID_PAYLOAD`, `INVALID_PARENT_STEP`, `MISSING_ARTIFACT`, `ARTIFACT_HASH_MISMATCH`, `OVERSIZED_INLINE_PAYLOAD`, `OUTCOME_MISMATCH`, `REDACTION_SUMMARY_MISMATCH`, `LEAKED_SECRET_IN_META`, `LEAKED_SECRET_IN_LINER`, `UNKNOWN_ENTRY` (warning).
```

Or, equivalent and slightly more readable, split the list into two subsections
("Errors" / "Warnings") with `UNKNOWN_ENTRY` as the sole entry in the Warnings
subsection. Either form is acceptable; the engineer may pick.

## PR #70 status (must be reconciled before the issue closes)

- `gh pr view 70 --json mergeStateStatus,mergeable` → `DIRTY` / `CONFLICTING`.
- 1 file, 8 additions / 2 deletions — SPEC.md only.
- Authored 2026-05-13 09:12, before #60's PR #65 landed (which extended §10.6
  with `RESERVED_KIND`). The conflict is the §10.6 paragraph.
- Carries **no labels** — same routing meta-gap flagged on PR #32 in TEAM_NOTES
  (Engineer A 16:42, 17:49, 18:19, 18:50).
- PR description proposes a slightly different shape than what #66 originally
  suggested: split §10.6 into Errors/Warnings subsections. That structural
  improvement is fine; the rebase needs to keep `RESERVED_KIND` in the Errors
  bucket and place `UNKNOWN_ENTRY` in the Warnings bucket.

## Recommended ownership

Engineer who picks this up should:
1. Rebase `fix/spec-add-missing-codes` onto current `main` (post-PR #65) and
   resolve the §10.6 conflict, keeping `RESERVED_KIND` in the list.
2. OR open a fresh PR with a clean diff; close #70 as superseded.
3. Either approach: do not touch any source file; this is SPEC-only.

## Out of scope

- Wiring or removing `UNSAFE_PATH` (see #60 follow-up).
- Adding a sibling `UNSAFE_PATH` SPEC entry — emission is dead code today.
- Any new diagnostic codes not already present in `verify.rs`.
- Restructuring §10.6 beyond an Errors/Warnings split is fine; deeper
  reformatting (per-code descriptions, anchor links, etc.) is a separate
  doc-polish ticket.

## Related issues

- #60 (closed) — RESERVED_KIND wired in PR #65. Already landed; only mentioned
  for context on the §10.6 baseline.
- #118 — workflow-label discipline policy; #66 is one of the bugs being
  promoted under that policy this tick.
- PR #70 — the existing fix attempt. Conflicting; no labels.
