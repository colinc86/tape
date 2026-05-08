<div align="center">

# tape 📼

**A cassette tape for agent runs. Record once, replay anywhere, share as a file.**

![format: tape/v0](https://img.shields.io/badge/format-tape%2Fv0-purple)
![runtime: claude code](https://img.shields.io/badge/runtime-claude%20code-orange)
![status: v0.1.1](https://img.shields.io/badge/status-v0.1.1-blue)
![tests: 106](https://img.shields.io/badge/tests-106%20passing-brightgreen)
![license: apache 2.0](https://img.shields.io/badge/license-apache%202.0-lightgrey)

![tape demo: verify, ls, play](./docs/demo.gif)

</div>

`tape` captures the messiest artifact in software — an AI agent's actual investigation — into a single file you can hand to a colleague, a different agent, or your future self. It records every model call, tool call, file edit, and pinned insight. The receiving agent loads it via MCP and rewinds to exactly where you left off.

## Install

`tape` runs in two modes that share the same binaries:

- **As a Claude Code plugin** — adds `/tape:tape-snapshot`, `/tape:tape-resume`, `/tape:tape-list`, and `/tape:tape-record-help` to your sessions, and registers a `tape` MCP server. *(Most people want this.)*
- **As a CLI** — standalone `tape verify` / `play` / `record` / `diff` / `mcp` commands on your `PATH`. Useful for scripting and non-Claude-Code workflows.

### As a Claude Code plugin

```console
git clone https://github.com/colinc86/tape
/plugin marketplace add ./tape/marketplace
/plugin install tape@tape-marketplace
```

The plugin bundles `tape`, `tape-mcp-wrap`, and `tape-hook` (macOS Apple Silicon binaries at v0.1).

<details>
<summary><b>As a CLI</b> — prebuilt download or build from source</summary>

#### Prebuilt (macOS Apple Silicon)

Grab the tarball + checksums from the [v0.1.0 release](https://github.com/colinc86/tape/releases/tag/v0.1.0):

```console
curl -LO https://github.com/colinc86/tape/releases/download/v0.1.0/tape-v0.1.0-aarch64-apple-darwin.tar.gz
curl -LO https://github.com/colinc86/tape/releases/download/v0.1.0/SHA256SUMS
shasum -a 256 -c SHA256SUMS
tar xzf tape-v0.1.0-aarch64-apple-darwin.tar.gz
mv tape tape-hook tape-mcp-wrap /usr/local/bin/
```

#### From source (any platform)

```console
git clone https://github.com/colinc86/tape
cd tape
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

</details>

## A tape in the wild

**Act I — record.** ▶  You're three hours into investigating why customer 4471's payments keep failing. You find the bug: a race in `process_refund()`. Before you context-switch:

```console
/tape:tape-snapshot bug-447
```

`bug-447.tape` lands in your repo. You attach it to the Jira ticket.

**Act II — rewind.** ⏪  Wednesday morning, your colleague picks up the ticket. In a fresh Claude Code session:

```console
/tape:tape-resume bug-447.tape
```

Their agent loads the cassette, reads the liner notes (the four-paragraph narrative you didn't have to write — it's auto-generated), finds the smoking-gun annotation you pinned, and writes the fix you suggested.

> Two days of context transfer, eliminated. That's the only sales pitch you'll find in this README.

## Three ways to record

Pick the path that matches your situation. **Default to `/tape:tape-snapshot`** unless you know why you want one of the others.

| | When you reach for it |
|---|---|
| ⏺&nbsp; **`/tape:tape-snapshot`** *(in-session)* | Mid-session and you want a tape NOW. **Default.** |
| 🎚&nbsp; **`tape record -- claude`** *(CLI proxy)* | Scripted runs, non-interactive `claude -p`, or you need raw HTTP fidelity (streaming chunk timing, exact request bodies). |
| ⏏&nbsp; **`tape.record` + annotate + eject** *(MCP, in-memory)* | The agent assembles a synthetic tape from a few annotations. Niche. |

All three produce valid `tape/v0` files; `meta.recorder.agent` distinguishes them downstream.

## Reading a tape

From a shell:

```console
tape verify <file>           # validates against tape/v0; exits 0 or 2
tape ls <file>               # one line per track
tape play <file> --step N    # full payload of one step
tape diff <a> <b>            # compare two runs (text or --format json)
tape mcp                     # serve the deck over stdio (used by the plugin)
```

From inside a Claude Code session, the deck (`tape mcp`) exposes 12 tools. Mutating tools are marked ⏏.

| Tool | What it does |
|---|---|
| `tape.load` | Mount a `.tape` file. Returns a session handle plus a quick summary. |
| `tape.summary` | Meta + liner notes for a handle. |
| `tape.tracks` | Lightweight track listing (filter by kind, range, regex). |
| `tape.play` | Full payload for one step or a range. 200 KB cap. |
| `tape.seek` | Substring search across track payloads. |
| `tape.tools` | Just the `mcp_call` tracks, optionally narrowed. |
| `tape.diff` | Compare two loaded tapes; returns the diff JSON. |
| `tape.fork` ⏏ | Branch from a step into a new in-memory handle. |
| `tape.record` ⏏ | Begin an in-memory recording in this MCP session. |
| `tape.annotate` ⏏ | Pin a note to a step. |
| `tape.eject` ⏏ | Save a recording or fork to a `.tape` file on disk. |
| `tape.snapshot` ⏏ | *(v0.1)* Capture this Claude Code session's transcript as a `.tape` file in one shot. |

The handle-not-contents rule: `tape.load` returns a string handle, not bytes. Track payloads come on demand — fast-forward to the steps you care about, skip the boring bits. A 50 MB tape coexists with a 200 K context window.

## What's on the cassette

```
bug-447.tape
├── meta.yaml          ← who recorded what, when, with what outcome
├── liner-notes.md     ← the case insert (200–500 words; four required sections)
├── tracks.jsonl       ← every event, in order
├── artifacts/         ← content-addressed blobs for payloads >4 KiB
│   └── ab/cd/<blake3-hash>.bin
└── redactions.json    ← audit trail of every redaction (when redactions occurred)
```

A `.tape` is a ZIP archive. Eight closed event kinds: `task`, `model_call`, `mcp_call`, `shell`, `file_read`, `file_write`, `annotation`, `eject`. Normative spec: [`SPEC.md`](./SPEC.md).

## Redaction

`tape` is paranoid by default. Every email, API key, AWS credential, JWT, Luhn-valid credit card, and bearer token is replaced with a typed placeholder before the file is written. A defense-in-depth scan re-checks `meta.yaml` and `liner-notes.md` after redaction; any leak there hard-fails the eject.

If that's not paranoid enough, drop a `.taperc`:

```yaml
redact:
  custom:
    - id: pii_customer
      pattern: 'CUST-\d{6}'
  enable_optional: ["ipv4_private"]
```

Built-in rules: `email`, `anthropic_api_key`, `openai_api_key`, `aws_access_key`, `jwt`, `ssn`, `credit_card`, `bearer_token`, plus `ipv4_private` and `generic_high_entropy` as opt-ins.

## Liner notes

Frequently asked, dryly answered.

**Is this vcrpy / llmock?**  No, but they're adjacent. vcrpy and llmock record HTTP calls and replay them as deterministic fixtures. `tape` records *runs* — model turns, tool calls, file edits, decisions — as a single coherent trajectory portable between agents and engineers. Different unit, different scope.

**Why a closed event-kind enum if Claude Code keeps adding tools?**  Because `tape verify` is load-bearing. A closed enum is the only way verify can refuse a malformed cassette without guessing. Claude Code's built-in tools (Grep, Glob, WebFetch, etc.) map to `Kind::McpCall` with `payload.server = "builtin"`.

**Does the cassette metaphor spread to every section header?**  ~~Yes, every single one.~~  No.

**What if my session isn't on Claude Code?**  v0.1 is Claude Code only. Adapters for Claude Desktop, Codex, and OpenClaw are in the tracklist below.

## Tracklist

| | What |
|---|---|
| **v0.1** *(now)* | in-session `/tape:tape-snapshot`, plugin marketplace, redaction + defense-in-depth, the 12-tool deck |
| **v0.2** | Claude Desktop adapter, interactive eject prompt, embedding-based diff alignment, judge-model narration |
| **v0.3+** | Codex / OpenAI Agents adapter, OpenClaw, `tape splice`, hosted cassette registry, cross-platform binary distribution |

Full changelog: [`RELEASE_NOTES.md`](./RELEASE_NOTES.md).

## License

[Apache 2.0](./LICENSE).
