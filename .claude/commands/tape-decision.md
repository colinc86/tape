---
description: Append an entry to DECISIONS.md. Usage `/tape-decision <title>` — title is the headline; you'll prompt for the rest if not enough context is in scope.
argument-hint: <short title of the decision>
---

The user is logging a non-obvious build decision. The brief is explicit: "make the most defensible choice and document it in `DECISIONS.md`." Your job is to write a clean entry.

## Title

If `$ARGUMENTS` is non-empty, use it as the entry headline. Otherwise ask the user for a one-line headline.

## Entry shape

Append to `DECISIONS.md` (create the file if it doesn't exist; lead with a one-paragraph header explaining what this file is for).

Each entry follows this format:

```markdown
## D<N>. <Title>

**Date:** <ISO date>
**Step:** <build-order step or "general">
**Status:** decided | revisit-when-<condition>

### Context
<1-3 sentences. What problem prompted the choice. Cite the brief if relevant.>

### Options considered
- **A.** <option> — <pros/cons in one line each>
- **B.** <option> — <pros/cons>
- **C.** <option> — <pros/cons>

### Choice
<The chosen option, one line.>

### Why
<2-4 sentences. The actual reasoning. Cite constraints (Claude Code compatibility, streaming requirement, vendor-neutrality, license, etc.). If the choice is reversible, say so.>

### Revisit if
<A specific condition that should make us reopen this. e.g. "if rmcp gains stable streaming-tools support, revisit our hand-rolled MCP server.">
```

## Process

1. Read existing `DECISIONS.md` to find the next `D<N>` number. If absent, start at `D1` and write the file header.
2. Gather context:
   - The headline (from `$ARGUMENTS` or prompt).
   - Which build-order step is in flight (check `tape-build-order` skill cursor).
   - The options, choice, reasoning. Use what's already in conversation context if obvious; ask only if you'd be guessing.
3. Append the entry. Do NOT rewrite existing entries.
4. Confirm with the user: "Logged D<N>. Proceeding."

## File header (when creating DECISIONS.md)

```markdown
# DECISIONS.md

Non-obvious build decisions for `tape` v0. The brief mandates this file: when the brief leaves a question open and we make a defensible choice, that choice plus its rationale lives here.

Entries are append-only. Status `revisit-when-<condition>` flags decisions that should be re-examined later; status `decided` is final unless explicitly reopened.
```

## Rules

- **Don't pad.** A 100-line decision entry is not better than a 30-line one. Be specific, then stop.
- **Cite the brief** when the brief is the constraint that drove the choice ("brief mandates streaming non-buffered, eliminating option B").
- **Don't editorialize about other tools.** "vcrpy is bad" is not a reason. "vcrpy records HTTP calls; we record agent runs — different unit" is.
- **No emojis** in entries.
