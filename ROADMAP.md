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

## Current Milestone — v0.2.1 (hotfix: fix workspace path-dep regression)

**Status:** Required. v0.2.0 (commit `33aa143`, 2026-05-15) shipped with
a release-mechanics regression in `Cargo.toml`: the workspace
`[workspace.package].version` bumped to `0.2.0`, but the internal
path-dep version constraints at lines 68-75 still read `version =
"0.1.0"`. `^0.1.0 = >=0.1.0, <0.2.0` does not satisfy `0.2.0`, so
**fresh clones of main fail `cargo check`** — issue #174 documents the
break.

v0.2.1 is **a hotfix release**. Single-issue scope:

**Cut criteria for v0.2.1:**

1. **#174 resolved** — bump path-dep constraints in `Cargo.toml:68-75`
   from `version = "0.1.0"` to `version = "0.2.0"` (the lower bound of
   the v0.2.x line, so subsequent patch bumps don't re-trigger this).
   Bump workspace `[workspace.package].version` to `0.2.1`. Bump the
   9 workspace crates in `Cargo.lock` to match.
2. `cargo check --workspace` clean on a fresh clone.

That's the whole release. No new features, no Phase-2 PRs gate it, no
other `priority:current` issues block it. Ship immediately to unbreak
fresh checkouts.

The originally-planned v0.2.1 scope (binary distribution + finish v0.2
headline themes) moves to **v0.2.2** below.

### Why a hotfix and not a normal patch

- #174 is a regression from the v0.2.0 cut commit itself, not a bug in
  the underlying functionality. It needs to land outside the normal
  feature-cadence window.
- #174's author (Principal) explicitly assigned it to "the PM/release
  lane, not Engineer A/B (whose charter forbids touching workspace
  versions)." PM cuts release-mechanics fixes.
- Cutting v0.2.2 with `#144 + headline theme + #174` as a combined
  release would mean fresh clones stay broken until v0.2.2 ships —
  unbounded delay. Hotfix unblocks engineers immediately.

### Next Milestone after v0.2.1 — v0.2.2 (binary distribution + remaining v0.2 themes)

This was the v0.2.1 plan before #174 surfaced. Now it's v0.2.2.

**Cut criteria for v0.2.2** (in priority order):

1. **#144 (binary distribution) resolved.** Build the macOS-aarch64
   tarball + `SHA256SUMS` for v0.2.0, v0.2.1, and v0.2.2; update the
   README's `curl` URLs; bump the plugin marketplace entry.
2. **At least one more original-v0.2 headline theme landed.** Pick from
   themes #1 (Claude Desktop concrete adapter), #2 (interactive eject),
   #3 (embedding diff alignment), or #5 (liner-notes-at-eject). Principal
   scopes which.
3. **#175 (CI workflow) landed.** Without CI, the next v0.2.x cut
   risks the same #174-style regression silently surviving. The release
   pipeline needs a guardrail.
4. **All open `priority:current` issues closed.**

### Remaining headline themes from the original v0.2 definition

These slipped past v0.2.0 and are the v0.2.x target:

1. **Claude Desktop adapter (concrete impl).** Infrastructure landed in
   v0.2.0 (PR #143); the `ClaudeDesktopAdapter` concrete impl is open.
   No issue filed yet.
2. **Interactive eject prompt.** The `[y/n/d/e]` confirmation flow.
   Not started; no issue filed.
3. **Embedding-based diff alignment.** Needleman-Wunsch on step-intent
   embeddings. Not started; no issue filed. Will share `tape-judge`
   model-client infrastructure.
5. **Liner-notes-at-eject.** Configurable model + token budget.
   Adjacent work (`tape relinernote` PR #159) is open at cut; the
   eject-time variant is the headline-theme target.

### Stretch items for v0.2.x

- **Causal-flow detection in diff** (the `causal` class in the schema
  today but never produced). Pairs with the embedding-based diff
  alignment work in headline theme #3.
- **Phase-2 Phase-1 follow-ons** from in-flight PRs at the v0.2.0
  cut: `tape recap --auto` (#154), `tape export --format md` (#156),
  `tape relinernote` (#159), `tape new` templates (#165), doctor
  `signing` category (#167), `tape stats --with-cost` (#168).
  These ship as they merge — no `priority:current` promotion needed.

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
- **#78** — `tape playlist`: named, ordered collections of cassettes that
  other commands can operate on as a unit.

> #31 (`tape stats`) closed via merged PR #147 (Step-1 single-cassette).
> A library-wide Step-2 would be a fresh ticket.

### Summarization + narration

- **#103** — `tape changelog`: model-narrated multi-cassette changelog
  (release notes / sprint retro / incident summary).

> #105 (`tape recap`) closed via merged PR #142 — see Phase-1 feature
> drops in v0.2.

### Tagging + policy + custom rules

- **#110** — `tape policy`: declarative cassette-compliance checker
  (signed/anon/tagged/recap/etc.) for team enforcement.
- **#104** — `tape redact-test`: canary-fuzz CLI for custom redaction/anon
  rules (FP/FN report, JUnit for CI).

> #93 (`tape tag`) closed via merged PR #155 (Step-1) — see Phase-1
> feature drops in v0.2.

### Parity gaps

> #74 (`tape annotate` CLI) closed via merged PR #141 — see Phase-1
> feature drops in v0.2. Bucket retained as a placeholder for future
> CLI/MCP parity work.

---

## Recently Shipped

### v0.2.0 — 2026-05-15 — Judge-model narration + nine new commands

The first minor bump and first non-patch release. Headline: `tape diff
--judge` now narrates substantive diffs as model-written paragraphs,
backed by a new `tape-judge` crate that handles model-client config,
retry policy, and a defense-in-depth scanner that re-redacts every
model output. Around it: nine new CLI subcommands (`tape doctor`,
`annotate`, `recap`, `new`, `stats`, `tag`, plus `tape stats --format
json`, `tape doctor claude-code`, and `tape diff --judge` itself).
Plus the `RuntimeAdapter` trait + `ClaudeCodeAdapter` as the v0.3
multi-runtime precursor.

No format changes; every v0.1.x tape continues to verify identically.

Known limitations in v0.2.0 (carried into v0.2.1):
- **No binary distribution** (#144). v0.2.0 release page ships
  source-only; README install path still references v0.1.0 tarball.
- **Four original v0.2 headline themes** (Claude Desktop adapter
  concrete impl, interactive eject prompt, embedding-based diff
  alignment, liner-notes-at-eject) deferred to v0.2.1+.

Full notes in RELEASE_NOTES.md.

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
