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

**Status:** In flight. Five Phase-1 feature drops have already landed on
`main` since v0.1.2 (2026-05-14 06:22Z); the five headline themes are
where the bulk of work still sits. v0.2.0 is **not** ready to cut — at
least one headline theme should land first so the release means something
beyond "incremental command additions."

The first **non-patch** release. Five **headline themes** plus a growing
set of **Phase-1 feature drops** landing alongside them. Both must be
considered in scope.

### Headline themes (the original v0.2 definition)

1. **Claude Desktop adapter.** Same `tape/v0` format, second runtime.
   Validates that the format is runtime-neutral. Effort sits in the
   recording / transcript-ingest layer.
   *Status:* `RuntimeAdapter` trait + `ClaudeCodeAdapter` shipped via
   merged PR #143 (closing #106). This is the precursor — the
   `ClaudeDesktopAdapter` concrete implementation is still to do. No
   issue filed yet.
2. **Interactive eject prompt.** The `[y/n/d/e]` confirmation flow
   described in the brief but unimplemented in v0.1.x. Lets the user
   inspect proposed redactions before the tape lands on disk.
   *Status:* Not started. No issue filed.
3. **Embedding-based diff alignment.** Replace the v0.1 LCS aligner
   with Needleman-Wunsch on step-intent embeddings (see `tape-diff`
   skill). Better diffs for non-trivial reruns.
   *Status:* Not started. No issue filed. Depends on the embedding
   provider integration that #145 also needs.
4. **Judge-model narration.** Re-enable the `--judge` flag (#62/#64
   stub). Narrates the substantive diffs as short paragraphs.
   *Status:* Foundational work scoped — **#145 (judge-model client +
   config + defense-in-depth scanner)** is `ready` for Engineering.
   The narration UX itself is a follow-on after #145.
5. **Liner-notes-at-eject.** Configurable model + token budget;
   replaces the stub liner notes that ship today when no model is
   available.
   *Status:* Not started. No issue filed. Will share the model-client
   infrastructure from #145.

### Phase-1 feature drops (shipped to `main`, awaiting v0.2.0 cut)

These are user-facing CLI subcommands that have landed since v0.1.2.
By strict semver they require a minor bump; they're being staged on
`main` until v0.2.0 is cut alongside at least one headline theme.

- **`tape annotate` CLI** (#74 → merged PR #141). CLI counterpart to
  the MCP `tape.annotate` deck tool — closes a parity gap.
- **`tape doctor`** (#81 → merged PR #140). Install-surface diagnostic
  with pass/warn/fail report.
- **`tape recap`** (#105 → merged PR #142). 1–2 sentence regenerable
  summary for paste-into-Slack/Linear/Jira/PR contexts.
- **`tape new`** (#99 → merged PR #146). Cassette generator with
  bundled templates — the `cargo new` for `.tape` files.
- **`RuntimeAdapter` trait + `ClaudeCodeAdapter`** (#106 → merged PR
  #143). Step 1 of the Claude Desktop adapter; the second-runtime
  concrete impl is the v0.2.0 gating piece.

### Cut criteria for v0.2.0

v0.2.0 ships when **all** of the following are true:

- At least one headline theme has its user-visible behavior on `main`
  (the headline-theme bar is "user can do something new at the CLI or
  in the deck," not "infrastructure exists"). Today: zero met.
- All open `priority:current` issues are closed.
- `cargo test --workspace` clean on `main`.
- Binary distribution gap (#144) resolved — v0.2.0 release page ships
  tarball + `SHA256SUMS`, and the plugin marketplace entry is bumped
  to match.

The Phase-1 features will travel as part of v0.2.0 regardless of which
headline theme actually triggers the cut — they're already user-facing
on `main` and shouldn't sit unreleased indefinitely.

### Stretch items for v0.2.x (post-v0.2.0)

- **Causal-flow detection in diff** (the `causal` class in the schema
  today but never produced). Pairs with the embedding-based diff
  alignment work.

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
- `priority:next` — explicit v0.2 / v0.3 work. Live: `gh issue list
  --label priority:next --state open` (currently #145 — judge-model
  foundational).
- `priority:later` — backlog. Live count via
  `gh issue list --label priority:later --state open` (~27).

### Multi-runtime + ingest (the v0.3 direction)

- **#95** — `tape ingest`: import LangSmith / OTLP / Langfuse / Helicone /
  OpenLLMetry / Phoenix traces. The "meet users where they are" play.
- **#88** — `tape to-otlp`: export a cassette as OpenTelemetry traces.
  Bidirectional with #95.
- **#102** — `tape to-fixture`: extract HTTP pairs to VCR / Polly / HTTPretty /
  JSONL test fixtures.

> #106 (`RuntimeAdapter` trait) closed via merged PR #143 — now part of
> the v0.2 "Claude Desktop adapter" headline theme. Concrete adapters
> (Cursor, Codex, OpenClaw) remain open in spirit but are not separately
> ticketed yet.

### Registry + distribution (the v0.3 direction)

- **#144** — v0.1.2 binaries not shipping (PM-filed, blocks any user
  install of v0.1.2+). Pre-requisite for the rest of this bucket since
  it forces the release-asset pipeline to exist.
- **#107** — WebAssembly build of `tape verify` (browser/Node/Deno npm
  package). Registry precursor; a hosted registry needs in-browser verify.
- **#108** — `tape self-update`: keep `tape` / `tape-hook` / `tape-mcp-wrap`
  current with checksum verification + atomic rollback. Unblocks frequent
  releases.
- **#90** — `colinc86/tape-action`: turnkey GitHub Action wrapping `tape test`
  and adjacent commands for CI.

> #81 (`tape doctor`) closed via merged PR #140 — see Phase-1 feature
> drops in v0.2.

### Cassette editing + synthesis

- **#61** — `tape merge`: combine cassettes with renumbered tracks and a
  provenance ledger.
- **#51** — `tape compact`: configurable lossy size-reduction with an
  auditable transform ledger.
- **#85** — `tape rewind`: reconstruct the file tree as the recorded agent
  saw it at any step.
- **#71** — `tape relinernote`: regenerate liner notes for existing cassettes
  with a configurable model + template.
- **#42** — `tape anon`: strip identifiers (paths, usernames, internal IDs)
  for publishable cassettes.
- **#89** — `tape encrypt` / `tape decrypt`: age-based confidentiality,
  orthogonal to `tape sign`.

> #99 (`tape new`) closed via merged PR #146 — see Phase-1 feature drops
> in v0.2.

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

- **#103** — `tape changelog`: model-narrated multi-cassette changelog
  (release notes / sprint retro / incident summary).

> #105 (`tape recap`) closed via merged PR #142 — see Phase-1 feature
> drops in v0.2.

### Tagging + policy + custom rules

- **#93** — `tape tag`: structured semantic tags via new `meta.tags[]` field +
  add/remove/list/auto subcommand.
- **#110** — `tape policy`: declarative cassette-compliance checker
  (signed/anon/tagged/recap/etc.) for team enforcement.
- **#104** — `tape redact-test`: canary-fuzz CLI for custom redaction/anon
  rules (FP/FN report, JUnit for CI).

### Parity gaps

> #74 (`tape annotate` CLI) closed via merged PR #141 — see Phase-1
> feature drops in v0.2. Bucket retained as a placeholder for future
> CLI/MCP parity work.

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
