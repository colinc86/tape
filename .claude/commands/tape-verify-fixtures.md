---
description: Build the tape binary and run `tape verify` against every fixture, plus the malformed fixtures for negative coverage. Reports a per-fixture verdict.
---

Verify every fixture in `tests/fixtures/`. Two passes:

1. **Valid fixtures** in `tests/fixtures/*.tape` — `tape verify` MUST exit 0.
2. **Malformed fixtures** in `tests/fixtures/malformed/*.tape` — `tape verify` MUST exit non-zero, and the diagnostic should match the documented expectation embedded in the fixture name (e.g. `malformed-missing-eject.tape` → expect `MISSING_EJECT_EVENT`).

## Process

1. `cargo build --release -p tape` (release for stable timing; warm builds are fine).
2. For each `tests/fixtures/*.tape`:
   - Run `target/release/tape verify <file>`.
   - Collect exit code and stderr.
3. For each `tests/fixtures/malformed/*.tape`:
   - Same.
   - If a sidecar file `<name>.expected.json` exists, parse it and assert the diagnostic codes match.

## Output shape

```
tape verify  fixture sweep

  valid fixtures  (3)
    ✓ minimal-success.tape       (3 tracks, exit 0, 14 ms)
    ✓ oversized-payload.tape     (5 tracks, 1 artifact, exit 0, 22 ms)
    ✓ redacted-email.tape        (8 tracks, 12 redactions, exit 0, 31 ms)

  malformed fixtures (2)
    ✓ missing-eject.tape         (exit 2, code MISSING_EJECT_EVENT) — matches expected
    ✗ bad-blake3.tape            (exit 2, code MALFORMED_TAPE)      — expected ARTIFACT_HASH_MISMATCH

OVERALL: 1 failure across 5 fixtures
```

If `tape` binary is not yet buildable (early build order), report cleanly:

```
SKIP: tape binary not buildable (build order step 2 not yet complete).
```

Don't try to substitute a partial verifier. Either the binary verifies or we don't run.

If the user passes `update`, regenerate the `<name>.expected.json` files for malformed fixtures from the actual diagnostics (so the user can review the diff and commit). Do NOT update unprompted.
