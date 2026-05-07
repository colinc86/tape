---
description: Show the user how to start a recording. Recording happens outside Claude Code, from a separate shell.
---

Print this verbatim — don't paraphrase, don't add extra commentary:

```
Recording happens from a separate shell, not inside this Claude Code session.

  tape record --task "<one-line description>" --out <name>.tape -- claude <args>

Example — investigate a bug, save the trajectory:

  tape record --task "find why payments fail for customer 4471" \
              --out bug-447.tape \
              -- claude "look at src/payments.rs and the recent failures in /var/log/payments.log"

When the child claude exits, the .tape file is written to the current directory.
Verify it:        tape verify bug-447.tape
Inspect it:       tape ls bug-447.tape
Hand it to me:    /tape-resume bug-447.tape
```

After printing the block, ask: "Do you want to record now? I can start the command in a new terminal if you want; otherwise, take the snippet and run it yourself."

Don't try to invoke `tape record` from inside this session — recording wraps a child process and would conflict with the current Claude Code session you're already in.
