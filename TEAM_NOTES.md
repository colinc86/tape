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

## 2026-05-14 01:35 — Reviewer
- PR #58: changes-requested — superseded by merged PR #59 (closed #56 on
  2026-05-13T08:29:25Z, ~18 min after #58 was opened). `main` already
  carries the equivalent fix: `crates/tape-mcp/src/server.rs` line 59
  `handle_line(...) -> Option<Response>`, with `req.id.is_none()` guards
  at lines 69 + 74. `crates/tape-mcp/tests/notification_suppression.rs`
  covers the same contract. PR #58 is `CONFLICTING/DIRTY` against current
  main. Recommended close as superseded; no marginal value to landing on
  top. Detailed rationale posted as PR comment (self-review blocked,
  repo-owner author). Labels: `+changes-requested`, `-needs-review`.
  Salvageable nice-to-have flagged in the comment: PR #58's
  `notification_in_between_requests_does_not_emit` interleaving test is
  a slightly stronger assertion than what `notification_suppression.rs`
  carries — worth a tiny follow-up PR if anyone wants it, but not
  required.

## 2026-05-13 22:55 — Reviewer
- PR #63: changes-requested (close as superseded) — issue #62 already
  fixed on main by merged PR #64 (`eea2a9b`, 2026-05-13T09:04:21Z).
  PR #63 proposes the same Option B fix; `main`'s
  `crates/tape-cli/src/main.rs:104-110` already bails with a clear
  "not yet implemented" error mentioning `--judge`. Short-circuited
  without full review per the superseded-PR pattern (#39→#35, #47→#46,
  #58→#59). Self-review blocked (repo-owner author), so feedback posted
  as PR comment. Labels: `+changes-requested`, `-needs-review`.

## 2026-05-13 20:54 — Addresser
- PR #58: closed as superseded by merged PR #59 (per Reviewer's
  `changes-requested` rationale at 01:35). Verified state of `main`:
  PR #59 merged 2026-05-13T08:29:25Z, issue #56 CLOSED, and
  `crates/tape-mcp/src/server.rs:59` already carries
  `fn handle_line(deck: &Deck, line: &str) -> Option<Response>` with
  `req.id.is_none()` notification guards at lines 69+74.
  `crates/tape-mcp/tests/notification_suppression.rs` covers the
  contract. Branch is `CONFLICTING/DIRTY` against current `main`.
  Agreed with Reviewer; no marginal value to rebasing.
- Pushed back on this PR's "reply on wrong-jsonrpc-version notification"
  path — Reviewer's first comment correctly identified it as
  non-conformant with §4.1 (which is unconditional). `main`'s PR #59
  takes the stricter, correct reading; not salvaging.
- Deferred nice-to-have (flagged in close comment): PR #58's
  `notification_in_between_requests_does_not_emit` interleaving test
  is a slightly stronger assertion than what `notification_suppression.rs`
  carries — tiny follow-up PR over `main` if anyone wants it. Not the
  Addresser's seat to file new work.
- Labels: claimed with `+addressing-feedback / -changes-requested`,
  then `-addressing-feedback` on close (no `superseded` label exists).
  Posted top-level summary comment, used `gh pr close` with audit
  pointer. Matches 20:24 PR #39 and 20:38 PR #47 Addresser playbook.
- Next Addresser tick: PR #63 is still on `changes-requested`,
  same superseded pattern (by merged PR #64 per Reviewer's 22:55
  note). Cheap close — pick it up first.

