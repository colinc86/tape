# Team Notes

Shared log for the agents that wake up against this repo. Append-only — never
edit prior entries. Format each entry as `## YYYY-MM-DD HH:MM — <role>`.

## Roles

- **PM** — Product Manager. Owns ROADMAP.md, RELEASE_NOTES.md, README.md, and
  files strategic feature/doc issues. Never touches code, never reviews PRs.
- **Principal** — Principal engineer. Converts ROADMAP items into engineering
  tickets, triages incoming issues, decides architecture.
- **Eng** — Implements tickets. Opens PRs.
- **Reviewer** — Reviews PRs.
- **Addresser** — Addresses PR feedback.
- **Bug-finder** — Reads the codebase looking for bugs, files them as issues.
- **Bugfixer** — Picks up `kind:bug` issues and ships fixes.

---

## 2026-05-13 20:50 — PM
- Bootstrapped ROADMAP.md and TEAM_NOTES.md from README + RELEASE_NOTES + the
  current issue set. Roadmap reorganizes the open enhancements (24 PM-suggested
  features filed today) into themed backlog buckets and names v0.1.2 as the
  next release (a patch rollup of the ~30 fixes merged since v0.1.1).
- Decision worth flagging: **v0.1.2 is releasable today**. All post-v0.1.1
  commits are backward-compatible bug fixes; the only blocker for cutting is
  the macOS-Apple-Silicon binary rebuild that ships with `/plugin install`. PM
  scope ends at tag+release; whoever owns the plugin pipeline needs to build
  and upload the tarball + SHA256SUMS before the release is user-facing.
- Decision worth flagging: 4 open bugs (#26 severity:medium, #109 #68 #66
  severity:low). None are release-blockers for v0.1.2 since fix complexity is
  small and they can roll into v0.1.3 if the patch ships first.
- Not done this tick: PR review (not PM's job — there are ~10 open duplicate
  PRs that need someone to close). Issue triage on the 24 enhancement issues
  (that's Principal's call).

## 2026-05-13 16:35 — Principal
- Refined #26 (`tape.fork` at last step + `tape.eject` produces invalid tape
  with two eject events). Confirmed bug against `main @ 926c5c3`, picked
  Option B (defensive strip in `eject::eject`) as the canonical fix, and
  ruled the two "adjacent points" in the original report out of scope (one
  already resolved by PR #35/#36, the other a separate UX question).
  Labels: `agent:principal`, `kind:bug`, `ready` added.
- No `triage`-labelled issues this tick. The four open bugs (#26, #109, #68,
  #66) were filed today by the bug-sweep loop with high-quality investigations
  already — they were skipping the `triage` step and arriving directly. I'm
  picking the highest-severity (#26) per tick and promoting one at a time;
  the others stay un-workflow-labelled until a future tick.
- Investigation notes:
  `.tape-handoffs/issue-26-fork-eject-double-terminator.investigation.md` on
  branch `principal/issue-26`.
- Flag for human: the bug-sweep loop's issues are not coming in with `triage`
  by default. If the Principal pipeline is meant to be the gate, the
  bug-finder workflow should be adjusted to apply `triage` on file; otherwise
  the Principal step is bypassable. Not changing anything this tick.
