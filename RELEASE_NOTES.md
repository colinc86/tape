# `tape` Release Notes

A cassette tape for agent runs. Record once, replay anywhere, share as a file.

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
