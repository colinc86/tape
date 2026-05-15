//! Bundled pricing table for `tape stats --with-cost`. Step-3 of #31
//! (issue #168) shipped the bundled `&'static`-backed table; Step-4
//! (issue #181) layered `PricingTable` + `--pricing-file <PATH>` over
//! it so a user can swap in a TOML-loaded table for a single
//! invocation.
//!
//! This is a **hand-maintained** snapshot of public per-million-token
//! list prices for a small set of common models. The intent is to
//! give users a real cost number in front of them without any
//! configuration surface; per-model breakdowns and cache pricing are
//! still deferred.
//!
//! ## Refresh discipline
//!
//! Bump [`PRICING_TABLE_LAST_UPDATED`] whenever any entry is touched.
//! Refresh ≤90 days before each minor release — `tape stats
//! --with-cost` emits a stale-guard warning when the table is older
//! than 90 days. Vendor pricing pages are the source of truth; do
//! not extrapolate or estimate when bumping.

use std::path::{Path, PathBuf};

use serde::Deserialize;
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

/// Owned counterpart to [`ModelPrice`] for tables loaded at runtime
/// (e.g. via `tape stats --pricing-file`). Same per-million-token
/// shape; vendor / model are `String` so the file's bytes own them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OwnedModelPrice {
    pub vendor: String,
    pub model: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

impl OwnedModelPrice {
    fn from_bundled(p: &ModelPrice) -> Self {
        Self {
            vendor: p.vendor.to_owned(),
            model: p.model.to_owned(),
            input_per_mtok: p.input_per_mtok,
            output_per_mtok: p.output_per_mtok,
        }
    }
}

/// A pricing table the cost code can consult — either the bundled
/// `&'static` snapshot or a user-supplied file. Two methods mirror
/// the existing free-function pair (`lookup` / `price_event`) so
/// callers can swap one for the other without restructuring.
///
/// `source_path` is `Some(_)` when this table came from
/// [`PricingTable::load_from_file`], used by the cost-block renderer
/// to name the user's file in any stale-guard warning. `None` for
/// the bundled table.
#[derive(Debug, Clone)]
pub struct PricingTable {
    pub rows: Vec<OwnedModelPrice>,
    /// `YYYY-MM-DD` date the rows were last verified against vendor
    /// pricing pages. For the bundled table this mirrors
    /// [`PRICING_TABLE_LAST_UPDATED`]; for a loaded table it's the
    /// TOML's `last_updated` field.
    pub last_updated: String,
    pub source_path: Option<PathBuf>,
}

impl PricingTable {
    /// The hand-maintained snapshot, materialised as owned rows.
    /// Zero IO. Consumed by [`crate::cost_total`] when
    /// `--pricing-file` is absent.
    #[must_use]
    pub fn bundled() -> Self {
        Self {
            rows: PRICING_TABLE
                .iter()
                .map(OwnedModelPrice::from_bundled)
                .collect(),
            last_updated: PRICING_TABLE_LAST_UPDATED.to_owned(),
            source_path: None,
        }
    }

