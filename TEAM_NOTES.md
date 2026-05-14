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

## 2026-05-13 17:24 — Addresser
- No PRs awaiting feedback (`gh pr list --label changes-requested --state open`
  returned empty). No-op tick.
- Cleanup: deleted orphan `addresser/notes-*` branches on origin from earlier
  ticks per the branch-hygiene note in the role brief.

## 2026-05-13 17:27 — Engineer A
- Claimed #109 (severity:low, kind:bug): meta.tool_budget is always None —
  tape diff's Latency summary is silently dead.
- PR #119 opened: `fix(record): populate meta.tool_budget at eject time`.
  Eject now emits a `ToolBudget` unconditionally from the in-flight snapshot
  (total_calls, token sums, wall_clock_ms). 5 regression tests in
  `crates/tape-record/tests/tool_budget_in_meta.rs` (all fail on `main`,
  all pass on the branch).
- Note for the next Engineer A tick: `main` has pre-existing fmt drift
  (161 diffs) and clippy `uninlined_format_args` failures in tape-format /
  tape-mcp-wrap. None are mine and none are touched by #119, but you'll see
  them when you run the BUILD checklist. Worth its own ready issue if not
  already filed.

## 2026-05-13 17:49 — Engineer A
- No claimable ready ticket this tick. Only unassigned `ready` issue is
  still #26 (`tape.fork` + `tape.eject` double-eject), and open PR #32
  already implements Principal's Option B fix (drop trailing eject in the
  pipeline) with regression tests. Same state earlier ticks observed.
- #109 is closed by the merged PR #119 from the prior tick.
- Did not touch labels on #26 or PR #32. No code changes.
- For the next Engineer A tick: nothing to claim until Principal promotes
  another `triage` issue to `ready` (or files a new one). PR #32 still
  carries no `needs-review`/`agent:eng` label, which is what would
  surface it to the Reviewer.

## 2026-05-13 17:54 — Addresser
- No PRs awaiting feedback (`gh pr list --label changes-requested --state open`
  returned empty). No-op tick.

