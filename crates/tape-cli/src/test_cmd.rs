//! `tape test` Phase 1 (issue #252, carved from #10). Pure
//! structural comparator: four independent checks over a baseline
//! cassette and a candidate cassette. The IO shell in
//! `crates/tape-cli/src/main.rs::cmd_test` loads both, calls
//! [`compare`], renders the report, and sets the exit code.
//!
//! Phase 1 deliberately does NOT call `tape_diff::compute` — the
//! diff aligner's LCS-based insertion/deletion tolerance is the
//! wrong oracle for a regression check. We want strict structural
//! equality (track count, kind-by-kind sequence, verbatim task,
//! verbatim outcome). The four checks are independent: even if the
//! track count differs, the kind-sequence prefix is still compared
//! so the user sees both signals in one report.

use tape_format::meta::Meta;
use tape_format::tracks::{Kind, Track};

/// Outcome of one structural check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub passed: bool,
    /// Empty when `passed`; one short line of detail when not.
    pub detail: String,
}

impl CheckResult {
    pub fn pass() -> Self {
        Self {
            passed: true,
            detail: String::new(),
        }
    }
    pub fn fail(detail: impl Into<String>) -> Self {
        Self {
            passed: false,
            detail: detail.into(),
        }
    }
}

/// Aggregate report — one `CheckResult` per structural field.
/// Field order matches the order checks run + render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestReport {
    pub track_count: CheckResult,
    pub kind_sequence: CheckResult,
    pub task_prompt: CheckResult,
    pub eject_outcome: CheckResult,
}

impl TestReport {
    /// Iterator of `(name, &CheckResult)` in display order so the
    /// renderer + the summary counter agree on what "all four"
    /// means.
    fn items(&self) -> [(&'static str, &CheckResult); 4] {
        [
            ("track count", &self.track_count),
            ("kind sequence", &self.kind_sequence),
            ("task prompt", &self.task_prompt),
            ("eject outcome", &self.eject_outcome),
        ]
    }

    pub fn all_passed(&self) -> bool {
        self.items().iter().all(|(_, r)| r.passed)
    }

    pub fn passed_count(&self) -> usize {
        self.items().iter().filter(|(_, r)| r.passed).count()
    }
}

/// Run the four Phase-1 structural checks. Pure function — no IO,
/// no panic on input shape (relies only on already-parsed `Meta`
/// and `Vec<Track>`).
#[must_use]
pub fn compare(a_meta: &Meta, a_tracks: &[Track], b_meta: &Meta, b_tracks: &[Track]) -> TestReport {
    TestReport {
        track_count: check_track_count(a_tracks, b_tracks),
        kind_sequence: check_kind_sequence(a_tracks, b_tracks),
        task_prompt: check_task_prompt(a_meta, b_meta),
        eject_outcome: check_eject_outcome(a_meta, b_meta),
    }
}

fn check_track_count(a: &[Track], b: &[Track]) -> CheckResult {
    if a.len() == b.len() {
        CheckResult::pass()
    } else {
        CheckResult::fail(format!("a={}, b={}", a.len(), b.len()))
    }
}

fn check_kind_sequence(a: &[Track], b: &[Track]) -> CheckResult {
    // Walk the shorter of the two so a track-count mismatch
    // doesn't suppress the kind-sequence signal. The two checks
    // report independently.
    let n = a.len().min(b.len());
    for i in 0..n {
        if a[i].kind != b[i].kind {
            return CheckResult::fail(format!(
                "first divergence at index {i}: a={}, b={}",
                kind_name(a[i].kind),
                kind_name(b[i].kind),
            ));
        }
    }
    CheckResult::pass()
}

fn check_task_prompt(a: &Meta, b: &Meta) -> CheckResult {
    if a.task == b.task {
        CheckResult::pass()
    } else {
        CheckResult::fail(format!(
            "a={:?}, b={:?}",
            truncate(&a.task, 60),
            truncate(&b.task, 60),
        ))
    }
}

fn check_eject_outcome(a: &Meta, b: &Meta) -> CheckResult {
    if a.outcome == b.outcome {
        CheckResult::pass()
    } else {
        // Mirror `tape_diff::compute`'s outcome rendering at
        // crates/tape-diff/src/lib.rs:160 — `{:?}` then lowercased
        // — so report strings agree with `tape diff` output.
        CheckResult::fail(format!(
            "a={}, b={}",
            outcome_str(a.outcome),
            outcome_str(b.outcome),
        ))
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Task => "task",
        Kind::ModelCall => "model_call",
        Kind::McpCall => "mcp_call",
        Kind::Shell => "shell",
        Kind::FileRead => "file_read",
        Kind::FileWrite => "file_write",
        Kind::Annotation => "annotation",
        Kind::Eject => "eject",
    }
}

fn outcome_str(o: tape_format::meta::Outcome) -> String {
    format!("{o:?}").to_lowercase()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_owned();
    }
    let head: String = s.chars().take(n).collect();
    format!("{head}…")
}

