//! Bundled pricing table for `tape stats --with-cost`. Step-3 of #31
//! (issue #168).
//!
//! This is a **hand-maintained** snapshot of public per-million-token
//! list prices for a small set of common models. The intent is to
//! give users a real cost number in front of them without any
//! configuration surface; per-model breakdowns, cache pricing, and
//! user-supplied overrides land in Step 4 alongside `--pricing-file`.
//!
//! ## Refresh discipline
//!
//! Bump [`PRICING_TABLE_LAST_UPDATED`] whenever any entry is touched.
//! Refresh ≤90 days before each minor release — `tape stats
//! --with-cost` emits a stale-guard warning when the table is older
//! than 90 days. Vendor pricing pages are the source of truth; do
//! not extrapolate or estimate when bumping.

use serde_json::Value;

/// One row in the bundled pricing table. Per-million-token rates in
/// USD; matches the `OpenAI` / `Anthropic` list-pricing format every
/// vendor uses. Cache-read / cache-write dimensions are deliberately
/// absent for this slice — they land with the per-model breakdown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPrice {
    /// `model_call.payload.vendor` value to match against. Lowercase,
    /// matches the convention SPEC §5.5.2 sets.
    pub vendor: &'static str,
    /// `model_call.payload.model` value to match against. Vendor's
    /// canonical model id, no aliases.
    pub model: &'static str,
    /// USD per million input tokens.
    pub input_per_mtok: f64,
    /// USD per million output tokens.
    pub output_per_mtok: f64,
}

/// Hand-maintained table. Order is documentation only — `lookup_price`
/// does a linear scan, so adding or removing entries is a one-line
/// edit. Picked from what current fixtures and recent recordings
/// actually exercise so the cost column has values to print.
pub const PRICING_TABLE: &[ModelPrice] = &[
    ModelPrice {
        vendor: "anthropic",
        model: "claude-opus-4-7",
        input_per_mtok: 15.00,
        output_per_mtok: 75.00,
    },
    ModelPrice {
        vendor: "anthropic",
        model: "claude-sonnet-4-5",
        input_per_mtok: 3.00,
        output_per_mtok: 15.00,
    },
    ModelPrice {
        vendor: "anthropic",
        model: "claude-haiku-4-5",
        input_per_mtok: 1.00,
        output_per_mtok: 5.00,
    },
    ModelPrice {
        vendor: "openai",
        model: "gpt-5",
        input_per_mtok: 5.00,
        output_per_mtok: 25.00,
    },
];

/// ISO-8601 date (YYYY-MM-DD) the table above was last verified
/// against vendor pricing pages. Bump whenever any entry is touched.
/// Consumed by [`crate::cost_total`]'s stale-guard.
pub const PRICING_TABLE_LAST_UPDATED: &str = "2026-05-15";

/// Threshold beyond which the bundled table is considered stale.
/// 90 days is conservative — vendor pricing churn cadence in
/// practice — and matches the §3.7 design from #31.
pub const PRICING_STALENESS_DAYS: i64 = 90;

/// Lookup. Linear scan of [`PRICING_TABLE`]; the table is small
/// enough that hashing is overkill (and would require a runtime
/// `OnceLock` construction). Returns `None` for any pair not in
/// the table; the caller (see `cost_total`) routes unpriced events
/// into the `total - priced` counter.
pub fn lookup_price(vendor: &str, model: &str) -> Option<&'static ModelPrice> {
    PRICING_TABLE
        .iter()
        .find(|p| p.vendor == vendor && p.model == model)
}

/// Per-event price helper. Given a `model_call.payload` JSON, returns
/// `Some((vendor, model, dollars))` when all four of
/// `vendor` / `model` / `tokens_in` / `tokens_out` are present AND the
/// vendor/model pair is in [`PRICING_TABLE`]. Returns `None` otherwise.
/// The dollar value is **not** rounded — rounding happens once at the
/// rendered-output boundary so accumulation across many events doesn't
/// compound a per-event rounding error.
pub fn price_event(payload: &Value) -> Option<(&'static str, &'static str, f64)> {
    let vendor = payload.get("vendor").and_then(Value::as_str)?;
    let model = payload.get("model").and_then(Value::as_str)?;
    let tokens_in = payload.get("tokens_in").and_then(Value::as_u64)?;
    let tokens_out = payload.get("tokens_out").and_then(Value::as_u64)?;
    let price = lookup_price(vendor, model)?;
    // `tokens_in` / `tokens_out` are `u64`; in practice they stay well
    // under `2^53` (a single trillion-token call would still fit
    // losslessly), so `as f64` precision loss is not reachable here.
    #[allow(clippy::cast_precision_loss)]
    let dollars = ((tokens_in as f64) * price.input_per_mtok
        + (tokens_out as f64) * price.output_per_mtok)
        / 1_000_000.0;
    Some((price.vendor, price.model, dollars))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_non_empty() {
        assert!(!PRICING_TABLE.is_empty());
    }

    #[test]
    fn lookup_finds_known_pair() {
        let p = lookup_price("anthropic", "claude-opus-4-7").expect("opus entry");
        assert!(p.input_per_mtok > 0.0);
        assert!(p.output_per_mtok > 0.0);
    }

    #[test]
    fn lookup_misses_unknown_pair() {
        assert!(lookup_price("anthropic", "no-such-model").is_none());
        assert!(lookup_price("not-a-vendor", "claude-opus-4-7").is_none());
    }

    #[test]
    fn last_updated_is_a_calendar_date() {
        // Format is `YYYY-MM-DD` with hyphens at positions 4 and 7.
        // We don't pull in chrono here; the rendered output's stale-
        // guard does the actual parse via `chrono_lite::parse_date`.
        // This is the spelling-check at write-time.
        let s = PRICING_TABLE_LAST_UPDATED;
        assert_eq!(s.len(), 10, "{s}");
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert!(s[0..4].chars().all(|c| c.is_ascii_digit()));
        assert!(s[5..7].chars().all(|c| c.is_ascii_digit()));
        assert!(s[8..10].chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn price_event_computes_dollars() {
        // 1M input tokens at $15/Mtok + 100k output tokens at $75/Mtok
        // = $15.00 + $7.50 = $22.50 for opus.
        let payload = serde_json::json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "tokens_in": 1_000_000_u64,
            "tokens_out": 100_000_u64,
        });
        let (v, m, dollars) = price_event(&payload).expect("priceable");
        assert_eq!(v, "anthropic");
        assert_eq!(m, "claude-opus-4-7");
        assert!((dollars - 22.50).abs() < 0.0001, "got {dollars}");
    }

    #[test]
    fn price_event_returns_none_when_pricing_absent() {
        let payload = serde_json::json!({
            "vendor": "anthropic",
            "model": "no-such-model",
            "tokens_in": 100_u64,
            "tokens_out": 50_u64,
        });
        assert!(price_event(&payload).is_none());
    }

    #[test]
    fn price_event_returns_none_when_tokens_missing() {
        let payload = serde_json::json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            // No tokens_in / tokens_out.
        });
        assert!(price_event(&payload).is_none());
    }
}
