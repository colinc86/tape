//! Text reporter for `tape doctor`. Phase 1 ships text-only; JSON is phase 2.

use std::fmt::Write;

use super::catalog;
use super::check::Status;
use super::runner::{Report, Summary};

/// Render the human-readable report. `color` controls ANSI escapes; the CLI
/// passes `false` if `--no-color` or `$NO_COLOR` is set.
pub fn render_text(report: &Report, color: bool, quiet: bool) -> String {
    let mut out = String::new();
    let version = env!("CARGO_PKG_VERSION");
    let _ = writeln!(out, "tape doctor — v{version}");
    let _ = writeln!(out);

    // Compute the column widths used by the per-row alignment. Padding the
    // id column makes the descriptions line up across categories.
    let id_width = report
        .results
        .iter()
        .map(|r| r.id.len())
        .max()
        .unwrap_or(0)
        .max(20);

    for cat in catalog::PHASE_1_CATEGORIES {
        let mut wrote_header = false;
        for r in &report.results {
            if r.category != *cat {
                continue;
            }
            if quiet && r.outcome.status == Status::Pass {
                continue;
            }
            if !wrote_header {
                let _ = writeln!(out, "{cat}");
                wrote_header = true;
            }
            render_row(&mut out, r, id_width, color);
        }
        if wrote_header {
            let _ = writeln!(out);
        }
    }

    render_summary(&mut out, &report.summary, color);
    out
}

fn render_row(out: &mut String, r: &super::runner::CheckResult, id_width: usize, color: bool) {
    let glyph = colorized(color, r.outcome.status, r.outcome.status.glyph());
    let _ = writeln!(
        out,
        "  {glyph} {id:<width$}  {msg}",
        id = r.id,
        width = id_width,
        msg = r.outcome.message,
    );
    if r.outcome.status != Status::Pass && r.outcome.status != Status::Na {
        if let Some(fix) = &r.outcome.suggested_fix {
            // Indent under the glyph to make the "fix:" line a clear
            // continuation of the row above.
            let _ = writeln!(out, "        fix: {fix}");
        }
    }
}

fn render_summary(out: &mut String, s: &Summary, _color: bool) {
    let _ = writeln!(
        out,
        "summary  {pass} pass · {warn} warn · {fail} fail · {na} n/a    exit {exit}",
        pass = s.pass,
        warn = s.warn,
        fail = s.fail,
        na = s.na,
        exit = s.exit_code(),
    );
}

fn colorized(color: bool, status: Status, glyph: &str) -> String {
    if !color {
        return glyph.to_owned();
    }
    let code: &str = match status {
        Status::Pass => "32",                // green
        Status::Warn => "33",                // yellow
        Status::Fail => "31",                // red
        Status::Na | Status::Harness => "2", // dim
    };
    format!("\x1b[{code}m{glyph}\x1b[0m")
}

/// Render the `--list-checks` enumeration. One row per check, tab-separated.
pub fn render_catalog_listing() -> String {
    let mut out = String::new();
    for check in catalog::phase_1_checks() {
        let _ = writeln!(
            out,
            "{id}\t{cat}\t{sev}\t{desc}",
            id = check.id(),
            cat = check.category(),
            sev = check.severity_on_fail().as_str(),
            desc = check.description(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::check::CheckOutcome;
    use crate::doctor::runner::{CheckResult, Report, Summary};

    fn synth_report() -> Report {
        let mut summary = Summary::default();
        let mut mk = |id, cat, outcome: CheckOutcome| {
            summary.count(outcome.status);
            CheckResult {
                id,
                category: cat,
                description: "synthetic",
                outcome,
            }
        };
        let results = vec![
            mk(
                "binary.tape.present",
                "binary",
                CheckOutcome::pass("on path"),
            ),
            mk(
                "config.user_taperc.parses",
                "config",
                CheckOutcome::fail("boom").with_fix("do x"),
            ),
        ];
        Report { results, summary }
    }

    #[test]
    fn text_render_includes_glyphs_and_fix_line() {
        let report = synth_report();
        let text = render_text(&report, false, false);
        assert!(text.contains("[OK]"));
        assert!(text.contains("[XX]"));
        assert!(text.contains("fix: do x"));
        assert!(text.contains("summary"));
        assert!(text.contains("exit 1"));
    }

    #[test]
    fn quiet_suppresses_pass_lines() {
        let report = synth_report();
        let text = render_text(&report, false, true);
        assert!(!text.contains("binary.tape.present"));
        assert!(text.contains("config.user_taperc.parses"));
    }

    #[test]
    fn no_color_strips_ansi() {
        let report = synth_report();
        let text = render_text(&report, false, false);
        assert!(!text.contains("\x1b["));
    }

    #[test]
    fn catalog_listing_has_one_line_per_check() {
        let s = render_catalog_listing();
        // Doctor catalog has 20 entries (phase 1 + #163 claude-code +
        // #166 signing + #177 pricing + #183 index + #186 pricing-
        // config). If you add one, update `list_checks_is_stable` in
        // `tests/doctor_integration.rs` first.
        assert_eq!(s.lines().count(), 20);
        for line in s.lines() {
            assert_eq!(line.split('\t').count(), 4);
        }
    }
}