/// Render the report to plain text. One `PASS` / `FAIL` line per
/// check (with the failing check's detail in parens) plus a
/// trailing summary line.
#[must_use]
pub fn render_report(report: &TestReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for (name, r) in report.items() {
        let verdict = if r.passed { "PASS" } else { "FAIL" };
        if r.passed {
            let _ = writeln!(out, "{name}: {verdict}");
        } else {
            let _ = writeln!(out, "{name}: {verdict} ({})", r.detail);
        }
    }
    let _ = writeln!(out, "{}/4 passed", report.passed_count());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tape_format::meta::{Meta, Outcome, Recorder};
    use tape_format::tracks::Track;

    fn base_meta() -> Meta {
        Meta {
            tape_version: "tape/v0".to_owned(),
            id: "x".to_owned(),
            created_at: "2026-05-16T00:00:00Z".to_owned(),
            ejected_at: "2026-05-16T00:00:01Z".to_owned(),
            task: "investigate billing".to_owned(),
            recorder: Recorder {
                agent: "test/0".to_owned(),
                user: None,
            },
            outcome: Outcome::Success,
            models: Vec::new(),
            tools: Vec::new(),
            tool_budget: None,
            redaction_summary: None,
            label: None,
            recap: None,
            recaps: Vec::new(),
            tags: Vec::new(),
            relinernotes: Vec::new(),
            compactions: Vec::new(),
            new_block: None,
        }
    }

    fn t(step: u64, kind: Kind) -> Track {
        Track {
            step,
            kind,
            ts: format!("2026-05-16T00:00:{step:02}Z"),
            payload: json!({}),
            parent_step: None,
            refs: Vec::new(),
            annotations: Vec::new(),
        }
    }

    fn standard_tracks() -> Vec<Track> {
        vec![
            t(1, Kind::Task),
            t(2, Kind::ModelCall),
            t(3, Kind::Shell),
            t(4, Kind::Eject),
        ]
    }

    #[test]
    fn all_four_pass_on_identical_inputs() {
        let m = base_meta();
        let tracks = standard_tracks();
        let report = compare(&m, &tracks, &m, &tracks);
        assert!(report.all_passed());
        assert_eq!(report.passed_count(), 4);
    }

    #[test]
    fn track_count_fails_independently() {
        let m = base_meta();
        let a = standard_tracks();
        let mut b = a.clone();
        b.push(t(5, Kind::Eject));
        let report = compare(&m, &a, &m, &b);
        assert!(!report.track_count.passed);
        assert!(report.track_count.detail.contains("a=4"));
        assert!(report.track_count.detail.contains("b=5"));
        // Other checks still ran over the prefix and passed.
        assert!(report.kind_sequence.passed);
        assert!(report.task_prompt.passed);
        assert!(report.eject_outcome.passed);
    }

    #[test]
    fn kind_sequence_fails_with_first_divergence_index() {
        let m = base_meta();
        let a = standard_tracks();
        let mut b = a.clone();
        b[2].kind = Kind::McpCall;
        let report = compare(&m, &a, &m, &b);
        assert!(!report.kind_sequence.passed);
        assert!(report.kind_sequence.detail.contains("index 2"));
        assert!(report.kind_sequence.detail.contains("a=shell"));
        assert!(report.kind_sequence.detail.contains("b=mcp_call"));
    }

    #[test]
    fn task_prompt_fails_with_truncated_diff() {
        let mut a = base_meta();
        let mut b = base_meta();
        a.task = "investigate billing".to_owned();
        b.task = "investigate inventory".to_owned();
        let report = compare(&a, &standard_tracks(), &b, &standard_tracks());
        assert!(!report.task_prompt.passed);
        assert!(report.task_prompt.detail.contains("investigate billing"));
        assert!(report.task_prompt.detail.contains("investigate inventory"));
    }

    #[test]
    fn task_prompt_truncates_long_strings_with_ellipsis() {
        let mut a = base_meta();
        let mut b = base_meta();
        a.task = "x".repeat(200);
        b.task = "y".repeat(200);
        let report = compare(&a, &standard_tracks(), &b, &standard_tracks());
        assert!(!report.task_prompt.passed);
        assert!(report.task_prompt.detail.contains('…'));
    }

    #[test]
    fn eject_outcome_fails_with_lowercase_enum_strings() {
        let mut a = base_meta();
        let mut b = base_meta();
        a.outcome = Outcome::Success;
        b.outcome = Outcome::Failure;
        let report = compare(&a, &standard_tracks(), &b, &standard_tracks());
        assert!(!report.eject_outcome.passed);
        assert_eq!(report.eject_outcome.detail, "a=success, b=failure");
    }

    #[test]
    fn render_report_emits_pass_lines_and_summary_on_all_pass() {
        let report = TestReport {
            track_count: CheckResult::pass(),
            kind_sequence: CheckResult::pass(),
            task_prompt: CheckResult::pass(),
            eject_outcome: CheckResult::pass(),
        };
        let out = render_report(&report);
        assert!(out.contains("track count: PASS"));
        assert!(out.contains("kind sequence: PASS"));
        assert!(out.contains("task prompt: PASS"));
        assert!(out.contains("eject outcome: PASS"));
        assert!(out.contains("4/4 passed"));
    }

    #[test]
    fn render_report_carries_detail_on_fail() {
        let report = TestReport {
            track_count: CheckResult::fail("a=4, b=5"),
            kind_sequence: CheckResult::pass(),
            task_prompt: CheckResult::pass(),
            eject_outcome: CheckResult::pass(),
        };
        let out = render_report(&report);
        assert!(out.contains("track count: FAIL (a=4, b=5)"));
        assert!(out.contains("3/4 passed"));
    }
}
