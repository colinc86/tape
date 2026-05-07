---
name: tape-redaction
description: Redaction engine reference — built-in rule list with patterns and entropy thresholds, two-pass eject behavior, .taperc schema, replacement-token format. Load when working in src/redact/, writing redaction tests, or implementing the eject confirmation prompt.
---

# Redaction engine

## Where it runs

Redaction runs **at eject time**, after artifact spillover, before the zip is written. It mutates a copy of `tracks.jsonl`, `meta.yaml`, `liner-notes.md`, and any text artifacts in-place. The originals are discarded — secrets MUST NOT survive in the temp dir past the redaction pass.

## Two passes

### Pass 1 — pattern-based, deterministic

Walk every string field in every track payload, plus `meta.yaml` and `liner-notes.md`. For each string, run all enabled rules in declaration order. Each match → replace in-place with the rule's replacement token, append an entry to `redactions.json`.

Rules are pure regex + optional post-validation (e.g. Luhn for credit cards). No LLM in pass 1 — it must be fast enough to run on every eject.

### Pass 2 — confirmation prompt

```
About to eject `my-bug.tape`:
  62 tracks, 47 redactions across 12 tracks
  Rules applied: email (31), anthropic_api_key (2), custom:pii_customer (14)

  [y] write tape    [n] cancel    [d] show diff    [e] edit redactions
```

- `y` → write the zip.
- `n` → abort, retain temp dir for inspection (but warn: temp dir will be auto-swept in 24h).
- `d` → render a side-by-side diff of redacted vs. original using `similar`. After viewing, return to the prompt.
- `e` → open the redactions list in `$EDITOR`; lines the user deletes are un-redacted (the original substring is restored). After save, re-prompt.

Non-interactive (stdin not a tty, or `--yes` passed): skip the prompt, write the zip.

## Built-in rules (each gets a stable `rule_id`)

| `rule_id` | Pattern (sketch) | Replacement | Default |
|---|---|---|---|
| `email` | RFC-5322 simplified: `[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}` | `<EMAIL>` | on |
| `anthropic_api_key` | `sk-ant-[A-Za-z0-9_-]{40,}` | `<API_KEY:anthropic>` | on |
| `openai_api_key` | `sk-[A-Za-z0-9]{20,}` (excl. `sk-ant-`) | `<API_KEY:openai>` | on |
| `aws_access_key` | `(AKIA\|ASIA)[0-9A-Z]{16}` | `<API_KEY:aws_access>` | on |
| `aws_secret_key` | high-entropy 40-char base64 within 50 bytes of `aws_secret`/`AWS_SECRET` context | `<API_KEY:aws_secret>` | on |
| `jwt` | `eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+` | `<JWT>` | on |
| `ssn` | `\b\d{3}-\d{2}-\d{4}\b` | `<SSN>` | on |
| `credit_card` | 13–19 contiguous digits, Luhn-validates | `<CC>` | on |
| `bearer_token` | `Bearer\s+[A-Za-z0-9._-]{20,}` | `<BEARER>` | on |
| `ipv4_private` | RFC-1918 ranges (`10.*`, `172.16-31.*`, `192.168.*`) | `<IP:private>` | **off** (opt-in) |
| `generic_high_entropy` | strings ≥32 chars, no whitespace, Shannon entropy ≥4.5 bits/char | `<SECRET>` | **off** (opt-in) |

Order matters: `anthropic_api_key` runs before `openai_api_key` so `sk-ant-…` doesn't first match the OpenAI rule.

## `.taperc` schema

```yaml
redact:
  # User-defined rules. id must be unique across all rules (built-in + custom).
  custom:
    - id: pii_customer
      pattern: 'CUST-\d{6}'
      replacement: '<CUST_ID>'   # optional, default <CUSTOM:pii_customer>
    - id: internal_host
      pattern: '[\w-]+\.internal\.justpark\.com'
  # Turn on built-in rules that default off.
  enable_optional: ["ipv4_private"]
  # Turn off built-in rules.
  disable_default: []
```

Resolution order:
1. Workspace `.taperc` (current dir → walk up to repo root).
2. User `~/.taperc`.
3. CLI flags: `--no-redact <id>`, `--enable-redact <id>`, `--redact-rule <id>=<pattern>` (one-shot).

Workspace `.taperc` overrides user `.taperc`. CLI overrides both.

## `redactions.json` entry format

```json
{
  "step": 7,
  "field_path": "$.payload.response.content[0].text",
  "rule_id": "email",
  "replacement": "<EMAIL>",
  "byte_range": [120, 137]
}
```

`byte_range` is into the field's string after JSON-decoding (i.e. the user's logical view), and is recorded so the diff view in the confirmation prompt can highlight precisely. `byte_range` is the only redactions.json field that's allowed to leak length info; this is acceptable because the redaction summary already does.

## Defense-in-depth

After pass 1 completes, run a final scan over `meta.yaml` and `liner-notes.md` against ALL built-in rules (even disabled ones). If any match: hard fail the eject with an error explaining which rule matched where. These two files are written by code we control, so a hit indicates a redaction-engine bug or a rule that needs to default-on.

## Test obligations

The brief mandates **≥5 positive and ≥5 negative cases per rule**. Suggested distribution per rule:

- 5 positive: canonical, with surrounding text, at start of string, at end of string, multiple-in-one-string.
- 5 negative: lookalike but invalid (e.g. wrong length, bad checksum), looks like the rule's pattern but is in a sanctioned context (allow-list TBD), low-entropy version, structurally adjacent (e.g. UUID for high-entropy).

Use `proptest` for cross-rule fuzzing: ensure no rule matches another rule's positive cases out of order.

## Performance budget

A 10 MB `tracks.jsonl` (typical large recording) MUST redact in <500ms on a developer laptop. Profile with `cargo flamegraph` if you regress past this. The hot loop is regex application across many short strings — use a single `regex::RegexSet` to dispatch all enabled rules in one pass, then run individual `Regex` only on rules that the set says matched.
