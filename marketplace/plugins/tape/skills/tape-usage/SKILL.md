---
name: tape-usage
description: How to use the tape MCP deck effectively. Load when the user asks to load, search, summarize, or fork a .tape file; or whenever you're calling any tape.* MCP tool. Encodes the handle-not-contents rule and the right call ordering for the killer-scenario flow.
---

# How to use the `tape` deck

This plugin exposes a `tape` MCP server providing 12 tools for working with `.tape` files (cassettes capturing agent runs). When the user references a `.tape` file or asks you to "pick up" a prior session, follow these patterns.

## The handle-not-contents rule

`tape.load` returns a **handle** (e.g. `tape:00000001`), not the file's contents. A 50 MB tape coexists with a 200 K context window because you only pull slices on demand.

**Always:**
1. `tape.load` first. Save the handle.
2. `tape.summary` next — cheap, returns meta + liner notes.
3. Then targeted: `tape.tracks` to see what's there, `tape.play` for full payloads, `tape.seek` for substring search.

**Never** try to dump the whole tape into context.

## The 12 tools

Read-only:
- `tape.load(path)` → `{handle, summary}` — mount and verify a tape
- `tape.summary(handle)` → meta + liner-notes + track count + kind histogram
- `tape.tracks(handle, [kind, range, regex])` → lightweight listing (step + kind + label)
- `tape.play(handle, step | range)` → full payloads for selected tracks (200 KB cap)
- `tape.seek(handle, query, [k])` → substring search across track payloads, top-k hits
- `tape.tools(handle, [server, tool])` → just `mcp_call` tracks
- `tape.diff(a_handle, b_handle, [all])` → JSON diff between two loaded tapes

Mutating (writes to disk, or session-local until `tape.eject`):
- `tape.fork(handle, from_step, [label])` → new handle truncated at `from_step`
- `tape.record(task)` → start a new in-memory recording (the agent constructs it via `tape.annotate`)
- `tape.annotate(handle, note, [step, by])` → pin a note
- `tape.eject(handle, out)` → write a recording / fork to disk
- **`tape.snapshot(out, [task], [transcript_path])`** *(v0.1)* — capture the active Claude Code session's transcript as a `.tape` file in one round-trip.

## Common flows

### Flow A — Resume a prior investigation (killer scenario)

```
1. tape.load(path)                 → handle, summary
2. tape.summary(handle)            → read the liner notes
3. tape.seek(handle, "smoking gun", k=5)
                                   → find the prior agent's pinned insight
4. tape.play(handle, step=<hit>)   → full annotation payload
5. (synthesize answer for the user)
```

### Flow B — Find what tools the prior agent used

```
1. tape.load(path)                 → handle
2. tape.tools(handle)              → just the mcp_call entries
3. (group by server.tool, summarize call counts)
```

### Flow C — Fork a tape to try a different approach

```
1. tape.load(path)                 → original_handle
2. tape.fork(original_handle, from_step=N)
                                   → new_handle (truncated to step N)
3. tape.annotate(new_handle, "trying alternative path here")
4. (continue work; eventually tape.eject if you want to save)
```

## Outcome semantics

`meta.outcome` is one of `success | failure | abandoned | unknown`. When loading a `failure` or `abandoned` tape, lead with that fact — the user is picking up unfinished work.

## When `tape.seek` misses

Fall back to:
1. `tape.tracks(handle, kind="annotation")` — agent-pinned notes
2. `tape.tracks(handle, kind="model_call")` — recent model turns
3. Walk `tape.summary`'s `kinds` histogram to see what's there

## What this plugin does NOT do

- The deck does not auto-load tapes. The user explicitly hands one off via `/tape:tape-resume <path>`, or asks you to load one.
- Forks and `tape.record` in-memory recordings created via the deck are session-local and disappear when this Claude Code session ends — unless `tape.eject` saves them to disk.

## Three ways to record

There are three recording paths, each suited to different situations:

1. **`tape.snapshot` (v0.1, in-session, recommended for most cases)** — captures the active Claude Code session's transcript. One round-trip. The user runs `/tape:tape-snapshot <name>` and gets a `.tape` containing everything from session start to now. No setup, no extra shell, no API key needed.

2. **`tape record -- claude <args>` (CLI, high-fidelity)** — proxies the model API as a parent process. Higher fidelity than snapshot (raw streaming bodies, exact chunk timing). Right when you need the full network record or are doing a non-interactive `claude -p "..."` run.

3. **`tape.record` + `tape.annotate` + `tape.eject` (in-memory, scripted)** — the deck-side mutating tools. The agent builds a small synthetic tape from MCP-side notes. Useful for scripted scenarios where the agent wants to package up a few annotations without referencing the larger session.

Default to #1 unless you specifically need fidelity (#2) or are scripting (#3).
