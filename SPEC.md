# `tape/v0` — Format Specification

**Status:** Draft 1 · Vendor-neutral · Apache 2.0

This document specifies the `tape/v0` file format. Any tool that reads or writes a `.tape` file MUST conform to this specification. The reference implementation lives in this repository at `crates/tape-format`.

The keywords MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT, RECOMMENDED, MAY, and OPTIONAL are to be interpreted as in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

---

## 1. Concepts

### 1.1 The unit of work is the run

A `.tape` captures a coherent **trajectory** — a single agent run that begins with a task, does work (model calls, tool invocations, file edits, shell commands, decisions), and ends in an outcome. It is NOT a log of API calls. Two API calls belonging to two unrelated tasks MUST NOT live in the same tape.

### 1.2 Vendor neutrality

The format encodes the *shape* of an agent run, not the surface of any particular vendor. Anthropic and OpenAI requests are recorded with the same `model_call` event kind. MCP traffic is recorded with the same `mcp_call` event kind regardless of which MCP server produced it.

### 1.3 Read-only by default

A reader of a tape never mutates it. Mutation is reserved for explicit operations (`fork`, `splice`, re-eject), which produce a new tape with a new identity rather than editing in place.

---

## 2. File container

A `.tape` file is a **ZIP archive**, conforming to the [PKWARE APPNOTE](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT) standard. Implementations MUST read it as a zip; the `.tape` extension is a convention, not a content discriminator.

### 2.1 Required entries

```
<archive root>/
├── meta.yaml          (REQUIRED)
├── liner-notes.md     (REQUIRED)
├── tracks.jsonl       (REQUIRED)
├── artifacts/         (OPTIONAL — present iff any track refs an artifact)
│   └── <aa>/<bb>/<full-hash>.bin
└── redactions.json    (REQUIRED iff any redactions occurred)
```

Entries other than the above are RESERVED. Implementations MUST ignore unrecognized entries on read but MUST NOT emit them on write.

### 2.2 Compression

Entries SHOULD use `DEFLATE` compression. Implementations MUST also accept `STORED` (uncompressed). Other compression methods (e.g. `BZIP2`, `LZMA`) MUST NOT be used in `tape/v0`; implementations MAY reject tapes that contain them.

### 2.3 Encoding

All textual entries (`meta.yaml`, `liner-notes.md`, `tracks.jsonl`, `redactions.json`) MUST be UTF-8 encoded with no BOM. Line endings MUST be `LF` (`\n`); `CRLF` is invalid.

### 2.4 Path separator

ZIP entry names MUST use forward slashes (`/`) regardless of host OS.

---

## 3. `meta.yaml`

A YAML 1.2 document containing recording metadata.

### 3.1 Required fields

| Field | Type | Description |
|---|---|---|
| `tape_version` | string | MUST be the literal string `"tape/v0"` for this revision |
| `id` | string | UUIDv7, lowercase hex with hyphens, e.g. `01h8xy...-...-...-...-...` |
| `created_at` | string | ISO-8601 timestamp of when recording started, with timezone (`Z` for UTC required) |
| `ejected_at` | string | ISO-8601 timestamp of when the file was written |
| `task` | string | A one-line human-readable description of what the agent was asked to do |
| `recorder.agent` | string | Identifier of the recording agent, e.g. `"claude-code/2.1.4"` |
| `outcome` | enum | One of `success`, `failure`, `abandoned`, `unknown` |

`created_at` MUST be lexicographically ≤ `ejected_at`.

### 3.2 Optional fields

