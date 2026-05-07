---
name: tape-diff
description: tape diff design — three-pass algorithm (align via Needleman-Wunsch on step-intent embeddings, classify each pair, narrate via small judge model), text and JSON output schemas, classification taxonomy, and impact-tracing rules. Load when working in src/diff/.
---

# `tape diff`

`tape diff` compares two recordings — typically two attempts at the same task — and tells the user what changed and whether it mattered. The principle from the brief: **diff doesn't judge, it reports facts**. The judge model writes neutral narration; the user decides if the divergence is good.

## CLI

```
tape diff <a.tape> <b.tape> [--all] [--format text|json] [--judge MODEL]
```

- `--all` — emit every aligned pair, not just non-identical ones.
- `--format` — default `text`, machine-readable `json`. The MCP `tape.diff` tool returns `json`.
- `--judge MODEL` — the narration model. Default `claude-haiku-4-5`. Skip narration with `--judge none`.

## Three passes

### Pass 1 — alignment

Goal: pair each track in A with its semantic counterpart in B (or mark it unpaired). Runs are not the same length; Engineer B's agent may have skipped a step or added one.

Algorithm: **Needleman-Wunsch** on step-intent embedding similarity.

- For each track, compute a "step intent" string — a one-liner that captures the *purpose* of the step, not its exact bytes. Examples:
  - `model_call` → first user-message turn or system-prompt-derived intent + tool calls invoked.
  - `mcp_call` → `<server>.<tool>(<arg-summary>)`.
  - `shell` → first 80 chars of command.
  - `file_read` / `file_write` → `read(<path>)` / `write(<path>)`.
  - `task` → the prompt text.
  - `annotation` → the note text.
- Embed each intent string. Use the same backend as `tape.seek` (local model preferred, API embedding endpoint as fallback — the choice goes in `DECISIONS.md`).
- NW with substitution cost = `1 - cosine(emb_a, emb_b)`, gap cost = `0.6` (gaps are cheaper than substituting unrelated steps; tune empirically).
- Output: a list of `(a_step?, b_step?)` pairs in order. At most one of the two may be `None` (gap).

### Pass 2 — classification

For each aligned pair, assign a class:

| Class | Definition |
|---|---|
| `identical` | Bytes-equal payloads (after normalizing timestamps, request IDs, and other ephemera). |
| `cosmetic` | Differences are confined to whitespace, ordering of equivalent fields, or other non-semantic noise. |
| `substantive` | Semantically different but the change does not propagate downstream (no later step's behavior depends on the changed bytes). |
| `causal` | Substantive change that *does* propagate: a later step's content/decision is influenced by this difference. |
| `inserted` | Step exists only in B (a-side gap). |
| `deleted` | Step exists only in A (b-side gap). |

Detection rules:
- `identical` / `cosmetic`: byte compare after normalization. Normalizers strip: `id` fields, `ts` timestamps, `request_id` HTTP headers, list ordering when fields are commutative (e.g. JSON keys).
- `substantive` vs `causal`: build a coarse data-flow graph. A step's output "flows into" a later step if the later step's input contains a substring/hash of this step's output. If the changed bytes lie within a region that flows into a later step, mark `causal` and record the downstream step numbers.

### Pass 3 — narration

For each non-identical pair, send the judge model a small prompt:

```
You are diffing two AI agent runs of the same task. Stay neutral. Do not say which run is better.

Step intent (A): <intent>
Step intent (B): <intent>
A payload (truncated to 1KB): <...>
B payload (truncated to 1KB): <...>
Classification: <class>
Causal downstream steps: <list, if causal>

Write 1-2 sentences explaining what changed and (if causal) what flows from it. No judgment.
```

Cache narration by `(class, hash(a_payload), hash(b_payload))` so re-runs are free.

## Output — text format

```
Task:    "Investigate payment failures for customer 4471"
Outcome: success vs failure

▸ Track  3  cosmetic    · model_call: identical request, response paraphrased
    before: "I'll start by querying the payments table…"
    after:  "Let me query the payments table first…"
    why:    Different surface wording for the same plan.

▸ Track  7  causal      · mcp_call: db.query — different WHERE clause
    before: WHERE customer_id = 4471
    after:  WHERE customer_id = 4471 AND status = 'failed'
    why:    A narrowed the result set; B's broader query returned 1.2k rows where A's returned 3.
    impact: flows into Track 9 → 11  (B follows the larger result set down a different branch)

▸ Track  9  inserted    · annotation: only in B
    after:  "smoking gun: race condition here"
    why:    B added an annotation; no counterpart in A.

Final answers: divergent
Tool budget:   62 calls (A) · 84 calls (B)  (+35%)
Latency:       12,340 ms (A) · 18,910 ms (B)  (+53%)
```

Color: classification names colorized when stdout is a tty (cosmetic=dim, substantive=yellow, causal=red, inserted/deleted=blue). No color when piped or `--no-color`.

`--all` adds `identical` rows (otherwise omitted) so users can see the full alignment.

## Output — JSON format

```json
{
  "task": "...",
  "outcome": {"a": "success", "b": "failure"},
  "alignment": [
    {"a_step": 1, "b_step": 1, "class": "identical"},
    {"a_step": 2, "b_step": 2, "class": "cosmetic", "narration": "..."},
    {"a_step": 3, "b_step": 3, "class": "causal", "narration": "...", "downstream_b": [9, 11]},
    {"a_step": null, "b_step": 9, "class": "inserted", "narration": "..."}
  ],
  "summary": {
    "answers_equivalent": false,
    "tool_budget": {"a": 62, "b": 84, "delta_pct": 35},
    "latency_ms": {"a": 12340, "b": 18910, "delta_pct": 53}
  }
}
```

`tape.diff` MCP tool returns this verbatim.

## Performance

- Step-intent embedding: cache per-tape on first `tape diff` of that tape, persist alongside the tape (NOT inside the zip — to a sidecar `<tape>.embeddings`). Re-use across runs.
- NW is O(n*m); for tapes with >1000 steps, fall back to anchor-based alignment (find high-similarity anchors first, then NW within bands).
- Judge calls run in parallel with bounded concurrency (default 8).

## "Final answers" check

For the summary's `answers_equivalent` field: pull the last `model_call` response from each tape (or the last `annotation` if the recording agent labeled its conclusion that way), and ask the judge model: "are these substantively the same answer?" with a binary output. Cache aggressively.

## Out of scope for v0

- Multi-tape diff (>2 inputs).
- Time-travel diff (diff at intermediate steps, before either has run to completion).
- Custom alignment scorers via plugins.