## 2026-05-13 18:19 — Engineer A
- No claimable ready tickets this tick. Only unassigned `ready` issues are
  #26 (`tape.fork` + `tape.eject` double-eject — open PR #32) and #68
  (verify missing `created_at <= ejected_at` check — open PR #69). Both PRs
  were authored by Engineer A earlier; both are still OPEN with no
  `changes-requested`. Skipping to avoid duplicate work.
- Neither PR carries `needs-review`/`agent:eng`, so they still aren't
  surfaced to the Reviewer — same state the 16:42 and 17:49 entries flagged.
- No code changes; no label touches.

## 2026-05-13 18:23 — Reviewer
- Nothing to review (`needs-review` and `re-review` lists both empty). No-op tick.

## 2026-05-13 18:23 — Addresser
- No PRs awaiting feedback (`gh pr list --label changes-requested --state open`
  returned empty). No-op tick.

## 2026-05-13 18:43 — Reviewer
- Nothing to review (`needs-review` and `re-review` lists both empty). No-op tick.

## 2026-05-13 18:50 — Engineer A
- No claimable ready tickets this tick. Only unassigned `ready` issue is still
  #26 (`tape.fork` + `tape.eject` double-eject — open PR #32). Same state the
  16:42, 17:49, and 18:19 entries flagged. PR #32 still carries no
  `needs-review`/`agent:eng` label.
- No code changes; no label touches.

## 2026-05-13 18:53 — Addresser
- No PRs awaiting feedback (`gh pr list --label changes-requested --state open`
  returned empty). No-op tick.

## 2026-05-13 20:07 — Reviewer
- PR #123: approved & squash-merged — enforces SPEC §3.1 `created_at ≤
  ejected_at` in `tape verify` via `BAD_TIMESTAMP` (matches Principal's
  scoping on #68; PR #69 used `INVALID_META_YAML` and is now redundant).
  Closes #68. Tests green workspace-wide; 3 new unit tests + malformed
  fixture. Self-approval blocked (repo-owner author), rationale posted as
  comment and merged per playbook. `agent:reviewer` label applied. The
  pre-existing `uninlined_format_args` clippy errors in
  `crates/tape-format/src/verify.rs` (lines 47, 140, 449, 508) predate
  this PR (confirmed via `git blame` to `^097c894`) — flagged for a future
  cleanup ticket but not blocking. PR #69 remains open and unlabeled;
  leaving it for Principal/author to close as superseded.

## 2026-05-13 19:19 — Engineer A
- No claimable ready tickets this tick. Only unassigned `ready` issue is
  still #26 (`tape.fork` + `tape.eject` double-eject — open PR #32). Same
  state as the 16:42, 17:49, 18:19, 18:50 entries. PR #32 still carries no
  `needs-review`/`agent:eng` label, so the Reviewer can't pick it up.
- No code changes; no label touches.

## 2026-05-13 19:23 — Reviewer
- Nothing to review: both `needs-review` and `re-review` queues empty.

## 2026-05-13 19:23 — Addresser
- No PRs awaiting feedback (`gh pr list --label changes-requested --state open`
  returned empty). No-op tick.

## 2026-05-13 20:40 — Reviewer
- PR #32: changes-requested — branch is ~30 commits behind `main`; `EjectOptions`
  gained `inherited_artifacts` (PR #47) and `label` (issue #72) since the branch
  was cut, and the PR's new test file fails to compile against current `main`
  with E0063. GitHub reports `MERGEABLE/CLEAN` because that only checks textual
  conflicts; no CI checks ran on the branch. Source-side fix in
  `crates/tape-record/src/eject.rs` is correct (Option B per issue #26) and the
  auto-merge anchors it cleanly — only the two `EjectOptions { … }` literals in
  `tests/eject_drops_trailing_eject.rs:30,74` need `inherited_artifacts:
  BTreeMap::new(), label: None`. Detailed feedback posted as comment (self-review
  on own PR blocked). Labels updated: `+changes-requested`, `-needs-review`.

## 2026-05-13 19:49 — Engineer A
- No claimable ready tickets this tick. Only unassigned `ready` issue is
  still #26 (covered by my open PR #32). PR #32 was moved to
  `changes-requested` by the Reviewer at 20:40 — that's the Feedback
  Addresser's job, not mine. No new `ready` tickets have been promoted
  since the prior tick.
- No code changes; no label touches.

## 2026-05-13 19:58 — Addresser
- PR #32 (issue #26 double-eject fix): addressed the Reviewer's 20:40
  must-fix. Rebased onto current `main` (`git rebase origin/main` clean
  — source fix in `crates/tape-record/src/eject.rs` auto-merged). Updated
  both `EjectOptions { … }` literals in `tests/eject_drops_trailing_eject.rs`
  to include `inherited_artifacts: BTreeMap::new()` and `label: None`
  (the fields added by PR #47 and issue #72 since this branch was cut).
- Gate: `cargo test --workspace` green; the two new regression tests pass.
  Did not touch the pre-existing `uninlined_format_args` clippy errors in
  `tape-format`/`tape-mcp-wrap` or fmt drift in `tape-redact/src/rules.rs`
  — same out-of-scope items TEAM_NOTES 17:27 and 20:07 flagged.
- Deferred nice-to-have: deck-level `tape.load → tape.fork → tape.eject`
  end-to-end test. Pipeline-level coverage is the durable SPEC §5.4
  backstop; a deck-level test exercises `tool_fork`'s contract, which is
  orthogonal to this PR. Worth its own ticket if Principal wants it.
- Force-pushed `bfc3f2f` to `fix/eject-drops-trailing-eject` (with
  `--force-with-lease` since the rebase rewrote the original `3cb9a3c`).
  Labels: `+re-review`, `-addressing-feedback`. Top-level summary comment
  posted; review re-requested.

## 2026-05-13 20:01 — Addresser
- No PRs awaiting feedback (`gh pr list --label changes-requested --state
  open` returned empty). PR #32 is now on `re-review` after the 19:58
  pass; nothing else has been bounced back since. No-op tick.

## 2026-05-13 20:02 — Engineer A
- No claimable ready tickets this tick. Only unassigned `ready` issue is
  still #26 (covered by PR #32, now `re-review` after Addresser's rebase
  — Reviewer's territory). No new `ready` tickets promoted since 19:49.
- No code changes; no label touches.

## 2026-05-13 21:05 — Reviewer
- PR #32: approved & squash-merged — Option B fix for #26 (drop a trailing
  eject in the pipeline before appending a fresh one). Closes #26. Local
  `cargo test --workspace` green; both new regression tests in
  `eject_drops_trailing_eject.rs` pass. Self-approval blocked (repo-owner
  author); rationale posted as comment and merged per playbook.
  `agent:reviewer` applied; `re-review` removed; branch deleted on origin.
  The pre-existing `uninlined_format_args` clippy errors in
  `tape-format/src/verify.rs` + `tape-mcp-wrap`, plus fmt drift in
  `tape-redact/src/rules.rs`, predate this PR (already flagged in
  TEAM_NOTES 17:27 and 20:07). Adjacent UX question from issue #26 about
  `tool_fork`'s `recording = false` / outcome-merge is intentionally out
  of scope here — separate ticket if Principal wants it.

## 2026-05-14 00:55 — Reviewer
- PR #39: changes-requested — duplicate of merged PR #35 (which closed
  #30 on 2026-05-13T07:06:49Z, ~3 minutes before #39 was opened).
  `main` already carries the schema enum + `parse_outcome_arg` + wiring
  in `tool_eject`. The diff overlaps the merged fix verbatim in shape;
  PR body itself acknowledged the parallel PR. Self-review blocked
  (repo-owner author); detailed rationale posted as a PR comment.
  Labels: `+changes-requested`, `-needs-review`. Requested close as
  superseded — no marginal value to landing on top. One minor behavioral
  divergence noted (this PR's stricter "outcome must be a string" vs
  main's `and_then(as_str)` collapse to Unknown); flagged in the comment
  as worth a separate tiny ticket if anyone wants the stricter contract,
  but not part of this review.

## 2026-05-13 20:19 — Engineer A
- No claimable ready tickets this tick — `gh issue list --label ready`
  returns empty (both with and without `kind:bug`). PR #32 merged at
  21:05; that closed #26 (the long-standing sole `ready` ticket). Sole
  remaining `kind:bug` is #66, which is `in-progress` and covered by
  Engineer B's PR #125 (`needs-review`). No new `ready` tickets have
  been promoted by the PM/Principal since the last tick.
- No code changes; no label touches.

## 2026-05-13 20:24 — Addresser
- PR #39: closed as superseded by merged PR #35 (per Reviewer's
  `changes-requested` rationale at 00:55). Verified state of `main`:
  PR #35 merged at 07:06:49Z (commit `97df1f6`), issue #30 closed at
  07:06:51Z, and `crates/tape-mcp/src/tools.rs` already carries the
  schema enum + `parse_outcome_arg` + `EjectOptions { outcome, ... }`
  wiring this PR proposed. Agreed with Reviewer; no marginal value to
  landing on top.
- Deferred nice-to-haves (per Reviewer's suggestion): the stricter
  "non-string `outcome` → `INVALID_PARAMS`" contract and the broader
  `eject_outcome.rs` test matrix. Left both for separate small tickets
  against `main` if Principal/PM wants them — not the Addresser's seat
  to file new work.
- Labels: claimed with `+addressing-feedback / -changes-requested`,
  then `-addressing-feedback` on close (no `superseded` label exists
  in the repo). Posted top-level summary comment and used `gh pr close`
  with a one-line audit pointer.

## 2026-05-14 01:00 — Reviewer
- PR #47: changes-requested — superseded by merged PR #46 (closed issue
  #41 on 2026-05-13T07:47Z). `main` already carries the equivalent fix
  as `EjectOptions::inherited_artifacts` (`crates/tape-record/src/eject.rs`
  lines 39, 89). PR #47 is `CONFLICTING/DIRTY` against current `main`
  (branch predates PR #46, PR #73 `label` field, and PR #27/#79 scan
  rewiring). Recommended close as superseded. Self-approval blocked
  (repo-owner author); detailed rationale posted as PR comment. Labels:
  `+changes-requested`, `-needs-review`. Salvageable nice-to-have flagged
  for a follow-up PR: the orphan-dropping pass (walking `refs` arrays
  + nested `{"ref":"sha:<hex>"}` stubs to prune unreferenced inherited
  artifacts) is real net-new logic not in main; would compose cleanly
  with #46's wholesale-inherit behavior, especially now that #26 is
  fixed and `is_valid()` end-to-end assertion holds.

## 2026-05-14 01:25 — PM
- Refined ROADMAP.md per strategic issue #129 (PM-addressed): the static
  "Open bugs not blocking the release" list went stale within hours. Replaced
  with a `gh issue list --label priority:current` procedure plus a timestamped
  snapshot; PM regenerates the snapshot on every ROADMAP refine. Headline
  fixes list updated to reflect #119, #123, #32 (all merged since previous
  tick) and to drop the now-incorrect "PR #69 pending" reference.
- Decision worth flagging: **v0.1.2 is one PR away from releasable** — PR
  #125 (#66 docs-only SPEC §10.6 fix) is the lone `priority:current` open
  bug, currently in `needs-review`. Once it merges, only the cargo bump +
  RELEASE_NOTES prose + binary rebuild + tag + `gh release create` remain.
- Decision worth flagging: **adopted Principal's priority-label workflow**
  (#118 / #126) in ROADMAP's Backlog intro. PM no longer enumerates
  release-blocking bugs statically; engineers query the label set live.
- Closing #129 with a comment pointing at this commit.
- Not done this tick: cutting the release (waiting on #125, plus binary
  rebuild is outside PM scope). PR triage of the 5+ open notes-only PRs
  (#112 #116 #122 #124 #130 #131) — those look like Principal/Reviewer
  housekeeping branches that need someone to either merge or delete; not
  PM's job to close.

## 2026-05-13 20:33 — Engineer A
- No claimable ready tickets this tick — `gh issue list --label ready`
  returns empty (with and without `kind:bug`, and with no assignee
  filter). Queue genuinely empty; no PM/Principal promotions since the
  20:19 Engineer A tick. No own open PR in `in-progress`. No code or
  label changes.

## 2026-05-13 20:38 — Addresser
- PR #47: closed as superseded by merged PR #46 (per Reviewer's
  `changes-requested` rationale at 01:00). Verified state of `main`:
  PR #46 merged at 07:47:07Z, issue #41 closed at 07:47:08Z, and
  `crates/tape-record/src/eject.rs:39,89` already carries the
  equivalent `EjectOptions::inherited_artifacts` fix (mechanically
  identical to this PR's Option A, modulo field name and the
  required-vs-`Option` shape — main's is the less ceremonial choice).
  Branch is also `CONFLICTING/DIRTY` against current `main` (PR #73's
  `label` field plus PR #27/#79's scan rewiring landed since this
  branch was cut). Agreed with Reviewer; no marginal value to
  rebasing onto current `main`.
- Flagged the salvageable orphan-dropping pass (walking `refs` arrays
  + nested `{"ref": "sha:<hex>"}` stubs to prune unreferenced
  inherited artifacts after fork-truncation) as worth a separate
  small PR over current `main`. Not in the Addresser's seat to open
  new tickets — left a pointer in the PR close comment so the work
  isn't lost.
- Labels: claimed with `+addressing-feedback / -changes-requested`,
  then `-addressing-feedback` on close (no `superseded` label exists
  in the repo). Posted top-level summary comment and used `gh pr close`
  with a one-line audit pointer. Matches the 20:24 Addresser playbook
  for PR #39.

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

## 2026-05-13 17:26 — Principal Decision
- Filed #118: workflow-label discipline for kind:bug issues. PINNED.
- Retroactively labelled `triage` on: none — sweep found 0 open `kind:bug`
  issues lacking a workflow label (only #109 `in-progress` and #26 `ready`).
- Created missing `kind:process` label (color `#1D76DB`). All other workflow
  / agent labels already existed.
- Effective immediately; bug-finder loop expected to comply (agent:pm to
  update its prompt).

## 2026-05-13 18:17 — Principal
- Decide rule A' (well-investigated bug lacking a workflow label).
  Promoted #68 `tape verify does not check created_at <= ejected_at
  (SPEC §3.1 MUST)` directly to `ready`. Reporter had already supplied
  reproducer + suggested patch + edge-case analysis, so this tick locked
  in acceptance criteria and a non-obvious choice: reuse `BAD_TIMESTAMP`
  rather than `INVALID_META_YAML` or a new code. Labels: added
  `ready`, `kind:bug`, `agent:principal`.
- Housekeeping: #66 (SPEC §10.6 missing diagnostic codes) was a sibling
  bug missing both `kind:bug` and a workflow label. Added `triage` +
  `kind:bug` so it surfaces in the standard triage queue for the next
  Principal tick — not refining today (one issue per tick).
- Did NOT touch #26 (still `ready`, PR #32 implements it but lacks
  `needs-review`; engineer/reviewer labelling is not Principal's hat).
- Pre-existing fmt drift in tape-format / tape-mcp-wrap flagged by
  Engineer A on 2026-05-13 17:27 still has no issue filed. Out of
  scope for this tick, but worth filing next time if it persists.

## 2026-05-13 23:18 — Principal
- Refined #66 (SPEC §10.6 missing `LINER_SECTIONS_OUT_OF_ORDER` +
  `UNKNOWN_ENTRY`): `triage` → `ready`, added `agent:principal`. Pure
  SPEC-text fix; no production code changes needed. Acceptance criteria,
  rebase plan, and out-of-scope list posted as an issue comment.
- Notable wrinkle: an engineer-authored fix already exists as PR #70
  (`fix/spec-add-missing-codes`), but it is `CONFLICTING` against `main`
  (PR #65 shifted §10.6 by adding `RESERVED_KIND` after #70 was opened)
  and carries no workflow labels. Engineer who picks #66 should either
  rebase #70 or open a fresh PR and close #70 as superseded.
- Investigation handoff: `.tape-handoffs/issue-66-spec-10-6-missing-codes.investigation.md`
  on branch `principal/issue-66` (pushed).
- Surfaced for human attention: PR-routing meta-gap persists. PRs #32,
  #70, and #69 are all engineer-authored, addressing real Principal-`ready`
  bugs, and carrying no `needs-review`/`agent:eng` label so the Reviewer
  doesn't see them. Per the wake-up brief I am NOT declaring a routing
  policy this tick; flagging so the human can decide which role owns
  engineer-PR labeling. Same flag the 16:42, 17:49, 18:19, 18:50 Engineer
  ticks have raised.
- Out of scope this tick (but worth a future ticket): `verify.rs:42, 75`
  defines `UnsafePath` / `"UNSAFE_PATH"` that is never emitted — reader
  rejects unsafe paths before verify runs. Either wire emission or remove
  the variant + SPEC entry. Sibling of #60.

## 2026-05-13 19:39 — Principal Decision
- Filed #126: workflow-label discipline for PRs. PINNED. Sibling to #118.
- Retroactively `needs-review`: PR #32, PR #39, PR #47, PR #58, PR #63,
  PR #69, PR #70, PR #94, PR #97, PR #113, PR #114, PR #115, PR #117,
  PR #120, PR #121, PR #125.
- Commented on PR #70 with rebase guidance (post-#65 `CONFLICTING` on
  `crates/tape-format/src/verify.rs` and SPEC.md §10.6).
- Labels created: `in-review` (#0E8A16), `approved` (#0075CA).
  `needs-review`, `changes-requested`, `blocked` pre-existed.
- Skipped (author-merge `principal: notes` cosmetic pile-up): PR #112,
  PR #116, PR #122, PR #124. Separate cleanup, out of scope.
- Note: PR #69 is superseded by merged #123; labelled per policy but
  recommend closing as superseded rather than reviewing.

## 2026-05-13 20:30 — Principal Backlog Hygiene
- Total open issues reviewed: 34 (32 non-pinned + #118/#126 pinned policy, skipped).
- Created labels: `priority:current`, `priority:next`, `priority:later`,
  `principal-close-candidate`.
- Labelled priority:current: #26, #66 (both release-eligible bugs named in
  ROADMAP's v0.1.2 section as deferrable to v0.1.3).
- Labelled priority:next: #74 (`tape annotate` CLI), #81 (`tape doctor`),
  #106 (RuntimeAdapter trait) — all explicitly named as v0.2 stretch in
  ROADMAP's Next Milestone section.
- Labelled priority:later: #2, #8, #10, #18, #31, #42, #51, #61, #67, #71,
  #78, #85, #88, #89, #90, #93, #95, #99, #100, #101, #102, #103, #104, #105,
  #107, #108, #110 (27 issues, all sitting in ROADMAP's Backlog buckets).
- Added missing kind:* labels: `kind:feature` added to all 30 enhancement
  issues (every enhancement-tagged issue was missing it per #118 discipline).
  #26 and #66 already had `kind:bug`.
- Closed as fixed: none in this pass. #68 (created_at≤ejected_at) and #109
  (meta.tool_budget) were already closed by colinc86 before this run.
- Closed as duplicate: none — no clear duplicates among open enhancement
  issues; they're well-differentiated by ROADMAP buckets.
- Tagged principal-close-candidate: none. The backlog as filed by PM
  matches the ROADMAP's themed backlog buckets cleanly; nothing looks
  out-of-scope for v0.x.
- Roadmap drift spotted: ROADMAP names #68 and #109 under "Open bugs not
  blocking the release (can roll into v0.1.3)" — but both are already closed
  on GitHub (per PR #123 for #68; #109 closed 2026-05-13). The Open-bugs
  list in the v0.1.2 milestone section is now down to #26 and #66 only.
  Surfacing this here so PM can refresh ROADMAP at release-cut time. Not
  modifying ROADMAP.md per scope.
- No PR actions, no production code changes, no ROADMAP edits.

## 2026-05-13 20:02 — Principal
- Filed #129: ROADMAP drift — §"Open bugs not blocking the release" still
  lists #68 (closed today via PR #123) and #109 (closed today by owner).
  Routed to `agent:pm` with `kind:doc`. Genuine open priority:current bugs
  as of this tick are #26 and #66.
- Triage queue empty; no `kind:bug` issues lack workflow labels; no
  reopened issues; no ready ticket aged >7 days. Decide-rule C (roadmap
  drift) was the cleanest candidate this tick.

## 2026-05-13 20:20 — Principal
- Workflow-label cleanup on #66 (SPEC §10.6 missing diagnostic codes):
  restored `in-progress` per #118. Issue had no workflow label after
  `in-progress` was removed at 00:37:46 with no replacement — leaving
  the only open `kind:bug` non-compliant. Two PRs are open against #66
  (PR #70 from 09:12 and PR #125 from 00:37, both in `needs-review`),
  so engineering is actively working it; `in-progress` is the correct
  state. Posted explanatory comment on the issue.
- State scan: triage queue empty. Open `kind:bug` count = 1 (#66, now
  compliant). No `ready` issues currently exist (the only one left,
  #26, closed at 01:05 after PR #32 squash-merged). New since prior
  tick: #26 closed; PR #125 opened against #66 (Engineer-B branch);
  PR #39 changes-requested as duplicate of merged #35 (Reviewer 00:55).
  8 `principal: notes` PRs from earlier ticks still open per known
  state — not my problem to merge.

## 2026-05-13 21:30 — Principal Executive Housekeeping
- Coordinated engineer collision on #66: declared PR #125 canonical, PR #70
  superseded. Comments posted on issue and PR.
- Filed #132: Remove unreachable UNSAFE_PATH diagnostic. `kind:cleanup`,
  `priority:later`, `ready`.
- Consolidated 9 stacked `principal: notes` PRs (#112, #116, #120, #122,
  #124, #127, #128, #130, #131) into this PR. Declared new procedure:
  rolling weekly notes PR going forward.

## 2026-05-13 21:17 — Principal
- No-op tick. Board clean: triage empty, no `kind:bug` lacking workflow
  label (#66 still the only open bug, `in-progress` + `priority:current`,
  Engineer-A just opened PR #134 against #132), zero `ready` issues so
  no staleness check needed. Recent landings (#32 fixes #26, #123
  enforces created_at ≤ ejected_at) and #134 in-flight — engineering
  velocity healthy, nothing for Principal to refine, close, or file.

## 2026-05-13 22:16 — Principal
- No-op tick. State unchanged from 21:17: triage queue empty, #66 still
  the only open `kind:bug` (compliant: `in-progress` + `priority:current`),
  zero `ready` issues so staleness check vacuous. No PRs merged in the
  last hour, no new triage-needing comments. PR #134 (Engineer-A,
  UNSAFE_PATH cleanup against #132) and PR #125 (Engineer-B, #66 SPEC
  fix) remain open in `needs-review`. Nothing for Principal to refine,
  close, or file. Rule F.

## 2026-05-13 23:15 — Principal
- No-op tick. Board state cleaner than prior ticks: PR #125 merged →
  #66 closed, PR #70 closed (superseded path completed), #129 closed
  by PM (aaa2e19 — ROADMAP §"Open bugs" replaced with live label-query
  procedure). Zero open `kind:bug` issues, triage queue empty, zero
  `ready` issues so staleness vacuous, no new merges in the
  >2026-05-13T21:00 window besides the already-noted #125. PR #134
  (Engineer-A → #132) still open in `needs-review`. Nothing for
  Principal to refine, close, or file. Rule F.

## 2026-05-13 23:16 — Principal
- No-op tick (fourth consecutive). State identical to 23:15: triage
  empty, zero open `kind:bug`, zero `ready` issues (staleness vacuous),
  no PR merges since #125 at 00:37Z. PR #134 still sitting in
  `needs-review` — Reviewer's lane, not mine. Rule F.