```yaml
recorder:
  agent: "claude-code/2.1.4"     # required
  user: "<USER>"                  # OPTIONAL — by default redacted to <USER>
models:                            # OPTIONAL — summary; full data is in tracks
  - vendor: anthropic              #   vendor MUST be lowercase
    model: claude-opus-4-7
    calls: 47
tools:                             # OPTIONAL — summary
  - kind: mcp                      #   one of: mcp, builtin
    server: "filesystem"           #   MCP: server name; builtin: tool name
    calls: 12
tool_budget:                       # OPTIONAL — summary
  total_calls: 62
  total_tokens_in: 145203
  total_tokens_out: 8432
  wall_clock_ms: 12340
redaction_summary:                 # REQUIRED iff redactions.json exists
  rules_applied: ["email", "anthropic_api_key", "custom:pii"]
  redaction_count: 47
label: "investigating-payments-bug"  # OPTIONAL — caller-supplied tag for
                                     # filing / categorising cassettes.
                                     # `tape record --label X` writes it.
tags:                                # OPTIONAL — multi-valued facet labels.
  - bug-fix                          #   Each entry MUST be a non-empty
  - auth                             #   string. Recommended convention is
  - regression-baseline              #   lowercase kebab-case (not enforced
                                     #   in this revision). Empty list and
                                     #   field-absent are equivalent; writers
                                     #   SHOULD omit the field when the list
                                     #   would be empty. Distinct from
                                     #   `label` — tags compose, label is a
                                     #   single grouping string.
                                     #   `tape tag --add` / `--remove`
                                     #   manages this list post-hoc.
```

If `redaction_summary` is present, `redactions.json` MUST exist, and the count and rule list MUST agree (same set of rule_ids; total entry count equals `redaction_count`).

`tags` is a SET in list shape: the on-disk representation MUST NOT contain duplicate entries. Writers SHOULD preserve insertion order for diff legibility; readers MUST NOT assume any specific order. Length / count caps and a stable diagnostic-code surface for malformed `tags` arrive in a follow-up to this revision.

### 3.3 Forbidden content

`meta.yaml` MUST NOT contain any string that matches a built-in redaction rule (see §7). This is a defense-in-depth check; a hit indicates a redaction-engine bug. Validators MUST reject tapes that fail this check.

---

## 4. `liner-notes.md`

A Markdown document, ≥1 byte, recommended length 200–500 words. It is the first thing a receiving agent should read; it serves as the cassette's case insert.

### 4.1 Required structure

The document MUST contain, in order, these four level-2 headings:

```markdown
## What I was asked to do
## What I found
## Suggested next step / fix
## What I'm uncertain about
```

Each heading MUST be followed by at least one non-empty paragraph or list item before the next heading. Empty sections are invalid.

### 4.2 Stub liner notes

When a recording exits abnormally (non-zero child exit, signal kill), an implementation MAY write stub liner notes that say only what is mechanically known (e.g. "Recording exited abnormally with exit code 137 after 12 model calls"). Stubs MUST still contain all four required H2 sections.

### 4.3 Forbidden content

Same as §3.3 — no string in `liner-notes.md` may match a built-in redaction rule. Validators MUST reject tapes that fail this check.

---

## 5. `tracks.jsonl`

A line-delimited JSON document. Each line is one event. Events are ordered.

### 5.1 Line format

Each line MUST be:

- A complete UTF-8 JSON object terminated by exactly one `\n`.
- The final line MUST also end with `\n`.
- No trailing commas, no comments, no streaming markers.

Empty lines and whitespace-only lines MUST NOT appear.

### 5.2 Required event fields

Every event MUST contain:

| Key | Type | Description |
|---|---|---|
| `step` | integer ≥ 1 | The 1-based ordinal of this event in the tape |
| `kind` | string | One of the v0 event kinds (§5.4); MUST be from the closed set |
| `ts` | string | ISO-8601 timestamp, MUST be ≥ the previous event's `ts` |
| `payload` | object | Kind-specific shape (§5.5) |

`step` values MUST start at 1 and increase by exactly 1 each line. Gaps are invalid.

### 5.3 Optional event fields

| Key | Type | Description |
|---|---|---|
| `parent_step` | integer | Step number of the event that caused this one; MUST be < `step` |
| `refs` | array of `"sha:<blake3-hex>"` | Artifacts referenced from this event's payload |
| `annotations` | array of `{by, note}` | Inline notes attached to this event |

If `parent_step` is present, the referenced step MUST exist in the same tape.

### 5.4 v0 event kinds (closed set)

| `kind` | Source | Required |
|---|---|---|
| `task` | Injected at the start of every recording | Exactly one, MUST be `step: 1` |
| `model_call` | API proxy | Zero or more |
| `mcp_call` | MCP wrapper | Zero or more |
| `shell` | Claude Code `Bash` hook | Zero or more |
| `file_read` | Claude Code `Read` hook | Zero or more |
| `file_write` | Claude Code `Write`/`Edit` hook | Zero or more |
| `annotation` | Recording agent or human | Zero or more |
| `eject` | Always last event | Exactly one, MUST be the final line |

