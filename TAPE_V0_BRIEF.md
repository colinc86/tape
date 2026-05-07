# `tape` — v0 Build Brief

You are building `tape`, a portable record-and-replay format and toolkit for AI agent runs. The unit of work is a `.tape` file that captures a complete agent trajectory (model calls, tool calls, decisions, outcomes) and is portable between engineers, agents, and frameworks.

This brief is everything you need to ship v0. Loop on it until the **Definition of Done** at the bottom is met. Do not ask for clarification on scope — the scope is fixed below. If something genuinely blocks you, make the most defensible choice and document it in `DECISIONS.md`.

## The one-line pitch

> A cassette tape for agent runs. Record once, replay anywhere, share as a file.

## v0 target runtime

**Claude Code only.** v0 is a tool for Claude Code users. The format and the deck MCP are designed to be runtime-agnostic from day one — but we ship, dogfood, and prove the value with Claude Code first. Adapters for Claude Desktop, Codex, and OpenClaw come in later versions. Keeping v0 single-target lets us lean on Claude Code's hook system for shell-level recording instead of building a generic shell wrapper, and lets us focus marketing/docs/examples on one workflow.

## The killer scenario (this MUST work end-to-end in v0)

Engineer A's agent investigates a bug, ejects a cassette, attaches it to a ticket. Engineer B (potentially a different model, different runtime) loads the cassette via the deck MCP and picks up exactly where A's agent left off — with full access to the original investigation, including searching, replaying specific steps, and forking from any point.

If this demo doesn't work, v0 is not done.

## Non-negotiable design principles

1. **Trajectory-level, not call-level.** The unit is the *run*, not the API request. Cassettes capture a coherent story.
2. **Vendor-neutral.** No Anthropic/OpenAI/LangChain/etc. coupling in the format. `tape/v0` is an open spec.
3. **The CLI is for humans, the MCP is for agents, the format is the lingua franca.** Three surfaces, one substrate.
4. **Read-only by default.** `play`/`seek`/`search` never mutate. Mutation requires explicit `fork`/`splice`/`eject`.
5. **Liner notes are mandatory.** Every cassette has human-readable narrative so receiving agents know what they're holding.
6. **Redaction is first-class.** Strips run at eject time, before the file ever lands on disk.
7. **Diff doesn't judge.** It reports facts. Users decide.

## Stack

- **Language:** Rust (stable). One binary, multiple modes.
- **License:** Apache 2.0.
- **Format:** `.tape` is a zip archive. JSONL inside. Spec in `SPEC.md` at the repo root.
- **Crates expected:** `clap` (CLI), `serde`/`serde_json`/`serde_yaml`, `zip`, `tokio`, `axum` or `hyper` (proxy), `reqwest` (outbound), `regex`, `blake3` (content addressing), `similar` (text diff), `rmcp` or hand-rolled MCP server (whichever is more stable at build time).
- **No** runtime deps on Anthropic/OpenAI SDKs. Speak their HTTP protocols directly.

## Format spec (`tape/v0`)

A `.tape` file is a zip archive with this layout:

```
my-bug.tape/
├── meta.yaml          # required
├── liner-notes.md     # required
├── tracks.jsonl       # required, one event per line, ordered
├── artifacts/         # optional, content-addressed by blake3 hash
│   └── ab/cd/<full-hash>.bin
└── redactions.json    # required if any redactions occurred
```

### `meta.yaml`

```yaml
tape_version: "tape/v0"
id: <uuidv7>
created_at: <iso8601>
ejected_at: <iso8601>
task: "<short human description, 1 line>"
recorder:
  agent: "<name/version of recording agent, e.g. 'claude-code/2.1.4'>"
  user: "<optional, redactable>"
models:
  - vendor: anthropic
    model: claude-opus-4-7
    calls: 47
tools:
  - kind: mcp
    server: "filesystem"
    calls: 12
  - kind: mcp
    server: "github"
    calls: 3
outcome: success | failure | abandoned | unknown
tool_budget:
  total_calls: 62
  total_tokens_in: 145203
  total_tokens_out: 8432
  wall_clock_ms: 12340
redaction_summary:
  rules_applied: ["email", "anthropic_api_key", "custom:pii"]
  redaction_count: 47
```

