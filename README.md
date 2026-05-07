# `tape`

> A cassette tape for agent runs. Record once, replay anywhere, share as a file.

`tape` is a portable record-and-replay format and toolkit for AI agent runs. The unit of work is a `.tape` file that captures a complete agent trajectory — model calls, tool calls, decisions, outcomes — and is portable between engineers, agents, and frameworks.

v0.1 ships for **Claude Code** with two ingestion paths: an in-session MCP tool (`/tape:tape-snapshot`) and a CLI proxy (`tape record -- claude`). Adapters for Claude Desktop, Codex, and OpenClaw come in later versions; the format and the deck (MCP) are runtime-agnostic from day one.

---

## Install

### From the plugin marketplace (recommended, 30 seconds)

```sh
# add this repo's marketplace as a source (or point at a published one)
/plugin marketplace add /path/to/tape/marketplace
# install the plugin
/plugin install tape@tape-marketplace
```

That's it. The plugin bundles all three binaries (`tape`, `tape-mcp-wrap`, `tape-hook`), registers a `tape` MCP server in your Claude Code session, and adds four slash commands:

- `/tape:tape-snapshot <name>` — capture this session as a `.tape` file.
- `/tape:tape-resume <path>` — load a `.tape` and pick up where it left off.
- `/tape:tape-list` — list `.tape` files in the project.
- `/tape:tape-record-help` — print the CLI recording incantation.

> **Platform note:** the bundled binaries are **macOS Apple Silicon** at v0.1. For other platforms, build from source (below).

### Building from source

Required if you want the `tape` CLI on your `PATH` for `tape record -- claude` flows, or if you're not on macOS arm64.

```sh
git clone <repo-url>
cd tape
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

This puts `tape`, `tape-mcp-wrap`, and `tape-hook` on your `PATH`.

---

## Two-minute tour

### Capture this Claude Code session

In any active session with the plugin installed:

```
/tape:tape-snapshot my-investigation
```

That writes `./my-investigation.tape` containing the full session — every prompt, every model turn, every tool call from the start of the session to now. Redaction (emails, API keys, AWS credentials, JWTs, etc.) runs automatically.

### Inspect it

From a shell:

```sh
tape ls my-investigation.tape
#   1  task          "Investigate the bug"
#   2  model_call    anthropic/claude-opus-4-7
#   3  shell         grep -n process_refund src/payments.rs
#   4  file_read     read("src/payments.rs")
#   ...

tape play my-investigation.tape --step 4   # full payload of one track
tape verify my-investigation.tape          # spec-conformance check
```

### Hand it to another agent

In a fresh Claude Code session:

```
/tape:tape-resume my-investigation.tape
```

The agent calls `tape.load`, reads the liner notes, calls `tape.seek` for key moments, and synthesizes a continuation that references what the prior agent found.

---

## How to record

There are three recording paths. Pick the one that matches your situation.

| Path | Effort | Fidelity | Use when |
|---|---|---|---|
| **`/tape:tape-snapshot <name>`** *(in-session)* | one slash command | medium — derives from Claude Code's transcript | you're already mid-session and want a tape now. **Default.** |
| **`tape record -- claude <args>`** *(CLI proxy)* | new shell, wrap `claude` | high — raw HTTP bodies, real streaming chunk timing | you're starting fresh, scripting a non-interactive `claude -p`, or need network-level fidelity |
| **`tape.record` + `tape.annotate` + `tape.eject`** *(in-memory MCP)* | three MCP calls from the agent | low — only what the agent annotates | scripted MCP-side use cases, building synthetic tapes from a few annotations |

Both ingestion paths produce valid `tape/v0` files that pass `tape verify` and load identically into the deck. `meta.recorder.agent` distinguishes them (`tape-mcp/0.1+transcript` vs `tape-cli/0.1+proxy`).

### CLI proxy example

```sh
tape record --task "find why payments fail for customer 4471" \
            --out bug-447.tape \
            -- claude "look at src/payments.rs and the recent failures in /var/log/payments.log"
```

When the child `claude` exits, `bug-447.tape` is written to the current directory.

---

## The killer scenario — Engineer A → Engineer B

The demo `tape` was designed around.

**Engineer A** investigates a bug in their Claude Code session, then ejects the cassette:

```
/tape:tape-snapshot bug-447
```

They attach `bug-447.tape` to a Jira ticket.

**Engineer B** picks up the ticket. With the plugin installed, in a fresh Claude Code session:

```
/tape:tape-resume bug-447.tape
```

Their agent:

1. Calls `tape.load` with the path. Gets a handle plus a quick summary.
2. Reads `tape.summary` for the liner notes.
3. Calls `tape.seek` for "smoking gun" — finds Engineer A's pinned annotation.
4. Calls `tape.play` to read the surrounding context.
5. Synthesizes an answer that references Engineer A's findings, ending with the prior agent's suggested next step verbatim.

The integration test for this round-trip lives in `crates/tape-cli/tests/killer_scenario.rs` and is part of CI.

---

## Why a `.tape` file?

Three properties the format guarantees:

- **Trajectory-level, not call-level.** The unit is the *run*, not the API request. A cassette captures a coherent story.
- **Vendor-neutral.** No Anthropic / OpenAI / LangChain coupling. `tape/v0` is an open spec.
- **Read-only by default.** `play` / `seek` / `search` never mutate. Mutation requires explicit `fork` / `splice` / `eject`.

And one feature that makes large tapes usable as agent memory:

- **Handle, not contents.** When an agent calls `tape.load`, it gets a session handle, not the full bytes. Track contents are pulled on demand. A 50 MB tape coexists with a 200 K context window.

---

## CLI reference

> If you installed the plugin and only use the slash commands, this section is optional.

```
tape verify <file> [--json]
tape ls <file>
tape play <file> [--step N | --range A..B | --kind K]
tape record [--label L] [--out PATH] [--task T] [--upstream-anthropic URL]
            [--upstream-openai URL] -- <command...>