Any other `kind` value renders the tape invalid for `tape/v0`. Future revisions reserve the kinds `fork` and `splice`; v0 readers MUST reject tapes containing them.

### 5.5 Payload shapes

#### 5.5.1 `task`

```json
{"prompt": "<string>"}
```

Prompt MUST be non-empty. Additional fields are allowed but RESERVED; v0 implementations SHOULD ignore unknown fields.

#### 5.5.2 `model_call`

```json
{
  "vendor": "anthropic" | "openai" | "<other-lowercase>",
  "model": "<model-id>",
  "request": { /* vendor-shape request body */ },
  "response": { /* vendor-shape response body */ },
  "stream_chunks": <integer>,        // OPTIONAL — number of SSE chunks streamed
  "duration_ms": <integer>,          // OPTIONAL
  "tokens_in": <integer>,            // OPTIONAL
  "tokens_out": <integer>,           // OPTIONAL
  "error": { "code": "<string>", "message": "<string>" }   // OPTIONAL — present on failure
}
```

`request` and `response` are stored verbatim as the vendor returned them, except for any redaction substitutions. The format does not interpret them.

#### 5.5.3 `mcp_call`

```json
{
  "server": "<server-name>",
  "tool": "<tool-name>",
  "args": { /* JSON-RPC params for tools/call */ },
  "result": { /* tools/call result */ },
  "error": { "code": "<string>", "message": "<string>" },  // OPTIONAL
  "duration_ms": <integer>                                  // OPTIONAL
}
```

#### 5.5.4 `shell`

```json
{
  "command": "<string>",
  "exit_code": <integer>,
  "stdout": "<string>",
  "stderr": "<string>",
  "duration_ms": <integer>
}
```

#### 5.5.5 `file_read`

```json
{
  "path": "<string>",
  "byte_range": [<int>, <int>],   // OPTIONAL, half-open [start, end)
  "content_hash": "blake3:<hex>"
}
```

#### 5.5.6 `file_write`

```json
{
  "path": "<string>",
  "before_hash": "blake3:<hex>" | null,   // null iff file did not exist before
  "after_hash":  "blake3:<hex>",
  "diff": "<unified-diff-string>"          // OPTIONAL
}
```

#### 5.5.7 `annotation`

```json
{
  "by": "agent" | "human",
  "note": "<string>"
}
```

#### 5.5.8 `eject`

```json
{
  "outcome": "success" | "failure" | "abandoned" | "unknown"
}
```

The outcome here MUST equal `meta.yaml.outcome`.

### 5.6 Artifact spillover