### `liner-notes.md`

Human-readable narrative, ~200–500 words, written by the recording agent at eject time. Sections:

- **What I was asked to do**
- **What I found**
- **Suggested next step / fix**
- **What I'm uncertain about**

This is the first thing a receiving agent reads. Treat it as the cassette's case insert.

### `tracks.jsonl`

One JSON object per line. Required fields: `step`, `kind`, `ts`, `payload`. Optional: `parent_step`, `refs`, `annotations`.

```jsonl
{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"Investigate payment failures..."}}
{"step":2,"kind":"model_call","ts":"...","payload":{"vendor":"anthropic","model":"claude-opus-4-7","request":{...},"response":{...}},"refs":["sha:a1b2..."]}
{"step":3,"kind":"mcp_call","ts":"...","payload":{"server":"db","tool":"query","args":{...},"result":{...}}}
{"step":4,"kind":"annotation","ts":"...","payload":{"by":"agent","note":"smoking gun: race condition here"}}
{"step":5,"kind":"eject","ts":"...","payload":{"outcome":"success"}}
```

Event kinds for v0: `task`, `model_call`, `mcp_call`, `shell`, `file_read`, `file_write`, `annotation`, `eject`. Future kinds reserved: `fork`, `splice`.

`shell`, `file_read`, and `file_write` are sourced from Claude Code's `PreToolUse`/`PostToolUse` hooks — see "Recording" section below.

Payloads exceeding 4 KB MUST go to `artifacts/` and be referenced via `refs: ["sha:<hash>"]`. The track payload then contains a stub like `{"ref":"sha:a1b2..."}`.

### `redactions.json`

Audit trail of every redaction. Each entry: `{step, field_path, rule_id, replacement}`. Replacement is always a typed placeholder like `<EMAIL>`, `<API_KEY:anthropic>`, never the original value or a hash of it.

## Recording: how it works

v0 records Claude Code sessions. Three capture layers, all wired up in v0:

### Layer 1 — Model API proxy

Run `tape record -- claude <args>` and `tape` does the following:

1. Spins up local proxies on free ports for Anthropic API (and OpenAI API for completeness — Claude Code may use either depending on config).
2. Sets `ANTHROPIC_BASE_URL` (and `OPENAI_BASE_URL` if applicable) in the child process to point at the proxy.
3. Runs the child command (`claude ...`).
4. Every model call → recorded as `model_call` track event with full request and response.

The proxy MUST be a transparent passthrough. Streaming responses are recorded *and* streamed to the child without buffering the full response (use a `tee`-style stream split). Failure to stream properly will break Claude Code, which uses streaming for everything.

### Layer 2 — MCP wrapper

`tape` ships a small wrapper binary `tape-mcp-wrap`. When `tape record` runs, it generates a temporary Claude Code MCP config that points at `tape-mcp-wrap` for each configured MCP server. The wrapper subprocesses the real server and tees the JSON-RPC traffic to the recording socket. Each tool call → `mcp_call` track event.

For v0, this works by writing a temporary `mcp.json` to a tmp dir and invoking Claude Code with `--mcp-config <tmpfile>`. We do NOT modify the user's persistent config.

### Layer 3 — Claude Code hooks

This is the win from targeting Claude Code first: shell calls, file reads, and file writes are recorded via `PreToolUse` / `PostToolUse` hooks rather than a generic shell wrapper.

When `tape record` starts, it generates a settings overlay that registers hooks for the built-in tools we care about:

- `Bash` → `shell` track event (command, exit code, stdout, stderr, duration)
- `Read` → `file_read` track event (path, byte range, content hash)
- `Write` / `Edit` → `file_write` track event (path, before-hash, after-hash, diff)