tape diff <a.tape> <b.tape> [--all] [--format text|json]
tape mcp                                   # MCP server over stdio
tape eject --from <session-id> --out PATH  # rare; used internally by record
```

Full per-command output is in `docs/cli.md` (auto-generated from `--help`).

---

## MCP reference (the deck)

The deck exposes 12 tools over MCP:

| Tool | Purpose | Mutates? |
|---|---|---|
| `tape.load` | Mount a `.tape` file. Returns a handle plus a quick summary. | session-local |
| `tape.summary` | Returns meta + liner-notes for a handle. | no |
| `tape.tracks` | Lightweight track listing. Filter by kind, range, regex. | no |
| `tape.play` | Full payload for one step or range (200 KB cap). | no |
| `tape.seek` | Substring search across track payloads. | no |
| `tape.tools` | Just `mcp_call` tracks, optionally filtered. | no |
| `tape.diff` | Compare two loaded tapes; returns the diff JSON. | no |
| `tape.fork` | Branch from a step into a new in-memory handle. | session-local |
| `tape.record` | Begin an in-memory recording in this MCP session. | yes |
| `tape.annotate` | Pin a note to a step (or "now" if recording). | yes |
| `tape.eject` | Save a handle (recording or fork) to a `.tape` file. | yes |
| `tape.snapshot` *(v0.1)* | Capture this Claude Code session's transcript as a `.tape` file in one shot. | writes to disk |

Full schema is in `docs/mcp.md`.

---

## Format spec

Normative: [`SPEC.md`](./SPEC.md). Companion docs: `docs/format.md`.

A `.tape` file is a ZIP archive containing:

- `meta.yaml` — recording metadata
- `liner-notes.md` — human-readable narrative (4 mandatory sections)
- `tracks.jsonl` — ordered events (8 v0 kinds: `task`, `model_call`, `mcp_call`, `shell`, `file_read`, `file_write`, `annotation`, `eject`)
- `artifacts/` — content-addressed blobs for any payload field >4 KiB
- `redactions.json` — audit trail of every redaction (when redactions occurred)

The closed event-kind enum is preserved across recording paths. Built-in non-MCP Claude Code tools (Grep, Glob, WebFetch, etc.) map to `Kind::McpCall` with `payload.server = "builtin"`. See [`DECISIONS.md`](./DECISIONS.md) §D3 for rationale.

---

## Redaction

Runs at eject time, before the file lands on disk.

1. **Pattern-based.** Built-in rules + custom rules from `.taperc`, applied to every string field in every track payload, plus `meta.yaml` and `liner-notes.md`.
2. **Defense-in-depth.** After redaction, scan `meta.yaml` and `liner-notes.md` against ALL built-in rules; eject hard-fails if any match (a redaction-engine bug would surface here, not in the output).

> **v0.1 caveat:** the interactive `[y/n/d/e]` confirmation prompt described in the brief is a v0.2 line item. Today, eject is non-interactive — redaction always runs, with no prompt to inspect the diff.

Built-in rules: `email`, `anthropic_api_key`, `openai_api_key`, `aws_access_key`, `jwt`, `ssn`, `credit_card` (Luhn-validated), `bearer_token`, plus `ipv4_private` and `generic_high_entropy` as opt-ins.

Custom rules in `.taperc`:

```yaml
redact:
  custom:
    - id: pii_customer
      pattern: 'CUST-\d{6}'
  enable_optional: ["ipv4_private"]
```

---

## Is this like vcrpy / llmock?

Honestly: **no, but they're adjacent**. vcrpy / llmock record API calls and replay them as deterministic test fixtures. `tape` records *agent runs* — model calls, tool calls, file edits, shell commands, decisions — as a single coherent trajectory portable between agents and engineers. Different unit, different scope.

You can build a vcrpy-like flow with `tape` (`tape replay` is on the roadmap), but the format's primary purpose is human-and-agent collaboration, not test-fixture playback.

---

## What's next

- **v0.2** — Claude Desktop adapter, interactive eject confirmation prompt, embedding-based diff alignment, judge-model narration.
- **v0.3+** — Codex / OpenAI Agents adapter, OpenClaw adapter, `tape splice`, hosted cassette registry, cross-platform binary distribution.

See [`RELEASE_NOTES.md`](./RELEASE_NOTES.md) for the full roadmap and the v0 / v0.1 changelog.

---

## License

Apache 2.0.