    /// Load a `PricingTable` from a TOML file. Schema is documented
    /// in issue #181: top-level `last_updated = "YYYY-MM-DD"` and one
    /// or more `[[model]]` arrays with `vendor` / `model` /
    /// `input_per_mtok` / `output_per_mtok`. Every failure is
    /// reported via [`PricingLoadError`]; no panics, no partial
    /// state. Replace-not-merge: the returned table is the only one
    /// the caller consults for the invocation; the bundled table is
    /// not consulted for misses.
    pub fn load_from_file(path: &Path) -> Result<Self, PricingLoadError> {
        let bytes = std::fs::read(path).map_err(|e| PricingLoadError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        if bytes.is_empty() {
            return Err(PricingLoadError::Empty {
                path: path.to_path_buf(),
            });
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| PricingLoadError::ParseToml {
            path: path.to_path_buf(),
            message: "file is not valid UTF-8".to_owned(),
        })?;
        let parsed: LoadedPricingFile =
            toml::from_str(text).map_err(|e| PricingLoadError::ParseToml {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?;
        if parsed.model.is_empty() {
            return Err(PricingLoadError::NoRows {
                path: path.to_path_buf(),
            });
        }
        // Validate prices. We accept zero (a free / preview tier is
        // plausible) but reject negative, NaN, or infinite values —
        // each would make the dollar total nonsensical.
        for (i, row) in parsed.model.iter().enumerate() {
            let label = format!(
                "{}/{} (model[{i}])",
                if row.vendor.is_empty() {
                    "<empty>"
                } else {
                    row.vendor.as_str()
                },
                if row.model.is_empty() {
                    "<empty>"
                } else {
                    row.model.as_str()
                },
            );
            if row.vendor.is_empty() {
                return Err(PricingLoadError::MissingField {
                    path: path.to_path_buf(),
                    field: format!("model[{i}].vendor"),
                });
            }
            if row.model.is_empty() {
                return Err(PricingLoadError::MissingField {
                    path: path.to_path_buf(),
                    field: format!("model[{i}].model"),
                });
            }
            for (name, v) in [
                ("input_per_mtok", row.input_per_mtok),
                ("output_per_mtok", row.output_per_mtok),
            ] {
                if !v.is_finite() {
                    return Err(PricingLoadError::BadPrice {
                        path: path.to_path_buf(),
                        row: label.clone(),
                        field: name.to_owned(),
                        reason: "value is NaN or infinite".to_owned(),
                    });
                }
                if v < 0.0 {
                    return Err(PricingLoadError::BadPrice {
                        path: path.to_path_buf(),
                        row: label.clone(),
                        field: name.to_owned(),
                        reason: format!("value is negative ({v})"),
                    });
                }
            }
        }
        // Validate the date: must parse as YYYY-MM-DD. The
        // chrono_lite helper that does this is private to the crate,
        // so we duplicate the four-line shape-check here rather than
        // promote the helper for one call site.
        if !is_ymd_date(&parsed.last_updated) {
            return Err(PricingLoadError::BadLastUpdated {
                path: path.to_path_buf(),
                value: parsed.last_updated,
            });
        }
        Ok(Self {
            rows: parsed.model,
            last_updated: parsed.last_updated,
            source_path: Some(path.to_path_buf()),
        })
    }

    /// Linear scan; tables are small enough that hashing is overkill.
    /// Returns the matching row or `None` for the unpriced bucket.
    #[must_use]
    pub fn lookup(&self, vendor: &str, model: &str) -> Option<&OwnedModelPrice> {
        self.rows
            .iter()
            .find(|p| p.vendor == vendor && p.model == model)
    }

    /// Sibling of the free [`price_event`] but consulting this table.
    /// Returns `Some((vendor, model, dollars))` on a four-field hit,
    /// `None` otherwise. Dollar value is **not** rounded per-event —
    /// the renderer rounds once at the output boundary.
    #[must_use]
    pub fn price_event<'a>(&'a self, payload: &Value) -> Option<(&'a str, &'a str, f64)> {
        let vendor = payload.get("vendor").and_then(Value::as_str)?;
        let model = payload.get("model").and_then(Value::as_str)?;
        let tokens_in = payload.get("tokens_in").and_then(Value::as_u64)?;
        let tokens_out = payload.get("tokens_out").and_then(Value::as_u64)?;
        let row = self.lookup(vendor, model)?;
        // See note on `price_event` re: u64→f64 precision.
        #[allow(clippy::cast_precision_loss)]
        let dollars = ((tokens_in as f64) * row.input_per_mtok
            + (tokens_out as f64) * row.output_per_mtok)
            / 1_000_000.0;
        Some((row.vendor.as_str(), row.model.as_str(), dollars))
    }
}

#[derive(Debug, Deserialize)]
struct LoadedPricingFile {
    last_updated: String,
    #[serde(default)]
    model: Vec<OwnedModelPrice>,
}

/// All the ways `--pricing-file` can fail. Each variant carries the
/// file path so the CLI diagnostic names the offending file.
#[derive(Debug, thiserror::Error)]
pub enum PricingLoadError {
    #[error("--pricing-file {}: failed to read file: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("--pricing-file {}: file is empty", path.display())]
    Empty { path: PathBuf },
    #[error("--pricing-file {}: TOML parse failed: {message}", path.display())]
    ParseToml { path: PathBuf, message: String },
    #[error("--pricing-file {}: no `[[model]]` rows in file", path.display())]
    NoRows { path: PathBuf },
    #[error("--pricing-file {}: required field `{field}` is missing or empty", path.display())]
    MissingField { path: PathBuf, field: String },
    #[error("--pricing-file {}: row `{row}` field `{field}` is invalid ({reason})", path.display())]
    BadPrice {
        path: PathBuf,
        row: String,
        field: String,
        reason: String,
    },
    #[error(
        "--pricing-file {}: `last_updated` is missing or not a YYYY-MM-DD date ({value:?})",
        path.display()
    )]
    BadLastUpdated { path: PathBuf, value: String },
}