The hooks POST to a local Unix socket that `tape` listens on for the duration of the recording. On child exit (or `SIGINT`), `tape` runs the eject pipeline.

### Eject pipeline

On child exit:

1. Stop accepting new events.
2. Drain in-flight events.
3. Ask the recording agent (via a final API call to its model) to write `liner-notes.md`. Pass it the meta + a compressed track summary. If the recording exited abnormally, skip this step and write a stub.
4. Run redaction pass (see below).
5. Show confirmation prompt (or skip if `--yes`).
6. Write the zip.
7. Clean up temp dir, sockets, hook overlay.

## Redaction pipeline

Runs at eject time, before the zip is written. Two-pass:

**Pass 1 — Pattern-based (deterministic, fast):**

Built-in rules (each has a stable `rule_id`):

- `email` — RFC 5322 emails
- `anthropic_api_key` — `sk-ant-*`
- `openai_api_key` — `sk-*` (with appropriate length/charset)
- `aws_access_key` — `AKIA*` and friends
- `aws_secret_key` — high-entropy 40-char strings near `aws_*` context
- `jwt` — three base64 segments separated by dots
- `ssn` — `\d{3}-\d{2}-\d{4}`
- `credit_card` — Luhn-validated 13–19 digit sequences
- `ipv4_private` — RFC 1918 ranges (off by default; opt-in)
- `bearer_token` — `Bearer <high-entropy>`
- `generic_high_entropy` — strings of length ≥ 32 with high Shannon entropy and no spaces (off by default; opt-in)

User rules from `.taperc`:

```yaml
redact:
  custom:
    - id: pii_customer
      pattern: 'CUST-\d{6}'
    - id: internal_host
      pattern: '\.internal\.justpark\.com'
  enable_optional: ["ipv4_private"]
  disable_default: []
```

**Pass 2 — Confirmation:**

Before writing the file, print a summary:

```
About to eject `my-bug.tape`:
  62 tracks, 47 redactions across 12 tracks
  Rules applied: email (31), anthropic_api_key (2), custom:pii_customer (14)
  
  [y] write tape    [n] cancel    [d] show diff    [e] edit redactions
```

In non-interactive mode (`--yes` or stdin not a tty), proceeds without prompting.

## CLI surface

Implement these in v0. Behaviors below are normative.

### `tape record`

```
tape record [--label LABEL] [--out PATH] [--yes] -- <command...>
```

Records the child process. On normal exit, runs eject pipeline. On `SIGINT`, prompts to eject or discard.

### `tape eject`

Used internally by `record`, but also callable from inside an MCP session via `tape.eject` (see MCP surface). Standalone invocation is rare but allowed for testing: `tape eject --from <session-id> --out PATH`.

### `tape play`

```
tape play <file.tape> [--step N] [--range N..M] [--kind model_call|mcp_call|...]
```

Pretty-prints tracks to stdout. Default: liner notes + meta + abbreviated track listing. With `--step` or `--range`: full payloads.

### `tape ls`

```
tape ls <file.tape>
```

One line per track:

```
  1  task         "Investigate payment failures for customer 4471"
  2  model_call   anthropic/claude-opus-4-7  in:1.2k out:340
  3  mcp_call     db.query("SELECT * FROM payments WHERE...")
  4  annotation   "smoking gun: race condition here"
  ...
```

### `tape diff`

```
tape diff <a.tape> <b.tape> [--all] [--format text|json] [--judge MODEL]
```

Three-pass: align (Needleman-Wunsch over step-intent embeddings), classify (identical/cosmetic/substantive/causal), narrate (small judge model, default `claude-haiku-4-5`).

Output (default `text`):

```
Task: "<from meta>"
Outcome: <a> vs <b>

▸ Track NN  <classification>  · <one-line summary>
    before: <snippet>
    after:  <snippet>
    why:    <judge narration, 1-2 sentences>
    impact: flows into Track XX → YY  (only if causal)

Final answers: <semantically equivalent | divergent>
Tool budget:   before X calls · after Y calls (±Z%)
Latency:       before X ms · after Y ms (±Z%)
```

