---
description: List all .tape files reachable from the current directory, with their task and outcome at a glance.
---

Find every `.tape` file under the current working directory (recurse, but skip `node_modules`, `target`, `.git`, and `dist`). For each one, run `tape verify --json` to confirm validity and read the meta line. Then print one line per file:

```
<path>  <outcome>  <task>
```

If a tape doesn't verify, print:

```
<path>  INVALID    <first diagnostic code>
```

The `tape` binary is on PATH (provided by this plugin's `bin/`). Use the Bash tool, not the deck MCP — this is a filesystem scan, not a tape-content question.

End with a one-line suggestion: "Run `/tape-resume <path>` to load one." (Don't run it for them — they pick.)

If nothing is found, say so plainly and don't pad.
