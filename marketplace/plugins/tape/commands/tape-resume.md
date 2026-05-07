---
description: Load a .tape file via the deck MCP and pick up where the prior agent left off — the killer-scenario flow.
argument-hint: <path-to-tape-file>
---

The user wants to resume work captured in a `.tape` file. The file path is `$ARGUMENTS`.

Do this in order:

1. **Verify the path.** If `$ARGUMENTS` is empty, ask the user for the path. If the file doesn't exist, say so and stop.
2. **Load the tape.** Call `tape.load` with `path: "<the path>"`. This returns a handle plus a quick summary.
3. **Read the liner notes.** Call `tape.summary` with the handle. The liner notes are the cassette's case insert — they tell you what the prior agent was working on, what they found, and what they thought the next step was.
4. **Identify the smoking gun.** If the prior agent pinned an annotation marking a key insight (use `tape.seek` for terms like "smoking gun", "root cause", "key", or whatever the liner notes hint at), retrieve it with `tape.play`.
5. **Tell the user what you have.** A 3–5 line summary, ending with the prior agent's suggested next step verbatim.
6. **Ask for direction.** Should you continue from where they left off, fork at a specific step, or interrogate the tape further?

## Rules

- The handle is **session-local** to this Claude Code session. If the user runs `/tape-resume` again later they'll get a different handle.
- **Don't dump the full tape**. The format is designed so an agent pulls slices on demand. Use `tape.tracks` to see what's there, then `tape.play` only on the steps that matter for the current question.
- If the tape's `outcome` was `failure` or `abandoned`, lead with that — the user is probably picking up an unfinished investigation.
- If `tape.seek` returns nothing relevant, fall back to `tape.tracks --kind annotation` and reading those.