`--format json` emits a machine-readable structure (used by the `tape.diff` MCP tool).

For v0, embeddings can be a small local model (e.g. `all-MiniLM-L6-v2` via `candle` or similar) OR an API call to a cheap embedding endpoint. Pick whichever ships faster; document the choice in `DECISIONS.md`.

### `tape verify`

```
tape verify <file.tape>
```

Validates the cassette against `tape/v0` schema: required files present, JSONL parses, refs resolve, no PII in `meta.yaml` or `liner-notes.md` (per the redaction summary). Exits 0 on valid, non-zero with diagnostics on invalid.

## The deck (MCP server)

Run `tape mcp` to start. Speaks MCP over stdio by default. Tools exposed:

| Tool | Purpose | Mutates? |
|---|---|---|
| `tape.load` | Mount a `.tape` file. Returns a session handle. | No (session-local) |
| `tape.summary` | Returns meta + liner notes for the loaded tape. | No |
| `tape.tracks` | Returns track list with brief labels. Supports `filter` (kind, range, regex). | No |
| `tape.play` | Returns full content of one step or range. | No |
| `tape.seek` | Semantic + text search across tracks. | No |
| `tape.tools` | Returns just `mcp_call` tracks, optionally filtered. | No |
| `tape.record` | Begins recording the current MCP session. | Yes (starts session state) |
| `tape.annotate` | Pin a note to a step (or "now" if recording). | Yes (in-session) |
| `tape.eject` | Save current recording to a path. | Yes (writes file) |
| `tape.fork` | Branch from a step into a new in-memory tape. | No (session-local) |
| `tape.diff` | Compare two loaded tapes, return JSON diff. | No |

Important: an agent calling `tape.load` does NOT receive the full tape contents. It receives a handle and a summary. The agent must call `tape.tracks`/`tape.play`/`tape.seek` to pull in specific slices. This is what makes large tapes usable as external memory rather than context-window bombs.

## Build order

Don't build everything at once. This order is chosen so each step delivers a testable artifact:

1. **`SPEC.md`** — write the format spec first, before any code. Reviewable on its own.
2. **`tape verify`** — validates files against the spec. Lets you write fixture tapes and check them.
3. **Hand-crafted fixture tapes** — write 2-3 example `.tape` files by hand (or with a script). Use them throughout dev.
4. **`tape play` / `tape ls`** — read-side tools. Operates on fixtures only, no recording yet.
5. **Anthropic proxy + `tape record`** — MVP recording: just model calls from a Claude Code session. No MCP, no hooks yet. End of this step: `tape record -- claude "say hi"` produces a tape with one `task` and one or more `model_call` events.
6. **MCP wrapper + recording** — extend recording to MCP calls via the temporary mcp.json mechanism.
7. **Claude Code hooks integration** — add `Bash`/`Read`/`Write`/`Edit` capture via the hook overlay + Unix socket. This is the v0-distinguishing feature.
8. **OpenAI proxy** — second vendor proves the proxy abstraction works (Claude Code occasionally uses other models for sub-agents).
9. **Redaction pipeline** — at eject time. Built-in rules + `.taperc` loading.
10. **Liner notes generation** — at eject time, ask the recording agent's last model to write them. (If no model available, eject with a stub and let the agent fill it in via `tape.annotate` before final save.)
11. **`tape mcp`** — the deck. Read tools first (`load`, `summary`, `tracks`, `play`, `seek`, `tools`), then write tools (`record`, `annotate`, `eject`, `fork`).
12. **`tape diff`** — last, because it's the hardest and depends on everything else.

After each step: commit, write a test, update README. Do not skip tests — this is a tool whose entire value is reliability.

## Testing

