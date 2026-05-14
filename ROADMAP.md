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

## Current Milestone — v0.2 (Claude Desktop + diff intelligence)

**Status:** Not started. Promoted from Next Milestone after v0.1.2 shipped
2026-05-14. No tickets cut yet — Principal will decompose the five themes
below into `priority:current` issues at the right time.

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

## Next Milestone — v0.3 (multi-runtime + ecosystem)

Once the format is proven on two runtimes (Claude Code + Claude Desktop in
v0.2), v0.3 broadens the surface and seeds the hosted-registry direction:

- **Codex / OpenAI Agents adapter** and **OpenClaw adapter** — third and
  fourth runtimes.
- **Cursor adapter** (#106) — slipping forward if it lands as a v0.2 stretch.
- **`tape splice`** — surgical edit of a single track's payload, preserving
  structure.
- **Hosted cassette registry** — the WebAssembly `tape verify` build (#107)
  is the gating prerequisite for in-browser verification.
- **Cross-platform binary distribution.** v0.1.x ships macOS-Apple-Silicon
  only; v0.3 broadens to at least Linux x86_64 + macOS Intel.

---

## Backlog

Open enhancement issues grouped by theme. Principal converts these into
engineering tickets at the right time; PM keeps the buckets and reorders.

**Priority labels** (per #118 / #126 — Principal's workflow-label discipline):

- `priority:current` — must land in the current milestone (now v0.2).
- `priority:next` — explicit v0.3 work, or v0.2 stretch (currently #74,
  #81, #106).
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

### v0.1.2 — 2026-05-14 — Spec-compliance rollup

A pure bug-fix release closing the next thirty-plus findings after v0.1.1,
dominated by SPEC enforcement gaps that `tape verify` was silently letting
through (six previously-undetected MUSTs are now enforced). All four
`priority:current` milestone bugs closed: #26 (fork+eject double terminator
→ #32), #66 (SPEC §10.6 missing codes → #125), #68 (created_at ≤
ejected_at → #123), #109 (`meta.tool_budget` populated → #119). No format
or behavior changes; tapes and plugin installs from v0.1.x continue to
work unchanged. Full notes in RELEASE_NOTES.md.

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
