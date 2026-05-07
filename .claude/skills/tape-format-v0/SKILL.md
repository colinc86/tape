---
name: tape-format-v0
description: Reference card for the tape/v0 file format — zip layout, meta.yaml schema, tracks.jsonl event kinds, artifact addressing, redactions.json. Load when working in src/format/, authoring fixtures, implementing tape verify, or writing SPEC.md.
---

# `tape/v0` format reference

A `.tape` file is a **zip archive**. Implementations MUST be able to read it as a zip; the `.tape` extension is a hint, not a content discriminator.

## Required layout

```
<name>.tape (zip)
├── meta.yaml          REQUIRED
├── liner-notes.md     REQUIRED
├── tracks.jsonl       REQUIRED, ordered, one event per line
├── artifacts/         OPTIONAL, content-addressed
│   └── ab/cd/<full-blake3-hash>.bin
└── redactions.json    REQUIRED if any redactions occurred (omit if zero)
```

Hash sharding: artifact path is `artifacts/<first-2-hex>/<next-2-hex>/<full-hash>.bin` where `<full-hash>` is the blake3 hex digest of the bytes. Sharding keeps directory sizes small for tapes with many artifacts.

## `meta.yaml` schema

Required fields:

| Field | Type | Notes |
|---|---|---|
| `tape_version` | string | MUST be `"tape/v0"` for this revision |
| `id` | string | UUIDv7 |
| `created_at` | string | ISO-8601, time the recording started |
| `ejected_at` | string | ISO-8601, time the file was written |
| `task` | string | One-line human description |
| `recorder.agent` | string | e.g. `"claude-code/2.1.4"` |
| `outcome` | enum | `success` \| `failure` \| `abandoned` \| `unknown` |

Optional but conventional:

```yaml
recorder:
  agent: "claude-code/2.1.4"
  user: "<redactable>"        # optional, gets redacted to <USER> by default
models:
  - vendor: anthropic
    model: claude-opus-4-7
    calls: 47
tools:
  - kind: mcp
    server: "filesystem"
    calls: 12
tool_budget:
  total_calls: 62
  total_tokens_in: 145203
  total_tokens_out: 8432
  wall_clock_ms: 12340
redaction_summary:
  rules_applied: ["email", "anthropic_api_key", "custom:pii"]
  redaction_count: 47
```

`redaction_summary` MUST be present iff `redactions.json` exists.

## `liner-notes.md`

Plain markdown, ~200–500 words, written by the recording agent at eject time. Mandatory section headings (level-2):

```markdown
## What I was asked to do
## What I found
## Suggested next step / fix
## What I'm uncertain about
```

A receiving agent reads liner notes first, before pulling tracks. They are case-insert copy, not log spam.

## `tracks.jsonl`

One JSON object per line, terminated by `\n`. Lines MUST be in `step` order, `step` starts at 1, no gaps.

Required keys per event: `step`, `kind`, `ts`, `payload`.
Optional keys: `parent_step` (int), `refs` (array of `"sha:<hex>"` strings), `annotations` (array).

### v0 event kinds (closed set)

| `kind` | Source | Payload shape (sketch) |
|---|---|---|
| `task` | injected at start by `tape record` | `{prompt: string}` |
| `model_call` | model API proxy | `{vendor, model, request, response, stream_chunks?}` |
| `mcp_call` | `tape-mcp-wrap` | `{server, tool, args, result, error?}` |
| `shell` | Claude Code `Bash` hook | `{command, exit_code, stdout, stderr, duration_ms}` |
| `file_read` | Claude Code `Read` hook | `{path, byte_range?, content_hash}` |
| `file_write` | Claude Code `Write`/`Edit` hook | `{path, before_hash, after_hash, diff?}` |
| `annotation` | agent or human | `{by: "agent"\|"human", note: string}` |
| `eject` | always last | `{outcome: "success"\|"failure"\|"abandoned"\|"unknown"}` |

**Reserved for future revisions** (must NOT be emitted in v0): `fork`, `splice`.

### Artifact spillover rule

Any payload field whose serialized JSON exceeds **4 KiB** MUST be moved to `artifacts/` and replaced inline by a stub:

```json
{"ref": "sha:<blake3-hex>"}
```

The enclosing event's `refs` array MUST include `"sha:<blake3-hex>"` for every artifact it references. Implementations MUST verify hashes on read.

## `redactions.json`

```json
[
  {"step": 7, "field_path": "$.payload.response.body[0].text", "rule_id": "email", "replacement": "<EMAIL>"},
  ...
]
```

Replacement is always a **typed placeholder** (`<EMAIL>`, `<API_KEY:anthropic>`, `<JWT>`, `<SSN>`, `<CC>`, `<USER>`, `<HOST:internal>`, etc.). NEVER the original value, NEVER a hash of the original (hashes are reversible against small spaces like SSNs). For custom rules, replacement is `<CUSTOM:rule_id>`.

`field_path` is JSONPath into the JSON-serialized event. For string-position-within-payload redactions (the common case), the engine replaces the matched substring in-place; `field_path` points at the containing string field.

## Verify checklist (`tape verify`)

1. File is a valid zip.
2. `meta.yaml` exists, parses, has all required fields, `tape_version == "tape/v0"`.
3. `liner-notes.md` exists and contains all four required H2 sections.
4. `tracks.jsonl` exists, every line is valid JSON, `step` is contiguous from 1, `kind` is in the v0 enum, `ts` is ISO-8601.
5. Last event has `kind == "eject"` (or, for in-progress sentinel cassettes, document the exception in `meta.yaml.recorder.state`).
6. Every `refs` entry resolves: artifact file exists, blake3 of bytes matches the claimed hash.
7. No payload field exceeds 4 KiB inline (any oversize must be a `{ref: ...}` stub).
8. If `redactions.json` exists, `meta.yaml.redaction_summary` agrees on rules and count.
9. No string in `meta.yaml` or `liner-notes.md` matches any built-in redaction rule (defense-in-depth — these are surfaces that escape the redaction sweep most easily).

Verify exits 0 on valid, non-zero with a structured diagnostic per failure.
