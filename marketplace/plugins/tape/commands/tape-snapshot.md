---
description: Capture this Claude Code session's transcript as a .tape file. One-shot — reads the session JSONL, converts to v0 events, runs the eject pipeline.
argument-hint: <name>
---

The user wants to snapshot the current Claude Code session into a `.tape` file. The desired filename stem is `$ARGUMENTS`.

Do this:

1. **Resolve the output path.** If `$ARGUMENTS` is empty, ask the user for a name. Otherwise the output path is `<ARGUMENTS>.tape` in the current working directory (so e.g. `/tape:tape-snapshot bug-447` writes `./bug-447.tape`).
2. **Call `tape.snapshot`** with `out: "<resolved path>"`. The `task` argument is optional — pass it only if you can summarize the session in one line; if omitted, the tool derives `meta.task` from the first user prompt in the transcript.
3. **Report the result** in a single line: where it was written, how many tracks landed, how many redactions fired, and any non-zero `parse_warnings.malformed_lines` or `unknown_event_types`.
4. **Tell the user how to use it.** Suggest `tape ls <file>` from a shell to inspect, or `/tape:tape-resume <file>` from a fresh session to pick up.

## Rules

- **One round-trip**, no preliminary calls — `tape.snapshot` does discovery + parse + convert + eject internally.
- **Don't dump the full track list.** The user can `tape ls` it; you just confirm it landed.
- **If `tape.snapshot` returns an error**, surface the error code and message verbatim. The most common cause is no active session transcript (`TAPE_NOT_FOUND`) — that means Claude Code's session JSONL isn't where we expected it.
- **The snapshot captures from session start to now.** There's no `/clear` boundary detection; that's a known v0.1 limitation.
