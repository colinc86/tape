# `tape` Release Notes

A cassette tape for agent runs. Record once, replay anywhere, share as a file.

---

## v0.2.0 — 2026-05-15 — Judge-model narration + nine new commands

The first minor bump since v0.1 — and the first non-patch release. v0.1.x
was the format-and-spec phase: ship the file layout, harden the verify
gate, fix every audit finding. v0.2 is the **agent-experience** phase:
the headline feature is judge-model narration on `tape diff`, and around
it a layer of new CLI subcommands that make a cassette do useful work
without ever opening the deck.

No format changes. Every `.tape` produced by v0.1.0/v0.1.1/v0.1.2 remains
a valid `tape/v0` cassette readable by v0.2.0, byte-for-byte. The bump
is minor (not patch) because eleven new user-facing surfaces ship at
once — semver requires a minor for that.

### Headline theme — judge-model narration

`tape diff --judge` now narrates the substantive differences between two
cassettes as short, model-written paragraphs instead of just listing
classified diff entries. The narration is grounded in the actual aligned
steps, scanned through the same defense-in-depth redaction pipeline the
eject path uses, and gated behind an explicit `--judge` flag (no surprise
network calls).

- **New crate `tape-judge`** (#148 → PR closing #145). The shared
  judge-model client: provider config, request/response shape,
  retry/timeout policy, and the defense-in-depth scanner that
  re-redacts every model output before it reaches the user. Every
  v0.2+ feature that calls a model goes through this crate.
- **`tape diff --judge` wiring** (#149 → #153). The user-visible
  surface. The flag was previously accepted by clap and silently
  ignored (#62) then rejected with a clear error (v0.1.2 via #64);
  v0.2.0 finally implements it.
- **`tape recap --auto`** is the second consumer of the judge
  infrastructure (Phase-2 work in flight as of cut; see Known
  Limitations).
- **`tape relinernote`** likewise — Phase-1 PR open at cut; will
  ship in v0.2.1.

### New CLI subcommands

Nine first-class user-visible commands ship in v0.2.0:

- **`tape doctor`** (#81 → #140) — install-surface diagnostic with
  pass/warn/fail report. Phase-1 covers the binary install. Phase-2
  added a `claude-code` category (#163 → #164) that checks the
  Claude Code plugin install and `enabled` state. Reduces onboarding
  pain as the runtime list grows.
- **`tape annotate`** (#74 → #141) — CLI counterpart to the MCP
  `tape.annotate` deck tool. Closes a long-standing parity gap.
- **`tape recap`** (#105 → #142) — 1–2 sentence regenerable summary
  for paste-into-Slack/Linear/Jira/PR contexts. The Phase-1 ships
  with a manual `--summary` flag; the `--auto` judge-driven variant
  is Phase-2 (open PR #154 at cut).
- **`tape new`** (#99 → #146) — cassette generator with bundled
  templates. The `cargo new` for `.tape` files. Phase-1 ships an
  empty template; Phase-2 (#162) bundles named templates
  (`test-fixture`, `bug-investigation`).
- **`tape stats <file>`** (#31 → #147) — single-cassette analytics
  (track counts by kind, byte sizes, time deltas). Phase-2 adds
  `--format json` with a pinned `schema_version: 1.0` (#157 → #160)
  so CI scripts can consume the output without breaking on layout
  changes.
- **`tape tag`** (#93 → #155) — structured semantic tags via the new
  `meta.tags[]` field. Step-1 ships the `add` / `list` / `remove`
  subcommands; auto-tagging is a future Step-2.

### Infrastructure

Two pieces of internal scaffolding land in v0.2.0. Neither has a
user-facing surface yet, but both are the gating prerequisites for
v0.3 themes:

- **`RuntimeAdapter` trait + `ClaudeCodeAdapter`** (#106 → #143).
  The recording subsystem is now generic over runtime. v0.1.x had
  Claude Code hardcoded; v0.2.0 extracts the runtime-specific bits
  (transcript discovery, event conversion, hook dispatch) behind a
  trait. `ClaudeCodeAdapter` is the first concrete impl. The
  **Claude Desktop adapter** (originally framed as v0.2 headline
  theme #1) lands when a second concrete impl is written; that's
  v0.2.1 or v0.3 work depending on Principal's call.
- **`tape-judge` crate** — see Headline theme.

### Format compatibility

Zero changes to `tape/v0`. SPEC.md is unchanged from v0.1.2. Every
fixture in `tests/fixtures/` continues to verify identically. Plugin
marketplace consumers don't need to update existing tapes.

The `meta.tags[]` field (introduced by #155) is optional and defaulted
to an empty array on read, so older v0.1.x cassettes parse cleanly.

### Workflow changes (no user impact)

Internal to the maintainer team: `priority:current` / `:next` /
`:later` and `needs-review` / `in-review` / `changes-requested` /
`approved` / `blocked` are now the canonical issue and PR workflow
labels (#118 / #126). v0.1.2's release notes mentioned this; v0.2.0
is the first release tagged under the full discipline.

### Known limitations carried into v0.2.0

Each of these has a tracking issue or open PR and is in scope for v0.2.1.

- **Binary distribution gap (#144)** — v0.2.0's GitHub release page
  ships **no tarball** and **no SHA256SUMS**. The README's `curl`
  install path continues to point at the v0.1.0 tarball. The plugin
  marketplace bundles v0.1-era binaries. v0.2.0 is therefore
  source-build-only; users who want the new commands need
  `cargo build --release` from the `v0.2.0` tag.

  This is on the record as a v0.2.0 limitation — the binary pipeline
  is the v0.2.1 priority:current target. ROADMAP commit `dc87494`
  documents PM's decision to ship v0.2.0 without binary assets after
  Principal explicitly skipped triage of #144 across five
  consecutive firings.

- **Claude Desktop adapter (v0.2 headline theme #1)** — the
  runtime-adapter framework landed (`RuntimeAdapter` trait +
  `ClaudeCodeAdapter`), but no second concrete adapter has been
  written yet. v0.2 was originally scoped as "Claude Code +
  Claude Desktop"; v0.2.0 ships with infrastructure in place and
  the second runtime deferred.

- **Interactive eject prompt (v0.2 headline theme #2)** — not
  started. No issue filed yet. v0.2.x follow-on.

- **Embedding-based diff alignment (v0.2 headline theme #3)** — not
  started. The Needleman-Wunsch alignment on step-intent embeddings
  is the v0.2.x follow-on. v0.2.0 still uses the v0.1 LCS aligner.

- **Liner-notes-at-eject (v0.2 headline theme #5)** — not started
  in production. PR #159 (`tape relinernote`) is a relate-but-not-
  identical command (regenerate liner notes for an already-ejected
  tape) and is open at cut.

- **Phase-2 PRs in flight at cut** — `tape recap --auto` (#154),
  `tape export --format md` (#156), `tape relinernote` (#159), and
  `tape new` bundled templates (#165) are all open `needs-review`
  or `changes-requested` as of the cut SHA. These ship in v0.2.1.

### Repository layout (unchanged)

Same as v0.1.2 plus one new workspace crate:

```
crates/
├── tape-cli/             CLI binary `tape`
├── tape-format/          format read/write/verify
├── tape-record/          recording subsystem + runtime adapter trait
├── tape-mcp-wrap/        JSON-RPC tee binary
├── tape-redact/          redaction engine
├── tape-play/            ls/play rendering
├── tape-diff/            three-pass diff
├── tape-mcp/             the deck — MCP server
└── tape-judge/           [NEW in v0.2.0] shared judge-model client
```

---

## v0.1.2 — 2026-05-14 — Spec-compliance rollup

A pure bug-fix release. v0.1.1 closed twenty findings from a three-agent
audit; v0.1.2 closes the next thirty-plus, dominated by SPEC enforcement
gaps that `tape verify` was silently letting through. Every `priority:current`
issue scoped to this milestone (#26, #66, #68, #109) is closed. **No format
or behavior changes**: every tape produced by v0.1, v0.1.1, or v0.1.2
remains a valid `tape/v0` cassette readable by any release in the line.

The patch is heavy on diagnostics. Six previously-undetected MUSTs in
SPEC are now enforced by verify, three diagnostic-code emissions that
had been dead code are now wired correctly, and two had-the-symptom-but-
not-the-cause bugs in the deck got their actual root cause fixed.

### Spec enforcement (`tape verify`)

The big block. `tape verify` is the load-bearing contract for the format —
if it accepts a malformed tape, every consumer downstream inherits the
problem. v0.1.2 closes six holes:

- **§3.1 `created_at ≤ ejected_at`** is now checked, emitting
  `BAD_TIMESTAMP` on violation (#68 → #123). Tapes with the meta fields
  inverted previously passed clean.
- **§5.4 "exactly one `task` / exactly one `eject`"** is enforced
  (#86 → #87). Tapes with two task events or two eject events were
  silently accepted.
- **§5.5.1 "task prompt MUST be non-empty"** is enforced (#96 → #98).
- **§9.1 `RedactConfig` typo rejection** — `serde(deny_unknown_fields)`
  on `RedactConfig` so a misspelled key under `redact:` in `.taperc`
  fails at config-load time instead of becoming a silent no-op
  (#36 → #40).
- **`UNKNOWN_KIND`** diagnostic is now emitted for non-reserved unknown
  event kinds (#91 → #92). Previously these surfaced as the generic
  `INVALID_TRACKS_JSON`. `RESERVED_KIND` for fork/splice events is
  separately wired in #65.
- **Defense-in-depth scan** now applies every default-enabled built-in
  redaction rule (#33 → #38). Previously only the `sk-ant-` prefix was
  caught, so tapes with leaked credentials in `meta.yaml` or
  `liner-notes.md` could ship undetected. `meta.label` is now redacted
  before this scan (#77 → #79) so a label containing an email or JWT
  no longer hard-fails eject.

### Recorder / hook correctness

The capture surface had several "the data looks right but the events
disagree" bugs that have all been buttoned up:

- **`tape-hook` content hashes**: `PreToolUse` populates `file_write.
  before_hash` (`#9 → #57`) so file_write events carry the pre-edit
  hash; content hashing now streams via `blake3::Hasher` (#43 → #52)
  instead of reading the entire file into memory, and the `blake3:0`
  sentinel that earlier versions emitted when content was missing has
  been removed (now the field is just absent, which is conformant).
- **NotebookEdit coverage**: the settings overlay's PreToolUse /
  PostToolUse matchers and the hook dispatch lists both include
  NotebookEdit now (#75 → #76, #83 → #84). Live recordings of notebook
  edits no longer get dropped on the floor.
- **`parent_step` validation**: writer and verifier both enforce that
  every event's `parent_step`, if present, points at a step that
  actually exists (#3 → #19). A stale parent_step is no longer a silent
  data-integrity problem.
- **HTTP failure status** on proxied `model_call` events: the Anthropic
  and OpenAI recorders now record the HTTP status code on failure
  (#6 → #24) instead of just the body. Critical for debugging
  rate-limited or auth-rejected calls in a replay.

### Deck / MCP

The MCP server (`tape mcp`) is the consumer interface — most of the
deck bugs were "the tool succeeded but the result was missing
something." All fixed:

- **`tape.play` resolves `{ref: sha:...}` stubs** (#44 → #48) against
  the loaded tape's `artifacts/` tree. Previously the agent got the
  stub back and had to resolve it manually.
- **`tool_eject` inherits artifacts and label** from the loaded tape
  (#41 → #46, #80 → #82). Forking + re-ejecting no longer produces a
  tape that fails `MISSING_ARTIFACT` on verify, and the new tape no
  longer loses `meta.label`.
- **`tape.fork` at last step + `tape.eject`** no longer produces a
  tape with two eject events (#26 → #32). Pipeline now drops a trailing
  eject before appending a fresh one.
- **Per-event timestamps** are preserved through `tool_eject` (#20 →
  #25) and `tape.snapshot` (#16). Replaying a tape now produces the
  same timeline as the original.
- **JSON-RPC notification suppression**: the MCP server no longer
  responds to JSON-RPC notifications, per §4.1 (#56 → #59). Some MCP
  clients hung waiting for an impossible response.
- **`tape.eject` accepts an optional `outcome` arg** (#35) — defaults
  to `unknown` if omitted (was previously hardcoded to `success`,
  #30).
- **`tape-mcp-wrap` PENDING_TTL** raised from 5 minutes to 1 hour
  (#53 → #55). Long-running tool calls no longer get their responses
  silently dropped.
- **`tape.seek` no longer panics on non-ASCII payloads** (#7 → #12) —
  the substring matcher's character-boundary handling is fixed.
- **`Session::append_at`** preserves `parent_step`, `refs`, and
  `annotations` on replay (#49 → #54). Snapshot replay no longer
  silently strips event metadata.
- **`meta.tool_budget`** is now populated at eject time (#109 → #119).
  `tape diff`'s Latency summary was silently dead because every tape
  was missing this field.

### Redaction engine

- **`.taperc` is loaded** on every recording path (#17 → #29). Earlier
  versions implemented the config but never read it; custom rules,
  `enable_optional`, and `disable_default` were all silent no-ops.
- **Engine rules are used** for the eject defense-in-depth scan
  (#23 → #27), so opt-in rules participate in the post-redaction
  audit.
- **`disable_default`** validates rule names (#45 → #50). Asymmetric
  with `enable_optional` previously — typos in `disable_default`
  silently succeeded, typos in `enable_optional` failed loud.
- **Oversize arrays and objects** spill to `artifacts/` (#4), not
  just strings. SPEC §5.6 measures encoded size; both writer and
  reader now agree on the threshold.

### Diff CLI

- **`tape diff --judge`** rejects with a clear error until narration
  lands (#62 → #64). The flag was previously accepted by clap and
  silently ignored.
- **`tape diff --last-answer`** restricts to agent annotations
  (#15 → #22) instead of picking up parser-warning annotations as
  "the canonical answer."

### Surfacing

- **`tape record --label`** reaches `meta.yaml` (#72 → #73). The
  label was previously used only for the default filename and was
  lost in the produced tape.

### SPEC documentation

- **§10.6 diagnostic-code list** now includes `LINER_SECTIONS_OUT_OF_ORDER`
  and `UNKNOWN_ENTRY`, both of which `tape verify` already emits
  (#66 → #125).

### Cleanup

- **`UNSAFE_PATH` diagnostic removed** as unreachable code
  (#132 → #137). The verify implementation never had a path that
  emitted it; the dead code is gone.

### Workflow changes (no user impact)

v0.1.2 also brought a project-internal workflow change worth noting
for downstream maintainers: `priority:current` / `priority:next` /
`priority:later` and `needs-review` / `in-review` / `changes-requested`
/ `approved` / `blocked` are now the canonical issue and PR workflow
labels (#118, #126). This affects how the project is run, not how
the format or CLI behaves.

### Known v0.1.2 limitations (still deferred to v0.2)

Unchanged from v0.1.1:

- No `/clear` boundary detection in `tape.snapshot`.
- No streaming-cursor `tape.record_session(start) → tape.eject_session()`
  two-step shape.
- Bundled binaries are macOS Apple Silicon only.
- `tape.diff` from the deck only works on tapes loaded from disk.
- Interactive eject prompt (`[y/n/d/e]`) still unimplemented.
- Diff alignment still LCS-based; Needleman-Wunsch + step-intent
  embeddings is v0.2 work.
- Judge-model narration not yet implemented (the `--judge` flag is
  explicitly rejected with the v0.1.2 message).

---

## v0.1.1 — 2026-05-07 — Audit cleanup

A bug-fix-only release. Closes 20 findings from a three-agent audit covering
spec compliance, security posture, concurrency, and edge-case correctness.
**No format or behavior changes** — every existing tape and every existing
plugin install continue to work unchanged.

Test count grows from 88 to 106 (+18 new tests).

### Security & spec compliance

- **`aws_secret_key` redaction rule** added (SPEC §7). Capture-group-targeted:
  the `aws_secret_access_key = ...` label survives, only the 40-char secret
  is replaced with `<API_KEY:aws_secret>`.
- **Custom `.taperc` replacement validation** (SPEC §6.2). Replacements must
  be typed placeholders (`<TYPE>` or `<TYPE:subtype>`); literal secrets and
  hashes are rejected at config-load time.
- **100× decompression-bomb limit** (SPEC §12.3) in the tape reader, with a
  64 KiB floor so trivially-small tapes don't false-positive.
- **`ALREADY_RECORDING` enforcement** in the deck's `tape.record` tool, with
  the recording flag cleared on `tape.eject` so subsequent recordings work.
- **Empty/whitespace-only line rejection** in `tracks.jsonl` per SPEC §5.1.
- **JSONPath validation** on `redactions.json::field_path`. Cheap subset
  (`$`, `$.name`, `$[n]`, `$["key"]`).
- **Email regex tightened** to disallow consecutive dots in domain.

### Robustness

- **`encode_cwd` hardened** — every non-alphanumeric/underscore char now
  becomes `-`, matching Claude Code's actual encoding for paths with `:`,
  `@`, `(`, `)`, `+`, `.`, `'`. Previously only `/` and ` ` were escaped.
- **Recorder Unix socket idle timeout** (30s) prevents a hung client from
  tying up a tokio task forever.
- **`tape-mcp-wrap` pending-map TTL** (5 min) bounds memory in long sessions
  where some `tools/call` requests never receive responses.
- **`tape-mcp-wrap` shutdown ordering** — drop the `Arc<Mutex<ChildStdin>>`
  outright instead of locking-and-shutdown, eliminating the race with the
  server-to-client tee task.
- **Per-field meta redaction** — instead of redacting the whole serialized
  YAML as text and re-parsing (which could fail if a redaction landed in a
  key position), redact `meta.task`, `meta.recorder.user`, and
  `meta.recorder.agent` individually. No re-parse, no failure mode.
- **JSON-serialized spillover threshold** — SPEC §5.6 measures the encoded
  value (which adds quotes plus escapes). Both writer (`eject`) and reader
  (`verify`) updated.
- **Empty `--label` fallback** — sanitization producing only dashes or empty
  string falls back to `session.tape` instead of an ambiguous filename.

### Polish

- **`hook.rs` content_hash sentinel removed.** When a hook's `tool_response`
  doesn't include `file_content`, the field is omitted entirely instead of
  emitting an invalid `blake3:0`.
- **`Session::start_at`** variant accepts an explicit timestamp so
  `tape.snapshot` aligns `meta.created_at` with the transcript's first event
  rather than wall-clock-now.
- **`task_text` truncation** — `meta.task` is documented as one line, and a
  ≤200-char first-line truncation enforces it. A 10 KB first user prompt no
  longer produces a 10 KB `meta.task`.
- **`pct_delta` returns `Option<i64>`** instead of `100` for the undefined
  case (a=0, b≠0). Renders as "Δ n/a".
- **`tape-snapshot.md` instruction** — clarified that `task` is optional.
- **`tape-usage` SKILL** — fixed stale "11 tools" lead.

### Bonus catch

The redact engine's JSONPath generator was producing `$.parent.["weird key"]`
(extra dot before bracket) for keys with non-identifier characters. Fixed
alongside the JSONPath validation work.

---

## v0.1 — 2026-05-06 — In-session recording

The big addition in v0.1 is **`tape.snapshot`**: record a Claude Code session into a `.tape` file from inside the session, in one MCP call. No separate shell, no `tape record -- claude` wrapping, no API key needed.

### What's new

- **`tape.snapshot(out, [task], [transcript_path])`** — twelfth deck tool. Reads Claude Code's session transcript (`~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`), converts entries to `tape/v0` events, runs the existing eject pipeline. Returns the path, track count, redaction count, and parse warnings.
- **`/tape:tape-snapshot <name>`** — slash command that calls the tool with the right args.
- **Plugin version 0.2.0** — marketplace entry bumped; the in-session flow ships there.
- **`crates/tape-record/src/transcript/`** — new module: parser, discovery (cwd-encoding), convert (RawEntry → Track), tool-name → Kind mapping table.
- **8 fixture transcripts** — checked-in JSONL slices covering minimal, with-bash, sibling-tool-result, orphan-tool-use, mcp-call, mixed-kinds, unknown-event-type, redaction-bait scenarios.

### How recording paths compare

| Path | Speed | Fidelity | Use when |
|---|---|---|---|
| `tape.snapshot` (v0.1) | one MCP call from active session | medium — derives from CC's transcript | you're already mid-session and want a tape now |
| `tape record -- claude` (v0) | fork a new shell, wrap claude | high — raw HTTP bodies, real chunk timing | you're starting fresh, or scripting non-interactive runs |
| `tape.record` + annotate + eject | in-memory, agent-built | low — only what the agent annotates | scripted MCP-side use cases |

The format is identical across paths (`tape verify` accepts all three). `meta.recorder.agent` distinguishes them: `tape-mcp/0.1+transcript` vs `tape-cli/0.1+proxy` vs `tape-mcp/0.1`.

### Design notes

- Built-in non-MCP Claude Code tools (Grep, Glob, WebFetch, WebSearch, Task, Skill, TodoWrite, etc.) map to `Kind::McpCall` with `payload.server = "builtin"`. SPEC.md is fixed for v0; extending the closed `Kind` enum is a `tape/v1` change.
- Snapshot captures from session start to now. `/clear` leaves no marker in the transcript; detecting it would be heuristic. Honest default: full session.
- Tool-result lookup precedence: inline `tool_result` block in subsequent user message → sibling file at `<session-id>/tool-results/<tool_use_id>.txt` → orphan (call recorded with `result: null` and a warning annotation).
- `+transcript` recorder agent suffix lets downstream tooling recognize the ingestion path.

### Tests

- 17 new transcript-module unit tests (parser, discovery, convert).
- 3 end-to-end snapshot tests via JSON-RPC against fixture transcripts.
- Existing 65 tests still green.

**Total workspace test count: 88 passing.**

### Known v0.1 limitations (deferred to v0.2)

- No `/clear` boundary detection.
- No streaming-cursor `tape.record_session(start) → tape.eject_session()` two-step shape.
- Bundled binaries are macOS Apple Silicon only; cross-platform binary distribution is a separate work item.
- `tape.diff` from the deck only works on tapes loaded from disk (not in-memory recordings).

---

## v0 — Initial release

The format spec, CLI, deck (MCP server), and recording subsystem all shipped together. Single target runtime: **Claude Code**.

## What shipped

### Format

- `tape/v0` specified in `SPEC.md`. ZIP layout, JSONL tracks, content-addressed `artifacts/`, JSON redaction audit. 12 sections + a 17-rule verify checklist + 23 stable diagnostic codes.
- `crates/tape-format` implements read, write, and verify against the spec.

### CLI surface

- `tape verify <file>` — schema validator. Exits 0 on valid; non-zero with structured `ERROR <CODE>: <message>` lines on invalid. `--json` for machine-readable output.
- `tape ls <file>` — one-line-per-track listing.
- `tape play <file> [--step N | --range A..B | --kind K]` — full payloads or summary view (default).
- `tape record [--task ...] [--upstream-anthropic ...] [--upstream-openai ...] -- <command>` — records a child process. Spawns Anthropic + OpenAI proxies, recorder Unix socket, and writes a Claude Code settings + mcp.json overlay into a temp dir; the child gets `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `TAPE_RECORDER_SOCKET`, `TAPE_OVERLAY_SETTINGS`, `TAPE_OVERLAY_MCP_CONFIG` in env.
- `tape diff <a> <b> [--all] [--format text|json]` — three-pass align/classify (LCS-based; embedding-based NW alignment is a v0.1 upgrade) with text and JSON output.
- `tape mcp` — JSON-RPC 2.0 MCP server over stdio. Hand-rolled, all 11 tools.

### Sidecar binaries

- `tape-mcp-wrap` — JSON-RPC tee for MCP servers. Spawned by Claude Code (via the temp `mcp.json`) instead of the real server; tees `tools/call` traffic and posts `mcp_call` events to the recorder socket. Non-recording when the recorder is unreachable — best-effort by design.
- `tape-hook` — invoked by Claude Code `PostToolUse` hooks for `Bash` / `Read` / `Write` / `Edit` / `MultiEdit`. Reads the hook event JSON on stdin, posts a `shell` / `file_read` / `file_write` event to the recorder socket. Always exits 0 — never blocks the user's tool flow.

### The deck (`tape mcp`)

All 11 tools per `tape-mcp-deck`:
- Read-only: `tape.load`, `tape.summary`, `tape.tracks`, `tape.play`, `tape.seek`, `tape.tools`, `tape.diff`
- Mutating: `tape.fork`, `tape.record`, `tape.annotate`, `tape.eject`

The handle-not-contents invariant holds: `tape.load` returns a handle plus a quick summary. Bulk content arrives only when the agent calls `tape.tracks` / `tape.play` / `tape.seek`.

### Redaction

`crates/tape-redact` — 11 built-in rules, custom `.taperc` (workspace + user search), defense-in-depth scan over `meta.yaml` and `liner-notes.md`. Eject-pipeline integration writes `redactions.json` and fills `meta.redaction_summary`.

Built-in rules: `email`, `anthropic_api_key`, `openai_api_key`, `aws_access_key`, `jwt`, `ssn`, `credit_card` (Luhn-validated), `bearer_token`, `ipv4_private` (opt-in), `generic_high_entropy` (opt-in). Each has ≥5 positive and ≥5 negative test cases.

### Test footprint

~68 tests across the workspace:
- **Format**: 3 unit (artifact addressing) + 2 integration (every fixture verifies as expected; 8 malformed fixtures each pair with a sidecar diagnostic-codes file)
- **Play**: 4 unit (label rendering, range parsing)
- **Record**: 7 unit (session, socket); 2 integration (Anthropic streaming-not-buffered + non-stream); 2 integration (OpenAI mirror); 4 integration (`tape-hook` for Bash/Read/Write/Edit); 2 integration (eject-time redaction)
- **MCP wrap**: 1 integration (`tools/call` round-trip against mock server)
- **Redact**: 26 unit (rules + config + custom + opt-in)
- **Deck**: 5 integration (initialize, tools/list, full read workflow, fork, record→annotate→eject)
- **Diff**: 5 unit (alignment + classification); 3 integration (text, JSON, self-diff-is-identical)
- **CLI**: 1 record smoke; 1 killer scenario; 3 diff integration

### The killer scenario test passes

`crates/tape-cli/tests/killer_scenario.rs` spawns the real `tape mcp` binary, drives Engineer B through `tape.load` → `tape.seek` → `tape.play`, and asserts the smoking-gun annotation (customer ID `CUST-447139`, function `process_refund`) is recovered. The single demo that v0 must support, supported.

## Known limitations / deferred to later

These are intentional v0 cuts; each has a stable place to land in the next release.

- **Liner notes generation is stub-only.** The brief allows stub liner notes when no model is available. Real model-call-at-eject is config-flagged but unwired in v0; `tape.annotate` is the documented escape hatch (an agent can annotate a stub-eject before re-eject).
- **No interactive eject confirmation prompt.** The two-pass redact pipeline applies in-place; the `--yes` / interactive `[y/n/d/e]` prompt described in the brief is structurally compatible but unimplemented. CLI today is non-interactive.
- **Diff alignment uses LCS, not Needleman-Wunsch.** Cheap, no embedding backend required. NW + step-intent embeddings is a v0.1 upgrade.
- **Diff narration is skipped.** No judge-model integration. The `narration` field is optional in the JSON shape; v0 leaves it absent.
- **Diff causal-flow detection is not implemented.** All differences classify as `identical` / `cosmetic` / `substantive` / `inserted` / `deleted`; the `causal` class exists in the schema but is not produced by v0.
- **Hook overlay cleanup on `SIGKILL`** is structural (per-run tempdir under `$TMPDIR/tape-*`) but the orphan-tempdir sweep at next launch is not implemented.
- **No interactive confirmation prompt at eject time.** Recording is fire-and-forget plus the redaction pipeline.
- **Streaming preserves chunk cadence**, validated end-to-end for both Anthropic and OpenAI shapes against mock upstreams. Real upstream stream-protocol quirks (gzip-Transfer-Encoding, chunked-with-trailers, etc.) are not yet exercised.

## v0.1 — next on the road

- **Claude Desktop adapter.** Same format, different runtime — no changes needed to the format crate or the deck.
- **Liner-notes-at-eject** with a configurable model + token budget.
- **Interactive eject prompt** (`[y/n/d/e]`).
- **Embedding-based NW alignment** for `tape diff`, with judge-model narration.
- **Causal-flow detection** in diff.

## v0.2 — further out

- **Codex / OpenAI Agents adapter.**
- **OpenClaw adapter.**
- **`tape splice`** — surgical edit of a single track's payload, preserving structure.
- **Hosted cassette registry.**

## Repository layout

```
tape/
├── Cargo.toml                workspace root
├── SPEC.md                   normative format spec
├── README.md                 publishable; install + worked example + reference
├── RELEASE_NOTES.md          this file
├── crates/
│   ├── tape-cli/             CLI binary `tape`
│   ├── tape-format/          format read/write/verify
│   ├── tape-record/          recording subsystem (proxies + socket + hooks + eject)
│   ├── tape-mcp-wrap/        JSON-RPC tee binary
│   ├── tape-redact/          redaction engine
│   ├── tape-play/            ls/play rendering
│   ├── tape-diff/            three-pass diff
│   └── tape-mcp/             the deck — MCP server
└── tests/
    └── fixtures/             3 valid + 8 malformed checked-in tapes
```

## License

Apache 2.0.