/// `YYYY-MM-DD` shape check. Same body as
/// `chrono_lite::parse_date`'s structural prefix (private helper);
/// duplicated here to keep the slice's blast radius inside
/// `tape-play` without promoting `chrono_lite` to `pub`. Returns
/// `true` for syntactically-plausible dates only — the actual
/// stale-guard arithmetic is in [`crate::pricing_age_days`].
fn is_ymd_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let Some(year) = s.get(0..4).and_then(|x| x.parse::<u32>().ok()) else {
        return false;
    };
    let Some(month) = s.get(5..7).and_then(|x| x.parse::<u32>().ok()) else {
        return false;
    };
    let Some(day) = s.get(8..10).and_then(|x| x.parse::<u32>().ok()) else {
        return false;
    };
    year >= 1 && (1..=12).contains(&month) && (1..=31).contains(&day)
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

    // --- Issue #181: PricingTable + load_from_file -------------------

    fn write_toml(name: &str, body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(name);
        std::fs::write(&p, body).unwrap();
        (dir, p)
    }

    #[test]
    fn bundled_table_mirrors_the_static_const() {
        let t = PricingTable::bundled();
        assert_eq!(t.rows.len(), PRICING_TABLE.len());
        assert_eq!(t.last_updated, PRICING_TABLE_LAST_UPDATED);
        assert!(t.source_path.is_none());
        // Spot-check that opus pricing made the trip.
        let p = t.lookup("anthropic", "claude-opus-4-7").unwrap();
        assert!((p.input_per_mtok - 15.0).abs() < 1e-9);
        assert!((p.output_per_mtok - 75.0).abs() < 1e-9);
    }

    #[test]
    fn load_pricing_file_happy_path() {
        let (_d, path) = write_toml(
            "good.toml",
            r#"
last_updated = "2026-05-15"

[[model]]
vendor = "anthropic"
model = "claude-opus-4-7"
input_per_mtok = 15.0
output_per_mtok = 75.0

[[model]]
vendor = "openai"
model = "gpt-5"
input_per_mtok = 5.0
output_per_mtok = 25.0
"#,
        );
        let t = PricingTable::load_from_file(&path).expect("good.toml should load");
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.last_updated, "2026-05-15");
        assert_eq!(t.source_path.as_deref(), Some(path.as_path()));
        assert!(t.lookup("anthropic", "claude-opus-4-7").is_some());
        assert!(t.lookup("openai", "gpt-5").is_some());
    }

    #[test]
    fn load_pricing_file_missing_file_returns_io_error() {
        let p = std::path::PathBuf::from("/this/path/definitely/does/not/exist.toml");
        let err = PricingTable::load_from_file(&p).unwrap_err();
        assert!(matches!(err, PricingLoadError::Io { .. }), "{err:?}");
    }

    #[test]
    fn load_pricing_file_empty_returns_empty_variant() {
        let (_d, path) = write_toml("empty.toml", "");
        let err = PricingTable::load_from_file(&path).unwrap_err();
        assert!(matches!(err, PricingLoadError::Empty { .. }), "{err:?}");
    }

    #[test]
    fn load_pricing_file_no_rows_returns_no_rows_variant() {
        let (_d, path) = write_toml("norows.toml", r#"last_updated = "2026-05-15""#);
        let err = PricingTable::load_from_file(&path).unwrap_err();
        assert!(matches!(err, PricingLoadError::NoRows { .. }), "{err:?}");
    }

    #[test]
    fn load_pricing_file_bad_toml_returns_parse_error() {
        let (_d, path) = write_toml("bad.toml", "this is not = valid toml [[[");
        let err = PricingTable::load_from_file(&path).unwrap_err();
        assert!(matches!(err, PricingLoadError::ParseToml { .. }), "{err:?}");
    }

    #[test]
    fn load_pricing_file_negative_price_rejected() {
        let (_d, path) = write_toml(
            "neg.toml",
            r#"
last_updated = "2026-05-15"

[[model]]
vendor = "anthropic"
model = "claude-opus-4-7"
input_per_mtok = -1.0
output_per_mtok = 75.0
"#,
        );
        let err = PricingTable::load_from_file(&path).unwrap_err();
        assert!(matches!(err, PricingLoadError::BadPrice { .. }), "{err:?}");
    }

    #[test]
    fn load_pricing_file_missing_vendor_field_rejected() {
        let (_d, path) = write_toml(
            "miss.toml",
            r#"
last_updated = "2026-05-15"

[[model]]
vendor = ""
model = "claude-opus-4-7"
input_per_mtok = 15.0
output_per_mtok = 75.0
"#,
        );
        let err = PricingTable::load_from_file(&path).unwrap_err();
        assert!(
            matches!(err, PricingLoadError::MissingField { ref field, .. } if field.contains("vendor")),
            "{err:?}"
        );
    }

    #[test]
    fn load_pricing_file_bad_last_updated_rejected() {
        let (_d, path) = write_toml(
            "bad_date.toml",
            r#"
last_updated = "yesterday"

[[model]]
vendor = "anthropic"
model = "claude-opus-4-7"
input_per_mtok = 15.0
output_per_mtok = 75.0
"#,
        );
        let err = PricingTable::load_from_file(&path).unwrap_err();
        assert!(
            matches!(err, PricingLoadError::BadLastUpdated { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn loaded_table_replace_not_merge() {
        // The loaded table only has opus; sonnet is absent here but in
        // the bundled table. Proves the replace-not-merge semantic
        // from issue #181.
        let (_d, path) = write_toml(
            "opus_only.toml",
            r#"
last_updated = "2026-05-15"

[[model]]
vendor = "anthropic"
model = "claude-opus-4-7"
input_per_mtok = 15.0
output_per_mtok = 75.0
"#,
        );
        let t = PricingTable::load_from_file(&path).unwrap();
        let opus_payload = serde_json::json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "tokens_in": 1_000_000_u64,
            "tokens_out": 100_000_u64,
        });
        let sonnet_payload = serde_json::json!({
            "vendor": "anthropic",
            "model": "claude-sonnet-4-5",
            "tokens_in": 1_000_000_u64,
            "tokens_out": 100_000_u64,
        });
        assert!(t.price_event(&opus_payload).is_some());
        assert!(
            t.price_event(&sonnet_payload).is_none(),
            "sonnet is in bundled table but absent from loaded — must NOT be merged"
        );
    }

    #[test]
    fn loaded_table_price_event_returns_table_strings() {
        let (_d, path) = write_toml(
            "good.toml",
            r#"
last_updated = "2026-05-15"

[[model]]
vendor = "anthropic"
model = "claude-opus-4-7"
input_per_mtok = 15.0
output_per_mtok = 75.0
"#,
        );
        let t = PricingTable::load_from_file(&path).unwrap();
        let payload = serde_json::json!({
            "vendor": "anthropic",
            "model": "claude-opus-4-7",
            "tokens_in": 1_000_000_u64,
            "tokens_out": 100_000_u64,
        });
        let (v, m, dollars) = t.price_event(&payload).unwrap();
        assert_eq!(v, "anthropic");
        assert_eq!(m, "claude-opus-4-7");
        // 1M * 15 + 100k * 75 → 15 + 7.5 = 22.5
        assert!((dollars - 22.5).abs() < 1e-9, "got {dollars}");
    }

    #[test]
    fn is_ymd_date_accepts_and_rejects() {
        assert!(is_ymd_date("2026-05-15"));
        assert!(is_ymd_date("0001-01-01"));
        assert!(!is_ymd_date("2026/05/15"));
        assert!(!is_ymd_date("2026-13-01"));
        assert!(!is_ymd_date("2026-05-32"));
        assert!(!is_ymd_date("yesterday"));
        assert!(!is_ymd_date(""));
        assert!(!is_ymd_date("2026-05-15T00:00:00Z"));
    }
}