Any payload field whose serialized JSON encoding (the field's value, not the whole event) exceeds **4096 bytes** MUST be moved to the `artifacts/` directory and replaced inline by a stub:

```json
{"ref": "sha:<blake3-hex>"}
```

"Field" includes any JSON value reachable from the payload — strings, objects, arrays, numbers. For string fields the encoded length includes the surrounding quotes and any required escapes; for object and array fields it is the value's complete canonical JSON serialization. The top-level payload wrapper is itself not eligible for spillover; only the values reachable beneath it are.

When an object or array field exceeds the threshold as a whole, the entire subtree MUST be spilled as one artifact — implementations MUST NOT spill the parent's children individually in addition to the parent. The artifact bytes for a string field are the raw UTF-8 bytes of the string (without surrounding quotes); for other types they are the value's canonical JSON encoding.

The enclosing event's `refs` array MUST include `"sha:<blake3-hex>"` for every artifact it references. Implementations MUST verify on read that:

1. Each `refs` entry corresponds to an actual artifact file.
2. The blake3 digest of the artifact's bytes matches the claimed hash.

#### 5.6.1 Artifact path layout

For an artifact with full hash `aabbccddeeff…`, the entry path is:

```
artifacts/aa/bb/aabbccddeeff….bin
```

Where `aa` is the first two hex characters and `bb` is the next two. The full hash forms the basename. The `.bin` suffix is mandatory.

#### 5.6.2 Artifact bytes

Artifact contents are stored as raw bytes. Implementations MUST NOT transform (re-encode, compress, prettify) artifact content. The hash is taken over the exact bytes in the file.

#### 5.6.3 Multi-reference

The same artifact MAY be referenced from multiple events. Storage is content-addressed; implementations MUST deduplicate.

---

## 6. `redactions.json`

A JSON array of redaction records. Each record describes one substitution made during the eject pipeline.

### 6.1 Schema

```json
[
  {
    "step": <integer>,
    "field_path": "<JSONPath>",
    "rule_id": "<string>",
    "replacement": "<string>",
    "byte_range": [<int>, <int>]    // OPTIONAL
  }
]
```

| Field | Type | Description |
|---|---|---|
| `step` | integer ≥ 1 | The track step in which the redaction occurred. Use `0` for redactions in `meta.yaml` (defense-in-depth catches; should not occur in valid tapes) |
| `field_path` | string | JSONPath into the JSON-serialized event payload pointing at the containing string field |
| `rule_id` | string | Stable identifier of the rule that fired (built-in id or `custom:<id>`) |
| `replacement` | string | The typed placeholder (`<EMAIL>`, `<API_KEY:anthropic>`, etc.) substituted in place of the original |
| `byte_range` | array | OPTIONAL; `[start, end)` byte offsets within the field's UTF-8 string content where the original substring was located |

### 6.2 Replacement constraints

The `replacement` string MUST:

- Be a typed placeholder of the form `<TYPE>` or `<TYPE:subtype>`.
- NOT be the original value.
- NOT be a hash, encoding, or otherwise reversible function of the original value (hashes are reversible against small spaces like SSNs).

### 6.3 Ordering

Records SHOULD appear in the order their substitutions were applied, which is determined first by `step` ascending, then by `byte_range[0]` ascending within a step.

---

## 7. Built-in redaction rules

The following rule_ids are reserved for built-in rules. A rule's exact regex pattern is implementation-defined within constraints; the key is that any string the brief's pattern would match MUST be redacted.

| `rule_id` | Matches | Replacement | Default |
|---|---|---|---|
| `email` | RFC-5322-style email addresses | `<EMAIL>` | enabled |
| `anthropic_api_key` | `sk-ant-` followed by ≥40 chars of `[A-Za-z0-9_-]` | `<API_KEY:anthropic>` | enabled |
| `openai_api_key` | `sk-` followed by ≥20 chars of `[A-Za-z0-9]` (excluding `sk-ant-` prefix) | `<API_KEY:openai>` | enabled |
| `aws_access_key` | `AKIA` or `ASIA` followed by 16 chars of `[0-9A-Z]` | `<API_KEY:aws_access>` | enabled |
| `aws_secret_key` | 40-char base64 string within 50 bytes of `aws_secret`/`AWS_SECRET` context | `<API_KEY:aws_secret>` | enabled |
| `jwt` | `eyJ` followed by two more base64url segments separated by `.` | `<JWT>` | enabled |
| `ssn` | `\b\d{3}-\d{2}-\d{4}\b` | `<SSN>` | enabled |
| `credit_card` | 13–19 contiguous digits passing Luhn check | `<CC>` | enabled |
| `bearer_token` | `Bearer ` followed by ≥20 chars of `[A-Za-z0-9._-]` | `<BEARER>` | enabled |
| `ipv4_private` | RFC-1918 ranges | `<IP:private>` | disabled (opt-in) |
| `generic_high_entropy` | strings ≥32 chars with no whitespace and Shannon entropy ≥4.5 bits/char | `<SECRET>` | disabled (opt-in) |

Rule application order: `anthropic_api_key` MUST run before `openai_api_key` (else `sk-ant-…` matches as OpenAI). All other rules MAY run in any order.

### 7.1 Custom rules

Users MAY define additional rules in a `.taperc` file (see §9). Custom rule_ids MUST be unique across the union of built-in and custom rules. Custom replacement defaults to `<CUSTOM:<rule_id>>` if not specified.

---

## 8. Eject pipeline

This section is normative for *writers*. Readers do not run an eject pipeline.

When a recording is finalized into a `.tape` file, the writer MUST execute these stages in order:

1. **Stop accepting events.** Drain in-flight events.
2. **Resolve oversized payloads.** For every event, walk the payload tree; for any field exceeding 4096 bytes when serialized, write to `artifacts/` and replace the inline value with a `{"ref": "sha:..."}` stub. Add to the event's `refs`.
3. **Liner notes.** If the recording exited normally, generate liner notes (typically by asking the recording agent's last model). Otherwise write a stub.
4. **Redaction Pass 1.** Walk every string field in every event payload, plus `meta.yaml` and `liner-notes.md`. Apply enabled rules. Record each substitution in the in-memory redactions list.
5. **Redaction Pass 2.** If interactive (stdin is a TTY and `--yes` not supplied), present a confirmation prompt summarizing redactions. Allow the user to: write, cancel, view diff, or edit the redaction list (un-redacting selected entries).
6. **Defense-in-depth scan.** After redaction completes, scan `meta.yaml` and `liner-notes.md` against ALL built-in rules (including disabled ones). Any match aborts the eject with a structured error.
7. **Write the zip.** Compose the entries in the layout of §2.1. Write to a temp path and atomically rename to the final destination.
8. **Cleanup.** Remove the temp recording directory, sockets, MCP overlay, and Claude Code settings overlay.

### 8.1 Atomicity

The final rename MUST be atomic on the target filesystem. On platforms where atomic rename is unavailable, the writer MUST fail with a structured error rather than risk a partial file appearing as a valid tape.

---

## 9. `.taperc` configuration file

YAML, optional. Located at one of:

1. The current working directory at recording time, or any ancestor up to the user's home directory.
2. `$HOME/.taperc`.

CWD-rooted config takes precedence over user config. CLI flags take precedence over both.

### 9.1 Schema

```yaml
redact:
  custom:
    - id: <string>                # MUST be unique across built-in + custom
      pattern: '<regex>'          # required, Rust regex syntax
      replacement: '<string>'     # OPTIONAL; defaults to '<CUSTOM:<id>>'
  enable_optional: ["<rule_id>"]  # rules listed in §7 with default disabled
  disable_default: ["<rule_id>"]  # rules listed in §7 with default enabled
```

Unknown keys under `redact:` MUST cause a config-load failure (typo prevention). Unknown top-level keys are ignored (forward-compat).

---

## 10. Validation (`tape verify`)

Conforming readers SHOULD provide a verification operation. The reference implementation is `tape verify <file>`. A tape is **valid** if all of the following hold; any failure renders it **invalid**.

### 10.1 Structural checks

1. The file is a readable ZIP archive (§2.2 compression rules).
2. Required entries `meta.yaml`, `liner-notes.md`, `tracks.jsonl` are present.
3. `redactions.json` is present iff `meta.yaml.redaction_summary` is present.
4. No reserved or unrecognized entries are emitted (warning, not failure).

### 10.2 Schema checks

5. `meta.yaml` parses as YAML 1.2 and contains all fields required by §3.1.
6. `tape_version` equals `"tape/v0"`.
7. `liner-notes.md` parses as Markdown and contains all four required H2 sections (§4.1) with non-empty bodies.
8. Every line of `tracks.jsonl` is valid JSON; `step` starts at 1, increases by exactly 1, and `kind` is in the closed set (§5.4).
9. The first event has `kind == "task"` and `step == 1`. The last event has `kind == "eject"`.
10. Timestamps are ISO-8601 with timezone, monotonically non-decreasing.

### 10.3 Reference checks

11. Every `refs` entry resolves to an artifact file at the path defined by §5.6.1.
12. The blake3 digest of each artifact's bytes equals the hash claimed in the `refs` entry.
13. No payload field exceeds 4096 bytes inline (such fields must instead be `{ref: ...}` stubs).

### 10.4 Consistency checks

14. `meta.yaml.outcome` equals the `outcome` in the `eject` event.
15. If `redaction_summary` is present, the `rules_applied` set and `redaction_count` agree with `redactions.json` exactly.

### 10.5 Defense-in-depth checks

16. No string in `meta.yaml` matches any built-in redaction rule.
17. No string in `liner-notes.md` matches any built-in redaction rule.

### 10.6 Diagnostic codes

Validators that fail SHOULD emit one or more structured diagnostics with these stable codes.

**Errors** (cause the tape to fail validation):

`MALFORMED_ZIP`, `MISSING_REQUIRED_ENTRY`, `INVALID_META_YAML`, `WRONG_TAPE_VERSION`, `INVALID_LINER_NOTES`, `MISSING_LINER_SECTION`, `LINER_SECTIONS_OUT_OF_ORDER`, `INVALID_TRACKS_JSON`, `STEP_GAP`, `UNKNOWN_KIND`, `RESERVED_KIND`, `MISSING_TASK_EVENT`, `MISSING_EJECT_EVENT`, `EJECT_NOT_LAST`, `BAD_TIMESTAMP`, `TS_NOT_MONOTONIC`, `INVALID_PAYLOAD`, `INVALID_PARENT_STEP`, `MISSING_ARTIFACT`, `ARTIFACT_HASH_MISMATCH`, `OVERSIZED_INLINE_PAYLOAD`, `OUTCOME_MISMATCH`, `REDACTION_SUMMARY_MISMATCH`, `LEAKED_SECRET_IN_META`, `LEAKED_SECRET_IN_LINER`.

**Warnings** (surfaced for the user but the tape still passes):

`UNKNOWN_ENTRY`.

---

## 11. Versioning and forward compatibility

This document specifies `tape/v0`. Future revisions will be `tape/v1`, `tape/v2`, etc. — major-version-only; the format does not use minor versions.

A `tape/v0` reader MUST reject tapes whose `tape_version` is anything other than `"tape/v0"`. There is no automatic forward compatibility within a major version.

A `tape/v0` reader SHOULD ignore unknown optional fields within objects it understands (e.g. an unknown key within a `model_call` payload), provided this does not violate any explicit rule above.

The event kinds `fork` and `splice` are RESERVED for future use; they MUST NOT appear in `tape/v0` files.

---

## 12. Security considerations

### 12.1 Secrets

Tapes inherently contain potentially sensitive material: prompts, model responses, file contents, shell output. The redaction pipeline (§8) is the primary mitigation, but it is best-effort. Users sharing tapes externally are responsible for verifying no sensitive content remains.

### 12.2 Path traversal in artifacts

ZIP entries with paths containing `..` or absolute paths MUST be rejected by readers. Artifact paths MUST conform exactly to §5.6.1.

### 12.3 Decompression bombs

Readers SHOULD enforce a maximum decompressed size proportional to the input. The reference implementation rejects any tape that decompresses to more than 100× its compressed size.

### 12.4 Hash verification

Artifact integrity (§10.3) is verified by blake3, which provides collision and second-preimage resistance sufficient for this use case. Readers MUST NOT skip hash verification on the assumption that the producer is trusted.

---

## Appendix A. Example minimal tape

```
minimal-success.tape (zip)
├── meta.yaml
├── liner-notes.md
└── tracks.jsonl
```

`meta.yaml`:

```yaml
tape_version: "tape/v0"
id: "01h8xy00-0000-7000-8000-000000000001"
created_at: "2026-05-06T10:00:00Z"
ejected_at: "2026-05-06T10:00:30Z"
task: "Say hello"
recorder:
  agent: "claude-code/2.1.4"
outcome: success
```

`liner-notes.md`:

```markdown
## What I was asked to do
Say hello.

## What I found
The greeting was produced.

## Suggested next step / fix
None — task completed.

## What I'm uncertain about
Nothing.
```

`tracks.jsonl`:

```jsonl
{"step":1,"kind":"task","ts":"2026-05-06T10:00:00Z","payload":{"prompt":"Say hello"}}
{"step":2,"kind":"model_call","ts":"2026-05-06T10:00:15Z","payload":{"vendor":"anthropic","model":"claude-opus-4-7","request":{"messages":[{"role":"user","content":"Say hello"}]},"response":{"content":[{"type":"text","text":"Hello!"}]}}}
{"step":3,"kind":"eject","ts":"2026-05-06T10:00:30Z","payload":{"outcome":"success"}}
```

---

## Appendix B. Reserved future surface

The following are explicitly reserved and MUST NOT appear in `tape/v0` writes:

- Top-level zip entries other than those listed in §2.1.
- Event kinds other than those listed in §5.4.
- `tape_version` values other than `"tape/v0"`.
- ZIP compression methods other than `STORED` and `DEFLATE`.
- Encryption (the format does not specify an encryption layer; encrypted tapes are out of scope for `v0`).

---

*End of `tape/v0` specification.*
