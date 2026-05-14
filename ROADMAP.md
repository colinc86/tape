# Roadmap

## Vision

A cassette tape for agent runs. **Record once, replay anywhere, share as a
file.** `tape` captures the messiest artifact in software — an AI agent's
actual investigation — into a single portable file that another agent or
engineer can rewind to exactly where you left off. The format (`tape/v0`) is
load-bearing; everything else is tooling on top of it.

The three-year arc:

1. **v0.x** — Claude Code is the only runtime. Prove the cassette metaphor.
2. **v0.2–v0.3** — Multi-runtime (Claude Desktop, Codex, Cursor, OpenClaw).
   Cassettes survive runtime switches because the format is runtime-neutral.
3. **v1.x** — Hosted registry + cross-tool ecosystem (CI actions, IDE
   integrations, observability bridges). The `.tape` extension becomes the
   default unit of agent-run exchange.

---

## Current Milestone — v0.1.2 (patch rollup)

**Status:** **Ready to ship.** All `priority:current` bugs closed. Cut is
pending: cargo bump + RELEASE_NOTES prose + binary rebuild + tag +
`gh release create`. Next PM tick can execute (the ready-to-ship state was
blocked this tick because the prior ROADMAP snapshot was stale).

A patch release that aggregates ~32 backward-compatible fixes merged since
v0.1.1 (2026-05-07). No format or behavior changes; existing tapes and plugin
installs continue to work unchanged.

Headline fixes (full list will go in RELEASE_NOTES at release time):

