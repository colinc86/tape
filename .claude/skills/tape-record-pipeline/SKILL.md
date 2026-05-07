---
name: tape-record-pipeline
description: Recording architecture — the 3 capture layers (HTTP API proxy, MCP wrapper, Claude Code hooks), the eject pipeline, the recorder Unix socket protocol, and the temporary mcp.json / settings overlay mechanisms. Load when working in src/record/ or src/main.rs's record subcommand.
---

# Recording architecture

`tape record` is a parent process that wires three independent capture layers into a single stream of `tracks.jsonl` events, then runs an eject pipeline at child exit.

```
                  ┌──────────────────────────────────────────┐
                  │          tape record (parent)            │
                  │  ┌──────────┐  ┌──────────┐  ┌────────┐  │
                  │  │ proxy    │  │ recorder │  │ eject  │  │
                  │  │ servers  │──│ socket   │──│ writer │  │
                  │  └──────────┘  └──────────┘  └────────┘  │
                  └─────▲──────────▲────────▲─────▲──────────┘
                        │          │        │     │
       ANTHROPIC_BASE_URL│          │        │     │OPENAI_BASE_URL
                        │          │        │     │
                  ┌─────┴──────────┴────────┴─────┴──────────┐
                  │  child process: claude <args>            │
                  │   ├─ model API calls      → proxy        │
                  │   ├─ MCP RPC traffic      → tape-mcp-wrap│
                  │   └─ Bash/Read/Write/Edit → hooks        │
                  └──────────────────────────────────────────┘
```

## Layer 1 — Model API proxy

- Spin up local HTTP servers on free ports for **Anthropic** and **OpenAI** API shapes.
- Set `ANTHROPIC_BASE_URL` (and `OPENAI_BASE_URL` if applicable) on the child env.
- For each request: tee the request to a `model_call` track event, forward to the real upstream, tee the response back.
- **Streaming is non-negotiable.** SSE responses MUST be split with a `tee`-style stream — bytes flow through to the child as they arrive from upstream, while a clone goes to the recorder. **Never wait for the full response before yielding to the child.** Claude Code uses streaming for everything; buffering breaks the UI.
- Pattern: `reqwest::Response::bytes_stream()` → `futures::stream::StreamExt::tee` (or hand-rolled `tokio::sync::broadcast`) → one branch to `axum`/`hyper` body, other to recorder.
- Recorder receives the *concatenated* response body for the track event, but only after the stream completes. Child does not wait.
- Errors from upstream propagate to the child unmodified, and are recorded with whatever partial response was streamed.

## Layer 2 — MCP wrapper

- Ship a tiny binary `tape-mcp-wrap` alongside `tape`.
- At record start, write a temporary `mcp.json` to `$TMPDIR/tape-XXXX/mcp.json` listing each user-configured MCP server, but with `command: tape-mcp-wrap` and the original command/args passed via env vars (`TAPE_WRAP_CMD`, `TAPE_WRAP_ARGS_JSON`, `TAPE_WRAP_SOCKET`).
- Invoke Claude Code with `--mcp-config <tmp-mcp.json>`. Do NOT touch the user's persistent `~/.claude.json` or `.mcp.json`.
- `tape-mcp-wrap`:
  1. Connects to the recorder Unix socket at `$TAPE_WRAP_SOCKET`.
  2. Subprocesses the real MCP server.
  3. Tees JSON-RPC traffic in both directions: each `tools/call` request paired with its response → `mcp_call` event.

## Layer 3 — Claude Code hooks

- At record start, generate a Claude Code settings overlay file (e.g. `--settings <path>` if supported, or via `CLAUDE_SETTINGS_PATH` if not) registering hooks:
  - `PreToolUse` + `PostToolUse` for `Bash` → `shell` track event
  - `PostToolUse` for `Read` → `file_read` track event
  - `PostToolUse` for `Write`, `Edit`, `MultiEdit` → `file_write` track event
- Each hook is a tiny shell command that POSTs the event JSON to the recorder Unix socket.
- The overlay lives in the temp dir and is cleaned up at exit. **No residue in user's settings on `SIGKILL`** — guarantee this with a fresh temp dir per run, never editing user settings in-place.

## Recorder socket protocol

A Unix domain socket at `$TMPDIR/tape-<run-id>/recorder.sock`, line-delimited JSON.

Each line is a single track event minus the `step` field (the recorder assigns step numbers monotonically as events arrive). The recorder timestamps the event on receipt if `ts` is missing.

```json
{"kind":"shell","payload":{"command":"ls","exit_code":0,"stdout":"...","stderr":"","duration_ms":12}}
```

Senders are: the proxy (in-process — bypasses the socket via direct channel), `tape-mcp-wrap`, and Claude Code hook scripts. The proxy's in-process path is preferred for low overhead; the socket exists primarily for out-of-process senders.

## Eject pipeline (on child exit or SIGINT)

```
1. Stop accepting events on socket. Drain in-flight events.
2. Inject final task summary if missing.
3. Resolve oversized payloads → write to artifacts/, replace inline.
4. If recording exited normally: ask recorder agent's last model to write liner-notes.md.
   If exited abnormally (non-zero, SIGKILL): write a stub liner-notes with what we know.
5. Run redaction pass over tracks.jsonl + meta.yaml + liner-notes.md.
6. Print confirmation summary; prompt unless --yes or non-tty.
7. Zip the directory into <out>.tape.
8. Clean up: temp dir, sockets, hook overlay, mcp.json overlay.
```

## SIGINT handling

On the first `SIGINT`, do NOT propagate to the child immediately. Instead:
1. Forward SIGINT to the child (Claude Code handles it gracefully — finishes the current turn).
2. Wait for child to exit (with a 30s timeout).
3. Run eject pipeline normally; prompt with `[e]ject  [d]iscard  [c]ontinue?`.

On a second `SIGINT` during the prompt: discard.

## Cleanup invariant

After `tape record` exits — by any path including panic, SIGKILL of the parent (impossible to clean up from inside, but document this) — the user's persistent Claude Code config MUST be untouched. Achieve this by:

- Using only `--mcp-config <tmpfile>` and `--settings <tmpfile>` (or env-based equivalents). Never `claude mcp add` or any persistent-write command.
- Writing the temp dir under `$TMPDIR/tape-<run-id>/` so a stale temp dir is recognizable on next launch.
- A startup sweep that removes orphan `tape-*` dirs older than 24h.

## Failure modes to test

- Upstream Anthropic 5xx mid-stream → child sees the partial stream + error chunk; tape contains the partial response.
- Child crashes before any model call → tape has `task`, no `model_call`, an `eject` with outcome `failure`, stub liner notes.
- Child sends a non-JSON-RPC frame to MCP wrapper → wrapper logs the malformed frame as an `annotation`, forwards verbatim.
- Hook script fails → POST is retried once; on second failure, drop the event and emit an `annotation` noting the drop.
