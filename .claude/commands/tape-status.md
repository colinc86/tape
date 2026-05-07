---
description: Print the v0 Definition-of-Done checklist with per-item status auto-derived from filesystem and tests.
---

Print a status board for `tape` v0. Walk the Definition of Done from `TAPE_V0_BRIEF.md` and the build order from the `tape-build-order` skill. For each item, derive the current state from the filesystem and (cheap) test runs.

## How to derive each item

| DoD item | Signal |
|---|---|
| `SPEC.md` complete | `SPEC.md` exists; word count >500; contains H2 sections matching the brief's required content. |
| `tape record` works | `cargo run -- record --help` succeeds; integration test `record_smoke` passes. |
| `tape verify` works | `cargo run -- verify --help` succeeds; runs against every fixture in `tests/fixtures/`. |
| `tape play` / `tape ls` | binaries respond to `--help`; snapshot tests for `play`/`ls` pass. |
| `tape diff` works | `cargo run -- diff --help`; integration test `diff_basic` passes. |
| `tape mcp` exposes 11 tools | run `mcp-deck-tester` agent (cheap form: only check `tools/list` length). |
| Redaction unit tests | `cargo test --test redact_unit` passes. |
| Custom `.taperc` works | integration test `redact_custom` passes. |
| Streaming non-buffered | run `proxy-stream-tester` agent (skip if no proxy yet). |
| Hook overlay clean | integration test `hook_cleanup_on_sigkill` passes. |
| Killer scenario test | integration test `killer_scenario` passes. |
| README walkthrough | `README.md` exists; manual gate — print "manual review required" until checked off explicitly. |

## Output shape

```
tape v0 status — <ISO date>

  Build order cursor: step <N> in flight

  [✓] 1. SPEC.md
  [✓] 2. tape verify
  [✓] 3. fixture tapes (3 files)
  [▸] 4. tape play / tape ls       ← in flight
  [ ] 5. Anthropic proxy
  [ ] 6. MCP wrapper
  ...

  Definition of Done
  [✓] SPEC.md complete
  [▸] tape record produces valid tape
  [ ] tape verify validates and rejects
  [✓] tape play / ls readable output
  ...

  Open decisions:  3   (DECISIONS.md entries)
  Open audits:     1   (audits/spec-impl-2026-05-06.md → 2 drift items)
  Test count:      127 unit + 8 integration (last green run: <ts>)
```

Don't run expensive things (don't trigger the killer scenario test in `/tape-status` — that's its own command). Default mode is fast: <10 seconds.

If a signal can't be cheaply derived, mark the item `[?] unknown — run /tape-<thing>`. Do NOT guess green.

If the user passes `verbose`, expand each item with the file paths or test names that drove the verdict.
