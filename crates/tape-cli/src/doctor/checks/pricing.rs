//! `pricing.*` — bundled-pricing-table freshness check. Issue #177 /
//! Step-4 of #81.
//!
//! One check: `pricing.table.fresh`. `Warn` severity; the pricing
//! table is *compiled into the binary*, so there is no "feature not
//! in use" branch — the check always runs and surfaces either `Pass`,
//! `Warn` (stale), or `Harness` (an impossible-in-practice parse
//! failure that the doctor refuses to panic on).
//!
//! No `Env` fields consumed — the input is the compile-time
//! `PRICING_TABLE_LAST_UPDATED` const and `SystemTime::now()`. The
//! check completes in <1 ms.
//!
//! Date parsing is intentionally inlined rather than imported from
//! `tape-play::chrono_lite` (private module): keeps the slice's
//! blast radius inside `crates/tape-cli/` and matches the issue's
//! out-of-scope carve-out about not touching `tape-play`.

use tape_play::pricing::{PRICING_STALENESS_DAYS, PRICING_TABLE_LAST_UPDATED};

use super::super::check::{Check, CheckOutcome, Env, Severity};

pub struct TableFresh;
impl Check for TableFresh {
    fn id(&self) -> &'static str {
        "pricing.table.fresh"
    }
    fn category(&self) -> &'static str {
        "pricing"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "bundled pricing table is within PRICING_STALENESS_DAYS of today (drives `tape stats --with-cost`)"
    }
    fn run(&self, _env: &Env) -> CheckOutcome {
        let Some(updated) = parse_ymd(PRICING_TABLE_LAST_UPDATED) else {
            return CheckOutcome::harness(format!(
                "pricing table date '{PRICING_TABLE_LAST_UPDATED}' is unparseable"
            ));
        };
        let Some(today) = today_days_since_epoch() else {
            return CheckOutcome::harness("system clock is before 1970-01-01");
        };
        run_with_days(today, updated)
    }
}

/// Testable seam. Pure arithmetic on day counts so the unit tests do
/// not bit-rot relative to wall-clock time.
fn run_with_days(today: i64, updated: i64) -> CheckOutcome {
    let age = today - updated;
    if age > PRICING_STALENESS_DAYS {
        CheckOutcome::warn(format!(
            "bundled pricing table is {age} days old (>{PRICING_STALENESS_DAYS} day threshold); \
             cost figures from `tape stats --with-cost` may be stale"
        ))
    } else {
        CheckOutcome::pass(format!(
            "bundled pricing table is {age} days old (\u{2264}{PRICING_STALENESS_DAYS} day threshold)"
        ))
    }
}

/// Parse a `YYYY-MM-DD` string to days-since-Unix-epoch. Mirrors the
/// shape of `tape-play::chrono_lite::parse_date` (private there);
/// inlined here to keep the slice inside `crates/tape-cli/`.
fn parse_ymd(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Howard Hinnant's days-from-civil-date algorithm. Same body as
/// `tape-play::chrono_lite::days_from_civil`; both implementations
/// return identical day counts for any valid civil date.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let d = i64::from(d);
    let doy = (153 * if m > 2 { m - 3 } else { m + 9 } + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn today_days_since_epoch() -> Option<i64> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let secs = i64::try_from(secs).ok()?;
    Some(secs / 86_400)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::check::Status;

    const FAKE_UPDATED: i64 = 20_000; // arbitrary day-count anchor

    #[test]
    fn pass_when_table_is_fresh() {
        let out = run_with_days(FAKE_UPDATED + 1, FAKE_UPDATED);
        assert_eq!(out.status, Status::Pass);
        assert!(out.message.contains("1 days old"), "{}", out.message);
    }

    #[test]
    fn warn_when_table_is_stale() {
        let out = run_with_days(FAKE_UPDATED + PRICING_STALENESS_DAYS + 1, FAKE_UPDATED);
        assert_eq!(out.status, Status::Warn);
        assert!(
            out.message
                .contains(&format!("{} days old", PRICING_STALENESS_DAYS + 1)),
            "{}",
            out.message
        );
        assert!(out.message.contains("may be stale"), "{}", out.message);
    }

    #[test]
    fn pass_at_exact_threshold() {
        // Boundary: `≤` not `<`. The pricing.rs in-source test
        // `stale_check_does_not_fire_at_90_days` pins the same edge.
        let out = run_with_days(FAKE_UPDATED + PRICING_STALENESS_DAYS, FAKE_UPDATED);
        assert_eq!(out.status, Status::Pass);
    }

    #[test]
    fn pass_when_clock_is_in_the_past() {
        // System clock predates the table — negative age. Treated as
        // not-stale: clock-skew is a separate concern, not pricing's.
        let out = run_with_days(FAKE_UPDATED - 1, FAKE_UPDATED);
        assert_eq!(out.status, Status::Pass);
    }

    #[test]
    fn parse_ymd_rejects_malformed_input() {
        assert!(parse_ymd("not a date").is_none());
        assert!(parse_ymd("2026/05/15").is_none());
        assert!(parse_ymd("2026-13-01").is_none());
        assert!(parse_ymd("2026-05-32").is_none());
        // Sanity: a well-formed date parses.
        assert!(parse_ymd("2026-05-15").is_some());
    }

    #[test]
    fn harness_when_helper_receives_unparseable_date() {
        // We can't force `PRICING_TABLE_LAST_UPDATED` to be malformed
        // without source edits, but the helper's malformed-input branch
        // is what the `run()` wiring delegates to. Exercise it directly.
        assert!(parse_ymd("malformed").is_none());
    }
}