- Unit tests for: format parsing, redaction rules (each rule gets at least 5 positive and 5 negative cases), proxy stream splitting, alignment algorithm.
- Integration test for the killer scenario (Engineer A → Engineer B): use the Anthropic API with a recorded fixture cassette as the "real" backend, run a small agent that produces a tape, then run another agent that loads it via MCP and answers a question requiring tape contents.
- **Fixture tapes live in `tests/fixtures/`** and are checked in. Critical that they are themselves valid `tape/v0` files — run `tape verify` against them in CI.

## Repository layout

```
tape/
├── Cargo.toml
├── README.md
├── SPEC.md
├── DECISIONS.md         # log of non-obvious choices made during the build
├── LICENSE              # Apache 2.0
├── src/
│   ├── main.rs          # CLI entrypoint, dispatches to subcommands
│   ├── format/          # tape/v0 spec impl: read, write, verify
│   ├── record/          # proxies (anthropic, openai, mcp), session mgmt
│   ├── redact/          # rule engine, built-in rules, .taperc loading
│   ├── play/            # ls, play, seek
│   ├── diff/            # align, classify, narrate
│   ├── mcp/             # the deck — server + tool implementations
│   └── lib.rs
├── tests/
│   ├── fixtures/        # example .tape files
│   ├── integration/     # end-to-end tests
│   └── unit/
└── docs/
    ├── format.md        # human-readable companion to SPEC.md
    ├── cli.md
    └── mcp.md
```

## README.md (write this early, refine as you go)

The README is the single most important marketing surface. Structure it like this:

1. One-line pitch + a 30-second GIF (placeholder for now) showing record → eject → ticket → load → resume
2. Install (`brew install` and `curl | sh` and `cargo install`)
3. The 60-second tutorial: record a session, eject, play it back
4. The Engineer A → Engineer B scenario, written as a literal walkthrough with copy-pasteable commands
5. CLI reference (auto-generated from `--help` is fine)
6. MCP reference
7. Format spec link (`SPEC.md`)
8. FAQ: "is this like vcrpy/llmock?" — answer honestly. They record API calls; we record agent runs. Different unit, different scope.

## Definition of Done (v0)

v0 is done when ALL of these are true:

- [ ] `SPEC.md` is complete, accurate, and matches the implementation.
- [ ] `tape record -- claude <args>` produces a valid `.tape` file capturing model calls, MCP calls, and Claude Code Bash/Read/Write/Edit tool invocations.
- [ ] `tape verify` validates correctly-formed tapes and rejects malformed ones with useful errors.
- [ ] `tape play` and `tape ls` produce readable output for any valid tape.
- [ ] `tape diff` produces a useful comparison between two tapes recorded against the same task.
- [ ] `tape mcp` exposes all 11 deck tools and responds correctly to MCP protocol — usable from Claude Code via `claude mcp add tape -- tape mcp`.
- [ ] Built-in redaction rules (email, anthropic_api_key, openai_api_key, aws_access_key, aws_secret_key, jwt, ssn, credit_card, bearer_token) all have unit tests.
- [ ] Custom rules via `.taperc` work end-to-end.
- [ ] Streaming responses are recorded AND streamed to Claude Code without buffering full response (Claude Code must remain responsive during recording).
- [ ] Hook overlay installs and uninstalls cleanly — no residue in user's Claude Code settings after `tape record` exits, even on `SIGKILL`.
- [ ] **The killer scenario test passes:** a fixture tape produced by one Claude Code session is loaded by another Claude Code session via the deck MCP, and the second session successfully answers a question that requires reading specific tracks from the tape.
- [ ] README is publishable. Walking through the README on a clean machine with Claude Code already installed gets a new user to a working `tape record` in under 5 minutes.

If you run into a genuine ambiguity that isn't resolved by this brief, make the most defensible choice, document it in `DECISIONS.md` with rationale, and continue. Do not stop and ask.

When v0 is done, write a `RELEASE_NOTES.md` summarizing what shipped, what didn't, and what's planned for v0.1 (Claude Desktop adapter) and v0.2 (Codex/OpenAI Agents and OpenClaw adapters), plus a `tape splice` operation and a hosted cassette registry as longer-horizon items.

Now build it.
