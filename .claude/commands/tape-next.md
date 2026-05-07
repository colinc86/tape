---
description: Identify the next unchecked step in the tape build order, plan it briefly, and start execution.
---

You are at the steering wheel of `tape` v0. Your job: pick the next concrete chunk of work and start moving on it.

## Process

1. **Read** the cursor and checklist in the `tape-build-order` skill, plus the Definition of Done in `TAPE_V0_BRIEF.md`.
2. **Identify** the lowest-numbered step that is not yet `[✓]`. That is the next step.
3. **Sanity-check** with the filesystem — does the step's exit criterion already appear to be met (file exists, test passes)? If yes, update the cursor and pick the next step.
4. **Briefly state the plan** in 3–6 lines: what the exit criterion is, what files will land, and what test will gate it.
5. **Start executing.** Do not ask permission; this command is the user telling you to keep moving. Use the appropriate skill for the area you're entering (`tape-format-v0` for step 1–4, `tape-record-pipeline` for 5–8, `tape-redaction` for 9, `tape-mcp-deck` for 11, `tape-diff` for 12).
6. When the step's exit criterion is met, update the cursor in the `tape-build-order` skill, run the `rust-builder` agent for a clean signal, and tell the user: "step N done; next is step N+1 — run `/tape-next` again to continue."

## Rules

- **Don't get ahead.** If step N+1 looks more interesting than step N, you must still finish step N first. The order is chosen so each step gives the next one easier ground.
- **One step per invocation.** Don't try to land 4 steps in a single `/tape-next`. Smaller increments → cleaner commits → easier rollback.
- **Decisions get logged.** Whenever you make a non-obvious choice (a crate selection, a tradeoff between two valid approaches, a deviation from the brief), use `/tape-decision` to log it before continuing.
- **If blocked**, say so — explicitly, with what you tried and what you'd need from the user (a credential, a clarification, a tool installation). Don't paper over with a workaround unless the brief's "make a defensible choice and document" rule applies.

## What "started executing" looks like

If step 1 (`SPEC.md`):
- Begin writing `SPEC.md`. Don't ask if you should — start.

If step 5 (Anthropic proxy):
- Scaffold `src/record/proxy/anthropic.rs`, add deps to `Cargo.toml`, write the smoke test, then implement.

If step 11 (`tape mcp`):
- Scaffold `src/mcp/server.rs`, register the 11 tools (read-only first, write after), write the protocol smoke test that the `mcp-deck-tester` agent will later exercise.

You don't need to finish in this turn — but you must make tangible progress and end with a clear "next time, do X" handoff.
