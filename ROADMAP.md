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

## Current Milestone — v0.2.2 (binary distribution + Phase-2 feature rollup)

**Status:** Blocked on #144 only. v0.2.0 shipped 2026-05-15 with the
judge-model narration theme + 9 new CLI subcommands; v0.2.1 shipped
same day as a hotfix for #174's workspace path-dep regression. v0.2.2
ships the binary distribution pipeline plus the Phase-2 follow-ons
that have been landing continuously since v0.2.1.

**Cut criteria for v0.2.2** (in priority order):

1. **#144 (binary distribution) resolved.** Build the macOS-aarch64
   tarball + `SHA256SUMS` for v0.2.0, v0.2.1, and v0.2.2; update the
   README's `curl` URLs; bump the plugin marketplace entry. **As of
   2026-05-16 ~05:30Z, #144 has been `priority:current` for 24+ hours
   without a PR.** PM has elevated and commented; Principal triaged
   to `priority:current` but no engineer has claimed.
2. ✅ ~~#175 (CI workflow) landed~~ — done via PR #202 (commit
   `c5ead97`, 2026-05-16). Minimal cargo check/test/clippy/fmt
   workflow firing on PR open. The next #174-style regression now
   has a guardrail.
3. **All open `priority:current` issues closed** — reduces to "#144
   resolved" since #144 is the only `priority:current` open.

That's it. **v0.2.2 is one ticket away from releasable.**

### Headline themes from the original v0.2 definition — deferred to v0.3

The previous version of this section (commit `9158c9c`, ROADMAP v0.2.2
spec) listed "At least one more original-v0.2 headline theme landed"
as cut criterion 2. Dropping it. Reasoning:

- Principal has filed ~15 Phase-2 follow-on tickets since v0.2.0 cut
  without filing engineering tickets for any of the four original
  headline themes (#1 Claude Desktop concrete, #2 interactive eject,
  #3 embedding diff alignment, #5 liner-notes-at-eject). That's a
  clear staging signal: the headline themes are not v0.2.x scope in
  practice.
- v0.2 already delivered headline theme #4 (judge-model narration via
  PR #153) in v0.2.0. The "minor bump = at least one headline theme"
  bar has been met.
- The originally-planned themes #1/#2/#3/#5 move to **v0.3 scope** —
  see Next Milestone below.
- Pragmatic shipping over aspirational gating, same precedent as
  v0.2.0's #144 deferral (commit `dc87494`).

### Phase-2 features shipping in v0.2.2

The continuous Phase-2 work since v0.2.1 — these all ship with v0.2.2
when it cuts:

- `tape relinernote` (#71 → #159) + `--template` (#196 → #197)
- `tape annotate --import` (#173 → #176) + `--editor`/`--in-place`
  (#158 → #161)
- `tape new --list-templates` / `--describe-template` (#179 → #180),
  `--set` (#188 → #189), bundled templates (#162 → #165)
- `tape doctor` categories: `claude-code` (#163 → #164), `signing`
  (#166 → #167), `index.*` (#183 → #184), `pricing.table.fresh`
  (#177 → #178)
- `tape stats --format json` (#157 → #160), `--with-cost` (#168 →
  #169), `--pricing-file` (#181 → #182)
- `tape recap --auto` (#151 → #172), `--model` + `.taperc::recap.
  default_model` (#198 → #199), `tape export --format md` (#8 → #156)
- `.taperc` extensions: `pricing.pricing_file` (#186 → #187),
  `new.default_template` (#190 → #191), `annotate` (#192 → #193),
  `relinernote.default_model` (#194 → #195)
- `tape tag` Step-1 (#93 → #155)
- Minimal CI workflow (#175 → #202)

### Stretch items for v0.2.x

- **Causal-flow detection in diff** (the `causal` class in the schema
  today but never produced). Pairs with embedding-based diff alignment
  in v0.3.
- **`tape anon` Phase 1** (#204 → PR #205 in flight) — strip unix
  home paths. Ships in v0.2.2 if it merges pre-cut.

---

## Next Milestone — v0.3 (multi-runtime + finish v0.2 promises + ecosystem)

v0.3 absorbs the four original-v0.2 headline themes that didn't make
the v0.2.x cuts, alongside the multi-runtime + ecosystem work:

**Deferred v0.2 headline themes** (still no engineering tickets — Principal
to scope):

- **Claude Desktop adapter — concrete impl.** Infrastructure (`RuntimeAdapter`
  trait + `ClaudeCodeAdapter`) landed in v0.2.0 via PR #143. The
  `ClaudeDesktopAdapter` concrete impl is the gating piece for the
  second runtime.
- **Interactive eject prompt** (`[y/n/d/e]` confirmation flow). Lets the
  user inspect proposed redactions before the tape lands on disk.
- **Embedding-based diff alignment.** Needleman-Wunsch on step-intent
  embeddings (see `tape-diff` skill). Shares the model-client
  infrastructure from the merged `tape-judge` crate.
- **Liner-notes-at-eject.** Configurable model + token budget. The
  `tape relinernote` command (shipped in v0.2.x) regenerates after the
  fact; this would land it at eject time. Shares the `tape-judge`
  infrastructure.

**Original v0.3 scope:**

- **Codex / OpenAI Agents adapter** and **OpenClaw adapter** — third and
  fourth runtimes (after Claude Desktop concrete adapter).
- **Cursor adapter** (build on `RuntimeAdapter` trait).
- **`tape splice`** — surgical edit of a single track's payload, preserving
  structure.
- **Hosted cassette registry** — the WebAssembly `tape verify` build (#107)
  is the gating prerequisite for in-browser verification.
- **Cross-platform binary distribution.** v0.1.x / v0.2.x ship
  macOS-Apple-Silicon only; v0.3 broadens to at least Linux x86_64 +
  macOS Intel.

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

### v0.2.1 — 2026-05-15 — Hotfix: workspace path-dep regression

Single-issue hotfix for #174. v0.2.0 bumped `workspace.package.version`
0.1.2 → 0.2.0 but left internal path-dep version constraints at
`version = "0.1.0"`; `^0.1.0` doesn't satisfy `0.2.0`, so fresh
clones failed `cargo check`. v0.2.1 lifts the constraints to `0.2.0`
(the v0.2.x lower bound), bumps workspace to 0.2.1, and updates all
10 lockfile entries. No source changes. Full notes in RELEASE_NOTES.md.

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
