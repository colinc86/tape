//! Doctor runner: filters checks, runs them, and aggregates outcomes.
//!
//! `CheckResult::description` is unused in phase 1 (the report renders
//! `outcome.message`, not the description) but lands in phase-2 JSON output.
#![allow(dead_code)]

use super::catalog;
use super::check::{Check, CheckOutcome, Env, Status};

/// One row in the doctor report.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub id: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub outcome: CheckOutcome,
}

#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub pass: usize,
    pub warn: usize,
    pub fail: usize,
    pub na: usize,
    pub harness: usize,
}

impl Summary {
    pub fn count(&mut self, status: Status) {
        match status {
            Status::Pass => self.pass += 1,
            Status::Warn => self.warn += 1,
            Status::Fail => self.fail += 1,
            Status::Na => self.na += 1,
            Status::Harness => self.harness += 1,
        }
    }

    /// Per the principal's scoping comment: 0 (all pass; warns ok),
    /// 1 (≥1 fail), 3 (harness). `--strict` (phase 2) will add exit 2.
    pub fn exit_code(&self) -> i32 {
        if self.fail > 0 {
            1
        } else if self.harness > 0 {
            3
        } else {
            0
        }
    }
}

/// What the user asked to run. Empty include/exclude/select lists mean
/// "no filtering on this axis".
#[derive(Debug, Clone, Default)]
pub struct RunFilter {
    pub select_ids: Vec<String>,
    pub include_categories: Vec<String>,
    pub exclude_categories: Vec<String>,
}

impl RunFilter {
    pub fn keeps(&self, check: &dyn Check) -> bool {
        if !self.select_ids.is_empty() && !self.select_ids.iter().any(|i| i == check.id()) {
            return false;
        }
        if !self.include_categories.is_empty()
            && !self
                .include_categories
                .iter()
                .any(|c| c == check.category())
        {
            return false;
        }
        if self
            .exclude_categories
            .iter()
            .any(|c| c == check.category())
        {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub results: Vec<CheckResult>,
    pub summary: Summary,
}

/// Run every catalog entry that survives the filter.
pub fn run(env: &Env, filter: &RunFilter) -> Report {
    let mut results = Vec::new();
    let mut summary = Summary::default();
    for check in catalog::phase_1_checks() {
        if !filter.keeps(check.as_ref()) {
            continue;
        }
        let outcome = check.run(env);
        summary.count(outcome.status);
        results.push(CheckResult {
            id: check.id(),
            category: check.category(),
            description: check.description(),
            outcome,
        });
    }
    Report { results, summary }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::check::Severity;

    struct Dummy {
        id: &'static str,
        cat: &'static str,
    }
    impl Check for Dummy {
        fn id(&self) -> &'static str {
            self.id
        }
        fn category(&self) -> &'static str {
            self.cat
        }
        fn severity_on_fail(&self) -> Severity {
            Severity::Fail
        }
        fn description(&self) -> &'static str {
            "dummy"
        }
        fn run(&self, _env: &Env) -> CheckOutcome {
            CheckOutcome::pass("ok")
        }
    }

    #[test]
    fn filter_select_ids_matches_exactly_one() {
        let f = RunFilter {
            select_ids: vec!["a".into()],
            ..Default::default()
        };
        assert!(f.keeps(&Dummy { id: "a", cat: "x" }));
        assert!(!f.keeps(&Dummy { id: "b", cat: "x" }));
    }

    #[test]
    fn filter_include_category_narrows() {
        let f = RunFilter {
            include_categories: vec!["binary".into()],
            ..Default::default()
        };
        assert!(f.keeps(&Dummy {
            id: "binary.x",
            cat: "binary"
        }));
        assert!(!f.keeps(&Dummy {
            id: "config.x",
            cat: "config"
        }));
    }

    #[test]
    fn filter_exclude_category_drops() {
        let f = RunFilter {
            exclude_categories: vec!["config".into()],
            ..Default::default()
        };
        assert!(f.keeps(&Dummy {
            id: "binary.x",
            cat: "binary"
        }));
        assert!(!f.keeps(&Dummy {
            id: "config.x",
            cat: "config"
        }));
    }

    #[test]
    fn summary_exit_code_priorities() {
        let s = Summary {
            fail: 1,
            harness: 1,
            ..Default::default()
        };
        assert_eq!(s.exit_code(), 1, "fail beats harness");
        let s = Summary {
            harness: 1,
            ..Default::default()
        };
        assert_eq!(s.exit_code(), 3);
        let s = Summary {
            warn: 1,
            pass: 5,
            ..Default::default()
        };
        assert_eq!(s.exit_code(), 0, "warn alone does not trip exit");
        let s = Summary::default();
        assert_eq!(s.exit_code(), 0);
    }

    #[test]
    fn summary_count_tallies_each_status() {
        let mut s = Summary::default();
        s.count(Status::Pass);
        s.count(Status::Pass);
        s.count(Status::Warn);
        s.count(Status::Fail);
        s.count(Status::Na);
        s.count(Status::Harness);
        assert_eq!(s.pass, 2);
        assert_eq!(s.warn, 1);
        assert_eq!(s.fail, 1);
        assert_eq!(s.na, 1);
        assert_eq!(s.harness, 1);
    }
}