- **Spec compliance:** `tape verify` now enforces SPEC §3.1 (created_at ≤
  ejected_at — #123), §5.4 (exactly one task / eject — #87), §5.5.1
  (non-empty task prompt — #98), §9.1 (`deny_unknown_fields` on RedactConfig —
  #40); UNKNOWN_KIND / RESERVED_KIND diagnostics wired (#65, #92); full
  built-in rule set scanned in defense-in-depth (#38); meta.label redacted
  before scan (#79); `meta.tool_budget` populated at eject time (#119);
  `tape.fork` at last step + `tape.eject` no longer produces two eject events
  (#32).
- **Recorder / hook correctness:** `tape-hook` streams content hashing via
  blake3::Hasher (#52); `PreToolUse` hook populates `file_write.before_hash`
  (#57); `NotebookEdit` covered by overlay matchers and hook dispatch (#76,
  #84); HTTP failure status recorded on proxied `model_call` (#24);
  `parent_step` validated on writer + verifier (#19).
- **Deck / MCP:** `tape.play` resolves `{ref: sha:...}` stubs (#48); `tool_eject`
  carries inherited artifacts (#46) and preserves the loaded tape's
  `meta.label` (#82); per-event timestamps preserved through `tool_eject` (#25)
  and `tape.snapshot` (#16); JSON-RPC notifications suppressed per §4.1 (#59);
  `tape.eject` accepts an optional `outcome` arg (#35); `tape-mcp-wrap`
  PENDING_TTL raised to 1h (#55); `tape.seek` no longer panics on non-ASCII
  (#12); `Session::append_at` preserves `parent_step`, refs, annotations (#54).
- **Redaction:** `.taperc` loaded on every recording path (#29); engine rules
  used for eject defense-in-depth (#27); `disable_default` rule names validated
  (#50); oversize arrays and objects spilled to `artifacts/` (#4).
- **Diff CLI:** `--judge` flag rejected with a clear error until narration
  lands (#64); `last_answer` restricted to agent annotations (#22).
- **Surfacing:** `--label` reaches `meta.yaml` (#73).

### Release blockers

- [ ] All `priority:current` bugs merged (see snapshot below).
- [ ] Bump `[workspace.package].version` to `0.1.2` in `Cargo.toml`.
- [ ] Update RELEASE_NOTES.md with a written changelog (prose, not commit
  titles).
- [ ] Rebuild macOS-Apple-Silicon binaries (`tape`, `tape-hook`,
  `tape-mcp-wrap`) for the plugin marketplace.
- [ ] Cut tag `v0.1.2` + `gh release create` with tarball + SHA256SUMS.

### Open `priority:current` bugs

This roadmap intentionally does **not** enumerate open bugs as a static list —
it goes stale within hours (it already did, see #129). The source of truth is
the live label set:

```
gh issue list --label priority:current --label kind:bug --state open
```

PM regenerates the snapshot below on every ROADMAP refine; engineers don't
take it as a contract.

Snapshot at **2026-05-14 05:35 UTC**:

> `gh issue list --label priority:current --label kind:bug --state open`
> returns **empty**.

All v0.1.2 milestone bugs are closed:

- #26 (severity:medium, fork+eject double terminator) — closed via merged
  PR #32.
- #66 (severity:low, SPEC §10.6 missing diagnostic codes) — closed via
  merged PR #125.
- #68 (severity:low, created_at) — closed via merged PR #123.
- #109 (severity:low, tool_budget) — closed via merged PR #119.

Remaining release work (next PM tick, option (b)):

1. Bump `[workspace.package].version` to `0.1.2` in `Cargo.toml`.
2. Prepend a v0.1.2 prose changelog to `RELEASE_NOTES.md` (matching the
   v0.1.1 style: brief intro + grouped fixes + Known limitations carried
   from v0.1.1).
3. Update `README.md` status badge `v0.1.1` → `v0.1.2`.
4. Commit `pm: release v0.1.2`, tag `v0.1.2`, push commit + tag, then
   `gh release create v0.1.2 --notes-file <prose>`.
5. (Outside PM scope) Rebuild macOS-Apple-Silicon binaries and upload the
   tarball + SHA256SUMS to the GitHub release; bump the plugin marketplace
   entry.

---

## Next Milestone — v0.2 (Claude Desktop + diff intelligence)

The first **non-patch** release. Five themes — runtime expansion, narration,
embedding-driven diff, interactive eject, and liner-notes-at-eject — all
already named in README's tracklist and RELEASE_NOTES's deferred list.

1. **Claude Desktop adapter.** Same `tape/v0` format, second runtime. Validates
   that the format is runtime-neutral. No changes to `tape-format` or the deck
   are expected; effort is in the recording/transcript-ingest layer.
2. **Interactive eject prompt.** The `[y/n/d/e]` confirmation flow described
   in the brief but unimplemented in v0.1.x. Lets the user inspect proposed
   redactions before the tape lands on disk.
3. **Embedding-based diff alignment.** Replace the v0.1 LCS aligner with
   Needleman-Wunsch on step-intent embeddings (see `tape-diff` skill). Better
   diffs for non-trivial reruns.
4. **Judge-model narration.** Re-enable the `--judge` flag (#62/#64 stub).
   Narrates the substantive diffs as short paragraphs.
5. **Liner-notes-at-eject.** Configurable model + token budget; replaces the
   stub liner notes that ship today when no model is available.

Stretch items for v0.2:

- **Causal-flow detection in diff** (the `causal` class in the schema today
  but never produced).
- **`tape annotate` CLI** (#74) — CLI counterpart to the MCP `tape.annotate`
  tool. Cheap once the deck infra is in place; closes a parity gap.
- **`tape doctor`** (#81) — install-surface diagnostic with pass/warn/fail
  report. Reduces onboarding pain as the runtime list grows.

---

## Backlog

Open enhancement issues grouped by theme. Principal converts these into
engineering tickets at the right time; PM keeps the buckets and reorders.

**Priority labels** (per #118 / #126 — Principal's workflow-label discipline):

- `priority:current` — must land in the current milestone (v0.1.2). Bug
  fixes scoped to release.
- `priority:next` — explicit v0.2 work (currently #74, #81, #106).
- `priority:later` — backlog. Live count via
  `gh issue list --label priority:later --state open` (27 as of 2026-05-13).

### Multi-runtime + ingest (the v0.3 direction)

- **#106** — `RuntimeAdapter` trait + Cursor adapter. The missing growth lever
  beyond Claude Code; should land alongside Claude Desktop in v0.2 if Cursor
  parity is cheap.
- **#95** — `tape ingest`: import LangSmith / OTLP / Langfuse / Helicone /
  OpenLLMetry / Phoenix traces. The "meet users where they are" play.
- **#88** — `tape to-otlp`: export a cassette as OpenTelemetry traces.
  Bidirectional with #95.
- **#102** — `tape to-fixture`: extract HTTP pairs to VCR / Polly / HTTPretty /
  JSONL test fixtures.

### Registry + distribution (the v0.3 direction)

- **#107** — WebAssembly build of `tape verify` (browser/Node/Deno npm
  package). Registry precursor; a hosted registry needs in-browser verify.
- **#108** — `tape self-update`: keep `tape` / `tape-hook` / `tape-mcp-wrap`
  current with checksum verification + atomic rollback. Unblocks frequent
  releases.
- **#90** — `colinc86/tape-action`: turnkey GitHub Action wrapping `tape test`
  and adjacent commands for CI.
- **#81** — `tape doctor`: install-surface diagnostic (see Stretch in v0.2).

### Cassette editing + synthesis

- **#61** — `tape merge`: combine cassettes with renumbered tracks and a
  provenance ledger.
- **#51** — `tape compact`: configurable lossy size-reduction with an
  auditable transform ledger.
- **#85** — `tape rewind`: reconstruct the file tree as the recorded agent
  saw it at any step.
- **#99** — `tape new`: cassette generator with bundled templates (the
  `cargo new` for `.tape` files).
- **#71** — `tape relinernote`: regenerate liner notes for existing cassettes
  with a configurable model + template.
- **#42** — `tape anon`: strip identifiers (paths, usernames, internal IDs)
  for publishable cassettes.
- **#89** — `tape encrypt` / `tape decrypt`: age-based confidentiality,
  orthogonal to `tape sign`.

### Read / inspect / dashboard

- **#67** — `tape view`: interactive TUI for browsing a cassette (htop for
  `.tape`).
- **#100** — `tape watch`: live dashboard for in-flight recordings (the
  during-record counterpart to `tape view --follow`).
- **#101** — `tape replay`: timeline-driven step-through (the missing verb
  behind "Record once, replay anywhere").
- **#31** — `tape stats`: read-only analytics over a cassette and across a
  library.
- **#78** — `tape playlist`: named, ordered collections of cassettes that
  other commands can operate on as a unit.

### Summarization + narration

- **#105** — `tape recap`: 1-2 sentence regenerable summary for paste-into-
  Slack/Linear/Jira/PR contexts.
- **#103** — `tape changelog`: model-narrated multi-cassette changelog
  (release notes / sprint retro / incident summary).

### Tagging + policy + custom rules

- **#93** — `tape tag`: structured semantic tags via new `meta.tags[]` field +
  add/remove/list/auto subcommand.
- **#110** — `tape policy`: declarative cassette-compliance checker
  (signed/anon/tagged/recap/etc.) for team enforcement.
- **#104** — `tape redact-test`: canary-fuzz CLI for custom redaction/anon
  rules (FP/FN report, JUnit for CI).

### Parity gaps

- **#74** — `tape annotate` CLI (see Stretch in v0.2).

---

## Recently Shipped

### v0.1.1 — 2026-05-07 — Audit cleanup

20 findings from a three-agent audit. No format or behavior changes. Test
count 88 → 106. Notable: `aws_secret_key` redaction rule, custom replacement
validation, 100× decompression-bomb limit, `ALREADY_RECORDING` enforcement in
the deck, per-field meta redaction, recorder socket idle timeout,
`tape-mcp-wrap` shutdown ordering. Full notes in RELEASE_NOTES.md.

### v0.1 — 2026-05-06 — In-session recording

`tape.snapshot` MCP tool — record a Claude Code session into a `.tape` file
from inside the session, in one MCP call. Plugin v0.2.0 ships the in-session
flow; 8 fixture transcripts cover the parse/convert path.

### v0 — 2026-05-04 — Initial release

The format spec, CLI, deck (MCP server), and recording subsystem shipped
together. Single target runtime: Claude Code. 11-tool deck, 68 tests, killer
scenario passes end-to-end.
