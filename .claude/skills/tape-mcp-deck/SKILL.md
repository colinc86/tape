---
name: tape-mcp-deck
description: The deck (tape mcp) — 11 tools, read-only vs mutating semantics, session-handle model, the handle-not-contents invariant that makes large tapes usable as external memory. Load when working in src/mcp/, implementing any deck tool, or designing the session state.
---

# The deck — `tape mcp`

`tape mcp` runs an MCP server over stdio (default) or HTTP+SSE (`--http <port>`, optional in v0). Speaks the standard MCP protocol. Designed to be added to a Claude Code config with:

```sh
claude mcp add tape -- tape mcp
```

## Core invariant: handle, not contents

When an agent calls `tape.load`, it gets a **session handle string**, not the tape's contents. Every subsequent operation is parameterized by that handle. The agent must explicitly `tape.tracks` / `tape.play` / `tape.seek` to pull slices into its context window.

This is what lets a 50 MB tape coexist with a 200K context window. The deck is external memory; the handle is the address.

A handle is a short opaque string (e.g. `tape:01HXY…:abcd`) — UUIDv7-derived for the tape, plus a per-session salt to distinguish multiple loads of the same tape.

## Session model

- A "session" is the lifetime of one `tape mcp` process.
- Sessions are NOT shared across processes. A second `tape mcp` invocation starts a fresh session.
- Within a session: state lives in-memory. Loaded tapes, recording state, fork buffers — all of it dies on process exit unless explicitly `eject`ed to disk.
- This is a deliberate constraint: making session state durable across processes is a footgun (stale handles, conflicting writers, format drift). v0 keeps it simple.

## The 11 tools

### Read-only

#### `tape.load`
**Args:** `path: string`
**Returns:** `{handle: string, summary: <same shape as tape.summary>}`
Mounts a `.tape` file. Verifies it (running the same checks as `tape verify`); rejects with a structured error if invalid. Returns the handle plus a summary so the agent doesn't need a second round-trip for the common case.

#### `tape.summary`
**Args:** `handle: string`
**Returns:** `{meta: <meta.yaml>, liner_notes: string, track_count: int, kinds: {<kind>: count}}`
Equivalent to `meta.yaml` + `liner-notes.md`. Cheap; designed to be called freely.

#### `tape.tracks`
**Args:** `handle: string, filter?: {kind?: string|string[], range?: [int, int], regex?: string, regex_field?: string}`
**Returns:** `{tracks: [{step, kind, ts, label}]}` — note `label`, NOT `payload`.
Returns the lightweight track listing. `label` is a one-line summary the deck synthesizes (e.g. `mcp_call: db.query("SELECT * FROM payments WHERE...")`). Agent uses this to navigate; pulls full payloads via `tape.play`.

#### `tape.play`
**Args:** `handle: string, step: int | range: [int, int]`
**Returns:** `{tracks: [<full track object>]}`
Returns full payload(s). Resolves any `{ref: ...}` stubs by reading the artifact. Caps total response at 200 KB; if exceeded, truncates and tells the agent to narrow the range.

#### `tape.seek`
**Args:** `handle: string, query: string, k?: int (default 5)`
**Returns:** `{hits: [{step, score, snippet, kind}]}`
Two-tier search: first an in-memory text-substring scan (fast, cheap); second a semantic search over step-intent embeddings if the substring scan returns <k hits. Embeddings are computed lazily on first `seek` call and cached for the session.

#### `tape.tools`
**Args:** `handle: string, server?: string, tool?: string`
**Returns:** `{calls: [<mcp_call track>]}`
Convenience filter that returns only `mcp_call` tracks, optionally narrowed to a specific server or tool name. Same payload-truncation rule as `tape.play`.

#### `tape.diff`
**Args:** `a_handle: string, b_handle: string, all?: bool`
**Returns:** `{diff: <JSON shape from tape diff --format json>}`
Runs the same diff algorithm as the CLI, returns the JSON form.

### Mutating (in-session only — does not write disk unless `tape.eject` is called)

#### `tape.fork`
**Args:** `handle: string, from_step: int, label?: string`
**Returns:** `{new_handle: string}`
Creates a new in-memory tape that is a deep copy of the source up to `from_step`, inclusive. The new handle can have events appended to it (via subsequent `tape.record` semantics scoped to this handle, or via `tape.annotate`). Forks are NOT recorded in the source — they are separate tapes.

#### `tape.record`
**Args:** `handle?: string, task?: string`
**Returns:** `{handle: string, recording: true}`
Begins recording the current MCP session into a tape. If `handle` is omitted, creates a fresh tape with the given `task` as the first track. Subsequent tool calls within this MCP session are captured as `mcp_call` events on this tape. Calling `tape.record` a second time without `tape.eject` is an error.

#### `tape.annotate`
**Args:** `handle: string, step?: int (default: latest), note: string, by?: "agent"|"human" (default: "agent")`
**Returns:** `{step: int}`
Pins an `annotation` track event. If `step` is omitted and the tape is currently recording, attaches to the most recent step.

#### `tape.eject`
**Args:** `handle: string, out: string, yes?: bool (default true for MCP — agents shouldn't be prompted)`
**Returns:** `{path: string, redactions: int}`
Runs the eject pipeline (artifact spillover, redaction, zip). Note `yes` defaults to `true` because the MCP context has no tty; safety still comes from the redaction engine, not the prompt.

## Tool descriptions (for the MCP `tools/list` response)

Be terse but informative. The description is what an agent reads when deciding whether to call the tool. Aim for one sentence per tool, ≤120 chars. Example:

```
tape.load: "Mount a .tape file. Returns a handle string plus a quick summary; pull track contents with tape.play."
```

Bad: `"Loads a tape."` (uninformative). Bad: a paragraph (wastes tokens in every system message).

## Error shape

All errors follow MCP's `isError: true` content convention plus a structured `code`:

```json
{
  "isError": true,
  "content": [{"type":"text","text":"<human-readable>"}],
  "_meta": {"code":"INVALID_HANDLE"}
}
```

Codes used in v0: `INVALID_HANDLE`, `INVALID_STEP`, `OUT_OF_RANGE`, `MALFORMED_TAPE`, `TAPE_NOT_FOUND`, `ALREADY_RECORDING`, `NOT_RECORDING`, `EJECT_FAILED`.

## Implementation notes

- Use `rmcp` if it's stable at build time; otherwise hand-roll a JSON-RPC server over stdio. The protocol is small.
- All session state is `Arc<Mutex<DeckState>>`. Mutable tools take `&mut`; read tools take `&`. Use a `RwLock` if contention shows up.
- Embeddings cache: keyed by tape `id` + step number. Invalidated when a fork happens (the new fork has its own cache).
- Tools list is static — register once at server start, don't dynamically add/remove.

## Test contract

The `mcp-deck-tester` agent exercises every tool. For each tool, it sends a representative request and asserts:
- Success path returns expected shape.
- A bad-handle / out-of-range request returns the documented error code.
- Read-only tools do not change session state (verifiable by calling `tape.summary` before and after and comparing).
