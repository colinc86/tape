# DECISIONS.md

Non-obvious build decisions for `tape`. The brief mandates this file: when the brief leaves a question open and we make a defensible choice, that choice plus its rationale lives here.

Entries are append-only. Status `revisit-when-<condition>` flags decisions that should be re-examined later; status `decided` is final unless explicitly reopened.

---

## D1. Anthropic-only proxy in v0; OpenAI proxy as the abstraction proof

**Date:** 2026-05-06
**Step:** 5 / 8
**Status:** decided

### Context
The brief says v0 records Claude Code, which "may use either" Anthropic or OpenAI APIs depending on config. We could either build the Anthropic proxy first and copy it for OpenAI, or build a vendor-agnostic proxy from the start.

### Choice
Build Anthropic first as a complete monolith (`crates/tape-record/src/proxy/anthropic.rs`), then refactor the shared logic into `proxy::common` when adding OpenAI in step 8. Both proxies become thin shims over `common`.

### Why
- Step 5 needed to ship a working recorder fast; over-abstracting upfront risked premature generalization.
- Refactoring after the second vendor existed gave a real second data point. The result (`proxy/common.rs` parameterized by `vendor` + `recorded_path`) is justified by use, not speculation.
- The brief's step-8 description ("proves proxy abstraction") matches: refactor once you have two real cases.

### Revisit if
A third vendor surfaces with a meaningfully different API shape (Google Gemini, etc.).

---

## D2. Transcript ingestion as the v0.1 in-session record path

**Date:** 2026-05-06
**Step:** v0.1 planning
**Status:** decided

### Context
v0 ships with one recording path: `tape record -- claude <args>`, which proxies the API as a parent process. To record an investigation already underway in an active Claude Code session, the user has to abandon the session and restart under `tape record`. UX-hostile.

The user asked: can the MCP, in the current session, just record everything from the start of the session (or last `/clear`) and dump it as a tape on demand?

### Options considered
- **A. Always-on hook capture from session start.** Plugin installs hooks that POST to a session-scoped log. Pro: live capture. Con: every session pays the I/O cost whether the user wants a tape or not; conflicts with `/clear` semantics.
- **B. Inotify-watch the transcript.** Daemon-style file watcher. Pro: live. Con: long-lived FD on a file Claude Code is also writing; threads, races, and a watcher process to manage.
- **C. Pull-on-eject from the existing transcript file.** No background work; at snapshot time, read `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl` from offset 0 to current EOF and convert. Pro: zero overhead until invoked, no new process model, works on an unmodified Claude Code. Con: post-hoc only; loses chunk-streaming fidelity.

### Choice
**Option C — pull-on-eject from the transcript file.** Implemented as a new MCP tool `tape.snapshot(out, task?)` plus a slash command `/tape:tape-snapshot`.

### Why
- Claude Code already writes the full transcript to disk; the data we need exists at snapshot time. Building any live-capture mechanism duplicates state the OS already has.
- Pull model means: no impact on sessions that never want a tape, no /clear-detection heuristics (you snapshot from session start to now, full stop), no shared resources to coordinate.
- The CLI proxy path stays unchanged for the cases where it's still right (non-interactive `claude -p`, scripted recordings, latency-fidelity captures). Both paths produce valid `tape/v0` files.

### Revisit if
Claude Code's transcript format becomes proprietary/encrypted, or the file is moved off disk.

---

## D3. Built-in non-MCP Claude Code tools map to `Kind::McpCall` with `server="builtin"`

**Date:** 2026-05-06
**Step:** v0.1 implementation
**Status:** decided

### Context
SPEC.md fixes the v0 `Kind` enum at 8 values: `task | model_call | mcp_call | shell | file_read | file_write | annotation | eject`. The transcript ingestion path needs to convert Claude Code's tool inventory (Bash, Read, Write, Edit, MultiEdit, NotebookEdit, Grep, Glob, WebFetch, WebSearch, Task, Skill, TodoWrite, plus arbitrary `mcp__<server>__<tool>` names) into this closed set.

The first three categories map cleanly: Bash → `Shell`, Read → `FileRead`, Write/Edit/MultiEdit/NotebookEdit → `FileWrite`. Tools named `mcp__<server>__<tool>` map to `McpCall` with `payload.server = <server>`.

The remaining built-in tools (Grep, Glob, WebFetch, WebSearch, Task, Skill, TodoWrite, etc.) have no clean mapping.

### Options considered
- **A. Add new `Kind` values** for every Claude Code built-in we encounter (`grep`, `web_fetch`, etc.). Rejected: SPEC.md is fixed for v0; changing it forks the wire format.
- **B. Add a single `Kind::ToolCall`** as a catch-all. Rejected: same problem — extending the enum is a `tape/v1` change, not a v0.1 change.
- **C. Drop these tools entirely** and don't record them. Rejected: WebFetch and Grep are load-bearing for many investigation flows.
- **D. Stretch `Kind::McpCall`** to cover them, with `payload.server = "builtin"` and `payload.tool = <tool name>`.

### Choice
**Option D.** Mapping table lives at the top of `crates/tape-record/src/transcript/convert.rs` as a single source of truth. Header comment documents the stretch.

### Why
- Preserves the closed v0 enum exactly. `tape verify` is unchanged. `tape.tracks --kind mcp_call` returns these tools alongside genuine MCP calls.
- The semantic stretch is small: from Claude Code's perspective, all of these are "tools the agent invoked." The `server: "builtin"` discriminator keeps consumers able to filter.
- v0.2 / `tape/v1` may introduce a `Kind::ToolCall` and migrate; that's a major-version concern.

### Revisit if
Cutting a `tape/v1` major version, or if `payload.server == "builtin"` collides with a real MCP server somewhere.

---

## D4. Recorder agent suffix distinguishes ingestion paths

**Date:** 2026-05-06
**Step:** v0.1 implementation
**Status:** decided

### Context
We now have two recording paths producing `tape/v0` files: the CLI proxy and the in-session transcript ingestion. Downstream tooling (e.g. anyone building on top of tape) may want to know which one produced a given tape — they have different fidelity guarantees.

### Choice
The CLI path writes `meta.recorder.agent = "tape-cli/<version>+proxy"`. The transcript path writes `meta.recorder.agent = "tape-mcp/<version>+transcript"`. The `+<source>` suffix is informational; the format spec doesn't constrain `recorder.agent` content.

### Why
- Free additional info, no spec change.
- Lets a future `tape diff` (or any consumer) recognize that two tapes of the same task may differ in `model_call` payload fidelity due to the ingestion path, not because the runs diverged.

### Revisit if
We add a third recording path or want to standardize the suffix vocabulary.
