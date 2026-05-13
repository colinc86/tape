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

## 2026-05-13 16:42 — Engineer A
- No ready tickets available this tick. Only `ready` issue is #26
  (`tape.fork` + `tape.eject` double-eject), and PR #32 already implements
  the Principal's recommended Option B fix (drop trailing eject in the
  pipeline) with a regression test. PR #32 has no `needs-review` label, so a
  Reviewer-labeling pass would unblock it — leaving that for the Principal or
  whichever role owns the agent-PR labels.
- Not claiming #26 to avoid duplicating PR #32. Did not touch labels on #26
  or PR #32.

## 2026-05-13 16:43 — Addresser
- No PRs awaiting feedback (`gh pr list --label changes-requested --state open`
  returned empty). No-op tick.

## 2026-05-13 17:18 — Principal
- Refined #109 (`meta.tool_budget always None — tape diff's Latency summary
  is silently dead`): added Principal comment with problem statement,
  acceptance criteria, approach hint, files-of-interest with line numbers,
  out-of-scope, and test plan. Labels: `bug, severity:low, kind:bug,
  agent:principal, ready`.
- Investigation notes: `.tape-handoffs/issue-109-tool-budget-unpopulated.investigation.md`
  on branch `principal/issue-109` (pushed). Verified one Meta construction
  site (`crates/tape-record/src/eject.rs:156`), `ToolBudget` struct already
  exists at `crates/tape-format/src/meta.rs:67-73`, and a cleaner
  chrono-arithmetic implementation than the bug report sketched.
- Picked #109 (rule A') over #68 / #66 because both of those already have
  open PRs (#69 / #70 respectively); #109 had no PR and no workflow label.
  #26 already promoted (per prior tick) and has open PR #32. Two other
  `kind:bug`-style issues (#68, #66) remain unpromoted but their PRs are
  in flight — leaving them for the Reviewer / Bugfixer dance until a PR
  lands or stalls. One thing per tick.
- Heads-up for human: branch `principal/issue-109` is push-only with the
  handoff markdown — leave for merge or let it linger; the issue body
  links it via raw GitHub URL.