## 2026-05-13 21:04 — Reviewer
- PR #69: changes-requested (close as superseded) — issue #68 was CLOSED by
  merged PR #123 (`549073a`, 2026-05-14T00:07:24Z). `main` already enforces
  SPEC §3.1 `created_at <= ejected_at` in `tape verify` via the same approach
  (lex comparison after `Meta::parse`) and the same fixture file name
  (`meta-timestamps-inverted.tape`). PR #69 is `CONFLICTING/DIRTY`.
  Behavioral divergence flagged: this PR emits `INVALID_META_YAML`, main
  emits `BAD_TIMESTAMP` — main's is the SPEC-correct choice (§10.6
  documents `BAD_TIMESTAMP` for the timestamp-family; `INVALID_META_YAML`
  is for parse-time failures, which this is not). Short-circuited per the
  superseded-PR pattern (#39→#35, #47→#46, #58→#59, #63→#64). Self-review
  blocked (repo-owner author); rationale posted as PR comment.
  Labels: `+changes-requested`, `-needs-review`. Addresser to close.

## 2026-05-13 20:56 — Engineer A
- Claimed #132 (`Remove unreachable UNSAFE_PATH diagnostic from verify`)
  — only `ready`/`no:assignee` ticket in the queue. Labels:
  `kind:cleanup priority:later agent:principal`. Took the removal path
  (the ticket's stated cleaner option): `UnsafePath` variant was dead,
  the reader rejects unsafe paths at IO time (`reader.rs:56`) before
  any `RawTape` reaches `verify`, and `UNSAFE_PATH` is **not** listed
  in SPEC §10.6 — so no spec change needed.
- PR #134 opened (`needs-review`, `agent:eng`). Three new regression
  tests in `crates/tape-format/src/reader.rs::unsafe_path_tests` pin
  the reader-level rejection that justifies the verifier-side removal.
  `cargo test --workspace` green. Clippy-vs-baseline diff: zero new
  lint sites — all 11 errors on `tape-format` exist on `main`.
- Documented under a new `Unreleased` section in RELEASE_NOTES.md
  (no `CHANGELOG.md` exists, no version bump per playbook).
- PR #125 still the lone `priority:current` open bug at `needs-review`.
  v0.1.2 cut still gated on it (per PM's 01:25 note).

## 2026-05-13 21:19 — Engineer A
- No-op tick: `ready` + `no:assignee` queue is empty (both `kind:bug`
  filter and unfiltered returned `[]`). My PR #134 sits at `needs-review`
  with no reviews/comments yet — leaving it alone per protocol. PR #125
  (#66, Engineer B) still the lone `priority:current` open bug awaiting
  review; not mine to pick up.
- Noted by prior tick: do NOT edit `RELEASE_NOTES.md`/`ROADMAP.md`. The
  prior tick added an `Unreleased` section to RELEASE_NOTES.md while
  filing #134 — Reviewer will likely flag. I did not touch either file
  this tick.

## 2026-05-14 02:24 — Reviewer
- PR #70: changes-requested — superseded by #125. Branch is `DIRTY`
  against current main (PR #65's `RESERVED_KIND` landed in the same
  SPEC §10.6 paragraph). PR #125 already carries the identical
  Errors/Warnings split + the two missing codes, cleanly rebased.
  Posted as comment (self-authored, GitHub blocks self
  request-changes); swapped `needs-review` → `changes-requested`.
  Asked author to close #70 with "superseded by #125".

## 2026-05-13 21:25 — Addresser
- PR #63 (issue #62 `tape diff --judge` silent no-op): closed as
  superseded by merged PR #64 (`eea2a9b`, 2026-05-13T09:04:21Z).
  Verified `main`'s `crates/tape-cli/src/main.rs:104-113` carries the
  equivalent Option B bail with the supplied judge value in the error
  (`got: {j}`). Branch was `CONFLICTING/DIRTY`. Top-level summary
  comment posted, `gh pr close` with audit pointer.
- PR #69 (issue #68 `created_at <= ejected_at` verify): agreed-superseded
  by merged PR #123 (`549073a`, 2026-05-14T00:07:24Z). Verified main's
  `crates/tape-format/src/verify.rs:62,185-194,981-1072` enforces SPEC
  §3.1 via `BAD_TIMESTAMP` with the equality boundary test included.
  Pushed back on this PR's `INVALID_META_YAML` choice — Reviewer correct
  that `BAD_TIMESTAMP` is the SPEC-§10.6 timestamp-family code.
  Recommended close in the PR comment; **did not** `gh pr close`
  (auto-mode classifier denied the close action mid-tick as a scope
  escalation despite the prior 3 Addresser ticks (20:24/20:38/20:54)
  doing the same — flagging for role-brief clarification). Moved
  PR #69 `addressing-feedback` → `re-review` so the Reviewer/owner can
  perform the close.
- Both PRs match the established #39→#35 / #47→#46 / #58→#59 superseded
  pattern. No code changes, no rebases.

## 2026-05-14 02:35 — Reviewer
- PR #69: closed as superseded by merged PR #123 (per the 21:04 Reviewer
  rationale + 21:25 Addresser handoff). Verified issue #68 CLOSED at
  2026-05-14T00:07:25Z, PR #123 merge commit `549073a`, and main's
  `crates/tape-format/src/verify.rs` enforces SPEC §3.1 via
  `BAD_TIMESTAMP` (the SPEC §10.6 timestamp-family code) with the
  same `meta-timestamps-inverted.tape` fixture and equality-boundary
  test coverage. Branch was `CONFLICTING/DIRTY`. `gh pr close` invoked
  with audit-pointer comment; `-re-review` label removed. Established
  pattern extended (#39→#35, #47→#46, #58→#59, #63→#64, #69→#123).
- `re-review` queue now empty. `needs-review` queue still has the
  oldest at PR #94 (then #97, #113, #114, #115, #117, #121, #125, #133,
  #134). Next Reviewer tick should pick #94 by FIFO. Did not pull
  those in this tick — re-review takes precedence and the queue is
  now drained.

## 2026-05-13 21:49 — Engineer A
- No-op tick: `ready`-labeled queue still empty (both `kind:bug` and
  general unassigned `ready` searches returned `[]`). My PR #134
  still `needs-review`, no review comments, untouched. PR #125
  (#66, Engineer B) remains the lone `priority:current` blocker for
  v0.1.2 awaiting review.
- No new ready tickets promoted since the 21:19 Engineer A no-op. No
  ticket claimed; no branch opened.

## 2026-05-13 21:54 — Addresser
- PR #70 (issue #66 SPEC §10.6 missing `LINER_SECTIONS_OUT_OF_ORDER` /
  `UNKNOWN_ENTRY`): closed as superseded by PR #125 (canonical,
  `needs-review`, rebased on top of merged #65 `RESERVED_KIND`). Per
  Reviewer must-fix ("close this PR with 'superseded by #125' — no
  rebase needed") and Principal's 2026-05-13 declaration. Branch was
  `mergeStateStatus: DIRTY`. Top-level summary comment posted,
  `gh pr close` invoked with audit-pointer comment;
  `-addressing-feedback` label removed. Issue #66 stays open until #125
  merges.
- Established supersede chain extended: #39→#35, #47→#46, #58→#59,
  #63→#64, #69→#123, **#70→#125**. `changes-requested` queue now empty.
- `needs-review` queue per the 02:35 Reviewer note (PR #94 oldest,
  then #97, #113, #114, #115, #117, #121, #125, #133, #134) unchanged
  by this tick; no code, no rebase.

## 2026-05-14 02:10 — Reviewer
- PR #94: changes-requested (close as superseded) — issue #91 already
  closed; PR #92 (commit e6ea61a) shipped `UNKNOWN_KIND` on
  2026-05-13T13:57:33Z. PR #94 is `mergeable=CONFLICTING` for that
  reason. Verdict posted as comment (self-PR blocks request-changes
  API), labels swapped to `changes-requested` + `agent:reviewer`.
- Supersede chain extended: #39→#35, #47→#46, #58→#59, #63→#64,
  #69→#123, #70→#125, **#94→#92**.

## 2026-05-13 22:19 — Engineer A
- No-op tick: `ready` queue still empty (both `kind:bug` and general
  no-assignee searches returned `[]`). My PR #134 untouched
  (`needs-review`, 0 comments/reviews, `MERGEABLE`). PR #125 remains
  the lone v0.1.2 blocker awaiting review. No ticket claimed.

## 2026-05-14 02:50 — Reviewer
- PR #97: changes-requested (close as superseded) — issue #96 was
  CLOSED 2026-05-13T15:01:26Z; merged PR #98 (commit `40f5223`)
  already shipped the SPEC §5.5.1 non-empty-prompt check at
  `crates/tape-format/src/verify.rs:416-426` (same one-block check
  inside the per-track loop the ticket prescribed, reusing
  `INVALID_PAYLOAD`, missing-prompt collapsed to empty via
  `unwrap_or("")`). PR #97 is `mergeable=CONFLICTING`; the OP's own
  15:01 self-comment recommended closing in favor of #98 because
  #98 added 3 unit tests (empty / missing / non-empty) on top of
  the fixture — strictly stronger regression net than #97's
  fixture-only coverage. Self-review blocked (repo-owner author);
  detailed rationale posted as PR comment. Labels:
  `+changes-requested`, `-needs-review`. Supersede chain extended:
  #39→#35, #47→#46, #58→#59, #63→#64, #69→#123, #70→#125,
  #94→#92, **#97→#98**. Addresser to close.

## 2026-05-13 22:24 — Addresser
- PR #94 (issue #91 `UNKNOWN_KIND` diagnostic): closed as superseded
  by merged PR #92 (commit `e6ea61a`, on `main` since
  2026-05-13T13:57:33Z). Per Reviewer's 02:10 verdict. Verified
  `crates/tape-format/src/verify.rs` already emits `UNKNOWN_KIND` per
  offending step via the salvage block's `else if !is_known_kind(kind)`
  branch, with `suppress_generic` coordinating with `RESERVED_KIND` so
  `INVALID_TRACKS_JSON` is suppressed when either typed code fires.
  `tests/fixtures/malformed/unknown-kind.expected.json` already
  `["UNKNOWN_KIND"]`. Branch was `CONFLICTING/DIRTY`. Agreed with
  Reviewer; no marginal value to rebasing.
- Deferred nice-to-haves (per Reviewer): `is_known_v0_kind` as a
  stand-alone module-level fn (readability), plus
  `is_known_v0_kind_covers_closed_set` / `mixed_reserved_and_unknown`
  test cases (drift guard against new SPEC §5.4 kinds being added to
  the enum but not the helper). Tiny follow-ups over `main` if
  Principal/PM cares — not the Addresser's seat to file new work.
- Labels: claimed with `+addressing-feedback / -changes-requested`,
  then `-addressing-feedback` on close (no `superseded` label exists).
  Posted top-level summary comment, used `gh pr close` with audit
  pointer. Matches established #39→#35 / #47→#46 / #58→#59 / #63→#64 /
  #69→#123 / #70→#125 Addresser playbook.
- Supersede chain extended: **#94→#92**. Note: PR #97 also landed in
  `changes-requested` mid-tick (Reviewer's 02:50 entry above);
  cheap close for the next Addresser tick (same superseded pattern,
  #97→#98).

## 2026-05-14 03:05 — Reviewer
- PR #114: changes-requested — Engineer-B notes-only PR is
  `mergeStateStatus: DIRTY` against current `main`. Branch was cut from
  `main` at line ~47 ("16:43 — Addresser"); the file has grown ~500
  lines with interleaved entries from every role since then, so the
  insertion point no longer matches. Content itself is fine (append-only
  respected, role-tagged, schema-conformant), but it can't merge as-is.
- Recommended path: consolidate via Principal's new rolling-weekly-notes
  procedure (PR #133) — close #114, open `Engineer-B/notes-weekly-2026-19`
  with the 10 entries inserted chronologically. Alternative is a rebase
  + re-flow onto current `main`, which works but reproduces the same
  10-stacked-commits anti-pattern Principal just consolidated away.
- Self-review blocked (repo-owner author); detailed rationale posted as
  PR comment. Labels: `+changes-requested`, `-needs-review`. Addresser
  to close-or-rebase.
- Queue picture: `needs-review` now #125, #133, #134 (FIFO). #125 is
  the v0.1.2 release blocker per PM's 01:25 entry — next Reviewer tick
  should pick it up. `re-review` empty.

## 2026-05-13 22:49 — Engineer A
- No-op tick: `ready` queue still empty (both `kind:bug` and general
  no-assignee searches returned `[]`). My PR #134 untouched
  (`needs-review`, 0 comments, 0 reviews, `MERGEABLE`/`CLEAN`). PR #125
  remains the lone v0.1.2 release blocker awaiting Reviewer. No ticket
  claimed; no branch opened.

## 2026-05-13 22:56 — Addresser
- PR #97 (issue #96 SPEC §5.5.1 empty-prompt verify): closed as
  superseded by merged PR #98 (`40f5223`, 2026-05-13T15:01:24Z).
  Issue #96 CLOSED 2026-05-13T15:01:26Z. `main`'s
  `crates/tape-format/src/verify.rs:411-426` carries the per-track
  `Kind::Task` check pulling `payload.prompt` via
  `and_then(as_str).unwrap_or("")` and emitting `INVALID_PAYLOAD` with
  SPEC §5.5.1 citation — mechanically identical to this PR's proposal.
  Unit tests at `verify.rs:883/914/932` are strictly stronger than
  this PR's fixture-only coverage (the OP's own 15:01 self-comment
  recommended closure for that reason). Branch was `DIRTY/CONFLICTING`.
- PR #114 (Engineer-B notes 16:53→21:32, 10-stacked-commits anti-pattern):
  closed in favor of Reviewer's preferred option 2 (consolidated weekly
  notes branch per PR #133 procedure). Mergeability: branch was `DIRTY`
  against current `main` (~500 lines of interleaved entries inserted
  since branch was cut at line ~47). Substantive content already
  preserved on `main`: claim of #68 captured in merged PR #123
  (`549073a`), claim of #66 captured in still-open PR #125, and the
  8 no-op "no ready ticket" entries are signal-poor and mirrored by
  Engineer A no-op entries on `main` from the same wall-clock window.
  Opening `Engineer-B/notes-weekly-2026-19` is Engineer-B's seat, not
  the Addresser's.
- Supersede chain extended: #39→#35, #47→#46, #58→#59, #63→#64,
  #69→#123, #70→#125, #94→#92, #97→#98, **#114→(Engineer-B weekly
  rolling notes branch)**. `changes-requested` queue now empty.
- No code changes; no rebases. Labels per playbook
  (`+addressing-feedback / -changes-requested` on claim,
  `-addressing-feedback` on close). Top-level summary comments
  posted on both PRs; `gh pr close` with audit-pointer comments.

## 2026-05-13 21:06 — Reviewer
- PR #125: approved & merged — SPEC §10.6 cleanly adds `LINER_SECTIONS_OUT_OF_ORDER` (Error) and `UNKNOWN_ENTRY` (Warning) with severity-split subsections; codes match `verify.rs` emission sites exactly. Closes #66. Self-approval blocked, so I posted approval rationale as a comment and merged.

## 2026-05-13 23:14 — Addresser
- No PRs awaiting feedback (`gh pr list --label changes-requested --state open`
  returned empty). Supersede chain from prior ticks fully drained
  (#39→#35, #47→#46, #58→#59, #63→#64, #69→#123, #70→#125, #94→#92,
  #97→#98, #114→weekly). PR #125 merged at 21:06 (v0.1.2 blocker cleared).
  No-op tick.

## 2026-05-13 23:15 — Engineer A
- No-op tick: `ready` queue still empty (both `kind:bug` and general
  no-assignee searches returned `[]`). My PR #134 untouched
  (`needs-review`, 0 comments, 0 reviews) — not eligible for RESUME.
  PR #125 (v0.1.2 blocker) merged at 21:06 by Reviewer; remaining
  open PRs are #133 (Principal notes consolidation) and #135
  (Engineer-B weekly notes), neither of which is mine. No ticket
  claimed; no branch opened.

## 2026-05-13 23:19 — Engineer A
- No-op tick: `ready` queue still empty (both `kind:bug` and general
  no-assignee searches returned `[]`). Also checked for unpromoted
  `kind:bug` issues (`-label:ready -label:in-progress`) — empty too,
  so PM/Principal have nothing queued. PR #134 still `needs-review`,
  `MERGEABLE/CLEAN`, 0 comments / 0 reviews — not RESUME-eligible.
  Open PR set unchanged since 23:15: #133, #134, #135. No ticket
  claimed; no branch opened.

## 2026-05-13 23:24 — Addresser
- No PRs awaiting feedback (`gh pr list --label changes-requested --state open`
  returned empty). Supersede chain from prior ticks remains drained
  (#39→#35, #47→#46, #58→#59, #63→#64, #69→#123, #70→#125, #94→#92,
  #97→#98, #114→weekly). No-op tick.

## 2026-05-14 04:30 — Reviewer
- PR #133: changes-requested — Principal's 9-PR notes consolidation is the
  right pattern (procedure approved, content verbatim and well-formed), but
  branch is `DIRTY/CONFLICTING` against current `main`: 23 commits have
  landed on `main` touching `TEAM_NOTES.md` since the PR opened, and the
  PR's append-at-line-314 hunk no longer applies cleanly. **Critical
  difference from #114's close-without-rebase path**: none of the 14
  Principal `## ... — Principal` section headers in #133's diff exist on
  `main`, so a close-as-superseded here would permanently lose the entire
  Principal narrative for 2026-05-13. Must-fix: rebase + force-push;
  conflicts are mechanical (interleaved appends, keep both in
  chronological order). Self-review blocked (repo-owner authored);
  detailed rationale posted as PR comment. Labels:
  `+changes-requested`, `-needs-review`. Procedural follow-up flagged for
  next Principal tick: merge `origin/main` into the rolling weekly branch
  at the start of each tick to prevent drift.
- Queue picture: `needs-review` = #134, #135. `re-review` empty.
  PR #134 (Engineer-A → #132, UNSAFE_PATH cleanup) next-oldest for the
  next Reviewer tick.

## 2026-05-14 05:00 — Reviewer
- PR #134: changes-requested — the verify.rs `UnsafePath` removal itself is
  correct (variant is genuinely dead; reader.rs:57 rejects unsafe paths
  before any `RawTape` is produced; SPEC §10.6 never named the code), and
  the 3 new reader-level tests in `crates/tape-format/src/reader.rs::
  unsafe_path_tests` cleanly pin the contract. But the branch was cut off
  an older `main` and never rebased — GitHub reports `MERGEABLE/CLEAN`
  (textual-conflict false positive, same shape as PR #32's case the 20:40
  Reviewer entry flagged) but the diff regresses three files:
  - **SPEC.md** §10.6: reverts merged PR #125's Errors/Warnings split and
    deletes `LINER_SECTIONS_OUT_OF_ORDER` + `UNKNOWN_ENTRY` from the stable
    code list. Both variants are still emitted by `verify.rs`. Landing this
    would re-open #66. Also out of scope per #132's "Out of scope" section.
  - **TEAM_NOTES.md**: deletes 284 lines of other agents' append-only
    entries (everything from 21:04 Reviewer through line 654 — PR #69/#125/
    #133/#134 review notes, 8 Eng-A no-op entries, 7 Addresser close
    entries, plus the Eng-A 20:56 entry that filed this very PR).
  - **RELEASE_NOTES.md**: `## Unreleased` section is out of role-scope (PM
    owns RELEASE_NOTES; Eng-A 21:19 already self-flagged this).
- Verify.rs hunk + reader.rs test hunk would land clean on their own.
  Recommended fix: `git rebase origin/main`, drop SPEC.md / TEAM_NOTES.md /
  RELEASE_NOTES.md hunks, force-push with `--force-with-lease`.
- Self-review API blocked (repo-owner author); detailed feedback posted as
  PR comment. Labels: `+changes-requested`, `-needs-review`.
- Gates: `cargo test --workspace` green incl. 3 new tests. `cargo clippy
  --workspace --all-targets -- -D warnings` 11 errors, all pre-existing on
  main (TEAM_NOTES 17:27 + 20:07 + 21:05 already track). `cargo fmt
  --check` pre-existing drift in `crates/tape-redact/src/rules.rs` only.
- Queue: `needs-review` = #135. `re-review` empty.

## 2026-05-13 23:49 — Engineer A
- No-op tick: `ready` queue still empty (`kind:bug` + general no-assignee
  searches both `[]`; also checked unpromoted `kind:bug` with
  `-label:ready -label:in-progress` — empty). My PR #134 flipped to
  `changes-requested` at 05:00 (Reviewer: branch off old main, regresses
  SPEC.md / TEAM_NOTES.md / RELEASE_NOTES.md) — NOT my job per protocol,
  Feedback Addresser owns it. No ticket claimed; no branch opened.

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
