---
description: Run the killer-scenario integration test — Engineer A produces a tape, Engineer B picks up via the deck MCP and answers a question that requires reading specific tracks. The single demo that must pass for v0 to ship.
---

Run the killer-scenario integration test. The brief is unambiguous: "If this demo doesn't work, v0 is not done."

## What this test does

The test is at `tests/integration/killer_scenario.rs` (create it if it doesn't exist; this command will scaffold it on first run).

The shape:

1. **Engineer A**'s session is simulated: a small agent driving Claude Code (or a mock client speaking the same surface) is asked to investigate a fixture bug. It records via `tape record` and ejects a `.tape` file to a tmp path.
2. **Engineer B**'s session: a fresh agent is started with `tape mcp` registered. The agent is given a task that requires information only present in A's tape (e.g. "what was the smoking gun A's investigation found, and which file is it in?").
3. The B agent loads A's tape via `tape.load`, calls `tape.summary`, then targeted `tape.tracks` / `tape.play` / `tape.seek` calls, and produces an answer.
4. The test asserts the answer references a specific known fact embedded in A's tape (e.g. a customer ID, a specific function name, an annotation A's agent pinned).

## Process

1. Verify prerequisites:
   - `tape` binary builds (`cargo build --release -p tape`).
   - The `tape mcp` server tools/list returns 11 tools (cheap probe).
   - At least one valid fixture or recording mechanism exists.
2. If the test file doesn't exist, scaffold it. It needs:
   - A fixture tape (use `tests/fixtures/killer-scenario-a.tape` if present; else dispatch the `fixture-author` agent to create one with a known smoking-gun fact).
   - A small driver that runs `tape mcp` as a subprocess and speaks JSON-RPC to it on behalf of "Engineer B".
   - The expected-answer assertion.
3. Run: `cargo test --release --test killer_scenario`.
4. Report.

## Output shape

```
killer-scenario  integration test

  prerequisites
    ✓ tape binary builds
    ✓ tape mcp tools/list returns 11
    ✓ fixture tests/fixtures/killer-scenario-a.tape exists (8 tracks, valid)

  Engineer A simulation
    ✓ tape recorded successfully (4.2s)
    ✓ liner-notes generated, 4 sections present
    ✓ outcome: success

  Engineer B simulation
    ✓ tape.load returned handle in 12 ms
    ✓ tape.summary returned in 3 ms
    ✓ Engineer B made 4 tape.* calls before answering
    ✓ answer references the expected fact ("CUST-447139", function "process_refund")

OVERALL: PASS — v0 killer scenario is intact.
```

On failure, include:
- Which prereq missed (if any).
- Engineer B's actual answer.
- Engineer B's call sequence (tools called, in order).
- The expected fact that was missing from the answer.

## Rules

- **No mocking the MCP server.** This test runs the real `tape mcp` binary and speaks the real protocol. The whole point is end-to-end validation.
- **Mock the model**, not the protocol. Engineer A's and Engineer B's "agent" can be a deterministic test harness that emits a fixed sequence of MCP/proxy calls. We are NOT testing model intelligence; we are testing that the mechanism delivers the right bytes to the right agent.
- **Failure is loud.** This is the v0 gate. Failed runs MUST exit non-zero so CI catches it.
- **Don't auto-fix.** Report; the parent fixes.
