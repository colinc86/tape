//! Read-side tools — `ls`, `play`, and shared label synthesis.
//!
//! All operations consume an already-loaded `RawTape` plus a parsed track list.
//! No IO happens in this crate beyond what its caller passes in.

use std::fmt::Write;

use serde_json::Value;
use tape_format::tracks::{Kind, Track};

pub mod pricing;

/// Render one line per track for `tape ls`.
///
/// Format: `  <step:3> <kind:13> <label>`
pub fn render_ls(tracks: &[Track]) -> String {
    let mut out = String::new();
    for t in tracks {
        let _ = writeln!(
            out,
            "  {:>3}  {:<12}  {}",
            t.step,
            kind_name(t.kind),
            label(t)
        );
    }
    out
}

/// One row in the `tape watch` polled status table. Phase 1 of #100
/// (carved per #250). `tracks` is `None` when the file doesn't yet
/// parse as a valid `.tape` (mid-eject, partial zip) — the renderer
/// emits `—` in that cell rather than aborting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRow {
    pub path: std::path::PathBuf,
    pub size: u64,
    pub modified: std::time::SystemTime,
    pub tracks: Option<u64>,
}

/// Render the `tape watch` status table. Pure function over a slice
/// and an explicit `now` so relative-time strings (`3s ago`) are
/// deterministic for snapshot tests. The IO shell in `tape-cli`
/// passes `SystemTime::now()`; tests pass a fixed instant.
#[must_use]
pub fn render_watch(rows: &[WatchRow], now: std::time::SystemTime) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<40}  {:>10}  {:>10}  {:>8}",
        "path", "size", "modified", "tracks"
    );
    for row in rows {
        let modified_rel = format_relative_time(row.modified, now);
        let tracks_cell = match row.tracks {
            Some(n) => n.to_string(),
            None => "—".to_owned(),
        };
        let _ = writeln!(
            out,
            "{:<40}  {:>10}  {:>10}  {:>8}",
            row.path.display(),
            format_size(row.size),
            modified_rel,
            tracks_cell
        );
    }
    out
}

/// Human-friendly byte-size formatter — binary units (KiB / MiB /
/// GiB) so `tape ls -lh`-style output matches what users expect on
/// disk. Values < 1024 are integer bytes; anything larger gets one
/// fractional digit.
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut idx = 0;
    while v >= 1024.0 && idx < UNITS.len() - 1 {
        v /= 1024.0;
        idx += 1;
    }
    format!("{v:.1} {}", UNITS[idx])
}

/// Format `then` as a human-friendly delta against `now`. Returns
/// `"now"` for ≤1s, `"<N>s ago"` for sub-minute, `"<N>m ago"`
/// minutes, `"<N>h ago"` hours, `"<N>d ago"` days. Negative deltas
/// (clock skew — `then` in the future) collapse to `"now"`.
pub fn format_relative_time(then: std::time::SystemTime, now: std::time::SystemTime) -> String {
    let elapsed = now.duration_since(then).unwrap_or_default();
    let secs = elapsed.as_secs();
    if secs <= 1 {
        return "now".to_owned();
    }
    if secs < 60 {
        return format!("{secs}s ago");
    }
    if secs < 3600 {
        return format!("{}m ago", secs / 60);
    }
    if secs < 86_400 {
        return format!("{}h ago", secs / 3600);
    }
    format!("{}d ago", secs / 86_400)
}

/// Render a single track as a header + one-line semantic body + blank
/// separator. Used by `tape replay` (#101 Phase 1, carved per #232) so
/// the timeline-walk narration shares the exact `── step N · kind ·
/// ts ──` header format with `tape play` and the deck's `tape.tracks`
/// tool. Body is the existing `label(t)` — vendor/model for
/// `model_call`, command for `shell`, etc. The block ends with `\n\n`
/// so consecutive calls produce visually separated blocks.
pub fn render_track_block(t: &Track) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "── step {} · {} · {} ──",
        t.step,
        kind_name(t.kind),
        t.ts
    );
    out.push_str(&label(t));
    out.push_str("\n\n");
    out
}

/// Three-state per-cassette redaction status, consumed by
/// [`render_track_detail`] and [`render_index_summary`] (Phase 1 of
/// #254, carved from #67). Matches the `cmd_stats` semantics
/// documented at the top of [`render_stats`] — the distinction
/// between "engine ran with zero hits" and "no `redactions.json` at
/// all" is load-bearing for cassette-format archaeology.
///
/// `Applied(N)` carries the whole-cassette count today; per-track
/// scoping is a follow-on (#254 / open question — `redactions.json`
/// in v0 does not carry per-track scoping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionStatus {
    /// Cassette has no `redactions.json` entry (older format /
    /// recorder predating the redaction engine).
    NotProcessed,
    /// `redactions.json` present, zero entries.
    NoneApplied,
    /// `redactions.json` present with N entries (cassette-wide).
    Applied(u64),
}

/// Phase 1 of #254 (carved from #67). Render a single track as a
/// full detail page — every top-level field, the per-track redaction
/// status, the annotation list, and the pretty-printed payload under
/// a `── payload ──` divider. Header is `══ track step N ══`
/// (double box-drawing) to visually differentiate from
/// [`render_track_block`]'s single-rule header.
///
/// Omits the `parent_step:` line when `t.parent_step` is `None` and
/// the `refs:` line when `t.refs` is empty, matching the
/// `#[serde(skip_serializing_if)]` posture on those fields in
/// `tape-format::tracks::Track`. Surfaces `parent_step` and redaction
/// status — the two pieces of information `tape replay --step N`
/// does NOT today.
pub fn render_track_detail(t: &Track, redaction: RedactionStatus) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "══ track step {} ══", t.step);
    out.push('\n');
    let _ = writeln!(out, "kind:           {}", kind_name(t.kind));
    let _ = writeln!(out, "step:           {}", t.step);
    let _ = writeln!(out, "ts:             {}", t.ts);
    if let Some(p) = t.parent_step {
        let _ = writeln!(out, "parent_step:    {p}");
    }
    if !t.refs.is_empty() {
        let _ = writeln!(out, "refs:           {}", t.refs.join(", "));
    }
    let _ = writeln!(out, "redaction:      {}", redaction_line(redaction));
    let _ = writeln!(out, "annotations:    {}", t.annotations.len());
    for a in &t.annotations {
        let _ = writeln!(out, "  - by: {}    note: {:?}", a.by, a.note);
    }
    out.push_str("\n── payload ──\n");
    let pretty = serde_json::to_string_pretty(&t.payload).unwrap_or_else(|_| t.payload.to_string());
    out.push_str(&pretty);
    out.push('\n');
    out
}

/// Phase 1 of #254 (carved from #67). Render the cassette's
/// structural index — track count, first/last ts (omitted when the
/// cassette is empty), redaction status, and the full 8-kind
/// histogram in enum-declaration order (zero-count rows included so
/// the output shape is constant across cassettes — helpful for
/// eyeball-diffing two summaries). Sibling of [`render_stats`] but
/// orthogonal: `tape stats` answers "what did this cost / how big is
/// it?", `tape view` (no flag) answers "what's in here and what's it
/// shaped like?".
pub fn render_index_summary(
    cassette_name: &str,
    tracks: &[Track],
    redaction: RedactionStatus,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "══ tape index: {cassette_name} ══");
    out.push('\n');
    let _ = writeln!(out, "tracks:         {}", tracks.len());
    if let (Some(first), Some(last)) = (tracks.first(), tracks.last()) {
        let _ = writeln!(out, "first_ts:       {}", first.ts);
        let _ = writeln!(out, "last_ts:        {}", last.ts);
    }
    let _ = writeln!(out, "redactions:     {}", redaction_summary_line(redaction));
    out.push_str("\n── kind histogram ──\n");
    let hist = kind_histogram(tracks);
    for k in [
        Kind::Task,
        Kind::ModelCall,
        Kind::McpCall,
        Kind::Shell,
        Kind::FileRead,
        Kind::FileWrite,
        Kind::Annotation,
        Kind::Eject,
    ] {
        let _ = writeln!(out, "  {:<12}  {}", kind_name(k), hist[k as usize]);
    }
    out
}

/// One-line redaction status for the detail page. Whole-cassette
/// scoping today; the `(cassette-wide)` qualifier on the applied
/// branch flags it for the future per-track carve.
fn redaction_line(r: RedactionStatus) -> String {
    match r {
        RedactionStatus::NotProcessed => "not processed".to_owned(),
        RedactionStatus::NoneApplied => "none applied".to_owned(),
        RedactionStatus::Applied(n) => {
            format!("{n} replacement(s) applied (cassette-wide)")
        }
    }
}

/// One-line redaction status for the index summary. Same three
/// variants, slightly different phrasing — the index line is a
/// cassette-wide summary by construction so the `(cassette-wide)`
/// qualifier reads naturally without scare-quotes.
fn redaction_summary_line(r: RedactionStatus) -> String {
    match r {
        RedactionStatus::NotProcessed => "not processed".to_owned(),
        RedactionStatus::NoneApplied => "none applied".to_owned(),
        RedactionStatus::Applied(n) => format!("{n} (cassette-wide)"),
    }
}

/// Render full track payloads for `tape play` (default, no filter — but
/// caller restricts via `--step` / `--range` / `--kind` before passing in).
pub fn render_play(tracks: &[Track]) -> String {
    let mut out = String::new();
    for t in tracks {
        let _ = writeln!(
            out,
            "── step {} · {} · {} ──",
            t.step,
            kind_name(t.kind),
            t.ts
        );
        let pretty =
            serde_json::to_string_pretty(&t.payload).unwrap_or_else(|_| t.payload.to_string());
        out.push_str(&pretty);
        out.push_str("\n\n");
    }
    out
}

/// Default summary view for `tape play <file>` with no filter — meta line plus ls.
pub fn render_summary_view(meta_yaml: &str, liner_md: &str, tracks: &[Track]) -> String {
    let mut out = String::new();
    out.push_str("══ liner notes ══\n\n");
    out.push_str(liner_md);
    if !liner_md.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n══ meta ══\n\n");
    out.push_str(meta_yaml);
    if !meta_yaml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n══ tracks ══\n");
    out.push_str(&render_ls(tracks));
    out
}

/// Step-1 of issue #31: read-only single-cassette analytics. Pure
/// function over already-parsed inputs; no IO. The output is
/// human-readable text only — JSON / TSV / library-aggregate /
/// pricing live in later steps.
///
/// `redactions_count` is `Some(N)` when the cassette had a
/// `redactions.json` entry, `None` otherwise — the difference matters
/// because we report "0" for a tape that was processed by the redact
/// engine with no hits vs an empty line for a tape that pre-dates the
/// redactions.json convention. (Roughly per the issue body's
/// "honest reporting" rule for `tokens: (none recorded)`.)
///
/// `with_cost` (Step-3 of #31, issue #168) opts into the dollar
/// estimate column. Off by default keeps Phase-1 / Phase-2 output
/// byte-for-byte identical.
pub fn render_stats(
    meta: &tape_format::meta::Meta,
    tracks: &[Track],
    redactions_count: Option<u64>,
    with_cost: bool,
) -> String {
    render_stats_with_pricing(
        meta,
        tracks,
        redactions_count,
        with_cost,
        &pricing::PricingTable::bundled(),
    )
}

/// Sibling of [`render_stats`] that consumes an explicit pricing
/// table. Step-4 of #31 (issue #181): the CLI's `--pricing-file`
/// path loads a user-supplied table and routes it here; everything
/// else (e.g. `tape stats --with-cost` with no override) calls
/// [`render_stats`] which substitutes
/// [`pricing::PricingTable::bundled`].
#[allow(clippy::module_name_repetitions)]
pub fn render_stats_with_pricing(
    meta: &tape_format::meta::Meta,
    tracks: &[Track],
    redactions_count: Option<u64>,
    with_cost: bool,
    pricing_table: &pricing::PricingTable,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "id: {}", meta.id);
    let _ = writeln!(out, "task: {}", meta.task);
    let _ = writeln!(out, "outcome: {}", outcome_name(meta.outcome));
    let span = format!("{} → {}", meta.created_at, meta.ejected_at);
    match wall_clock_ms(tracks) {
        WallClock::Span(ms) => {
            let _ = writeln!(out, "span: {span}  ({ms} ms)");
        }
        WallClock::CollapsedSnapshot => {
            let _ = writeln!(out, "span: {span}");
            let _ = writeln!(
                out,
                "time accounting: N/A — single-timestamp snapshot (issue #5)"
            );
        }
        WallClock::Unknown => {
            let _ = writeln!(out, "span: {span}");
        }
    }

    out.push('\n');
    let hist = kind_histogram(tracks);
    let _ = writeln!(out, "tracks: {}", tracks.len());
    for k in [
        Kind::Task,
        Kind::ModelCall,
        Kind::McpCall,
        Kind::Shell,
        Kind::FileRead,
        Kind::FileWrite,
        Kind::Annotation,
        Kind::Eject,
    ] {
        let n = hist[k as usize];
        if n > 0 {
            let _ = writeln!(out, "  {}: {}", kind_name(k), n);
        }
    }

    out.push('\n');
    let model_calls: Vec<&Track> = tracks
        .iter()
        .filter(|t| t.kind == Kind::ModelCall)
        .collect();
    if model_calls.is_empty() {
        let _ = writeln!(out, "tokens: (none recorded)");
    } else {
        let (tin, tout, unknown) = token_totals(&model_calls);
        let known = model_calls.len() as u64 - unknown;
        let unknown_note = if unknown > 0 {
            format!(" ({unknown} model_call event(s) missing token counts)")
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "tokens: in={tin} + out={tout} across {known} model_call event(s){unknown_note}"
        );
    }

    // Step-3 of #31 (issue #168): opt-in dollar-cost estimate column.
    // When `with_cost` is false this block is suppressed entirely,
    // keeping the Phase-1 / Phase-2 output byte-for-byte identical.
    // The bundled table is consumed for callers that don't specify
    // one; `render_stats_with_pricing` is the override path (#181).
    if with_cost && !model_calls.is_empty() {
        render_cost_block(&mut out, &model_calls, pricing_table);
    }

    let mcp_n = hist[Kind::McpCall as usize];
    let shell_n = hist[Kind::Shell as usize];
    let _ = writeln!(out, "tools: {mcp_n} mcp_call, {shell_n} shell");

    let read_n = hist[Kind::FileRead as usize];
    let write_n = hist[Kind::FileWrite as usize];
    let _ = writeln!(out, "files: {read_n} read, {write_n} write");

    match redactions_count {
        Some(n) => {
            let _ = writeln!(out, "redactions: {n}");
        }
        None => {
            let _ = writeln!(out, "redactions: (none recorded)");
        }
    }
    out
}

/// Pinned wire-version for the JSON output of [`render_stats_json`].
/// **Load-bearing**: once `1.0` ships, the shape is frozen — adding a
/// new field requires bumping to `1.1`, never patching `1.0` in
/// place. Consumers pin against this string. (Issue #157.)
pub const STATS_SCHEMA_VERSION: &str = "1.0";

/// Issue #157 / Phase-2 of #31. JSON sibling of [`render_stats`].
/// Reuses every computation the text path already does — `kind_histogram`,
/// `wall_clock_ms`, `token_totals`, `outcome_name`, `kind_name` — and
/// projects the result into the pinned `1.0` schema. No new numbers,
/// no parsing, no IO.
///
/// Omit-when-absent semantics mirror the text path's `(none recorded)`
/// convention:
///
/// - Zero `model_call` events → `tokens.recorded == false`, sub-fields
///   omitted.
/// - `redactions_count` is `None` → `redactions.recorded == false`,
///   `count` omitted.
/// - `wall_clock_ms` is `Unknown` or `CollapsedSnapshot` →
///   `span.wall_clock_ms` is JSON `null`; `span.time_accounting`
///   carries the distinguishing label.
pub fn render_stats_json(
    meta: &tape_format::meta::Meta,
    tracks: &[Track],
    redactions_count: Option<u64>,
) -> serde_json::Value {
    let hist = kind_histogram(tracks);

    // `by_kind` only includes kinds with count > 0. The fixed iteration
    // order matches `render_stats`'s text output for legibility — the
    // resulting JSON object key order is determined by `serde_json::Map`
    // insertion order when serialised with `to_string_pretty`.
    let mut by_kind = serde_json::Map::new();
    for k in [
        Kind::Task,
        Kind::ModelCall,
        Kind::McpCall,
        Kind::Shell,
        Kind::FileRead,
        Kind::FileWrite,
        Kind::Annotation,
        Kind::Eject,
    ] {
        let n = hist[k as usize];
        if n > 0 {
            by_kind.insert(kind_name(k).into(), serde_json::Value::from(n));
        }
    }

    let (wall_ms, time_accounting) = match wall_clock_ms(tracks) {
        WallClock::Span(ms) => (Some(ms), "ok"),
        WallClock::CollapsedSnapshot => (None, "snapshot_collapsed"),
        WallClock::Unknown => (None, "unknown"),
    };
    let wall_ms_json = match wall_ms {
        Some(ms) => serde_json::Value::from(ms),
        None => serde_json::Value::Null,
    };

    let model_calls: Vec<&Track> = tracks
        .iter()
        .filter(|t| t.kind == Kind::ModelCall)
        .collect();
    let tokens_obj = if model_calls.is_empty() {
        serde_json::json!({ "recorded": false })
    } else {
        let (tin, tout, unknown) = token_totals(&model_calls);
        let known = model_calls.len() as u64 - unknown;
        serde_json::json!({
            "recorded": true,
            "input": tin,
            "output": tout,
            "known_model_calls": known,
            "missing_model_calls": unknown,
        })
    };

    let redactions_obj = match redactions_count {
        Some(n) => serde_json::json!({ "recorded": true, "count": n }),
        None => serde_json::json!({ "recorded": false }),
    };

    serde_json::json!({
        "schema_version": STATS_SCHEMA_VERSION,
        "id": meta.id,
        "task": meta.task,
        "outcome": outcome_name(meta.outcome),
        "span": {
            "created_at": meta.created_at,
            "ejected_at": meta.ejected_at,
            "wall_clock_ms": wall_ms_json,
            "time_accounting": time_accounting,
        },
        "tracks": {
            "total": tracks.len() as u64,
            "by_kind": serde_json::Value::Object(by_kind),
        },
        "tokens": tokens_obj,
        "tools": {
            "mcp_call": hist[Kind::McpCall as usize],
            "shell": hist[Kind::Shell as usize],
        },
        "files": {
            "read": hist[Kind::FileRead as usize],
            "write": hist[Kind::FileWrite as usize],
        },
        "redactions": redactions_obj,
    })
}

fn outcome_name(o: tape_format::meta::Outcome) -> &'static str {
    use tape_format::meta::Outcome;
    match o {
        Outcome::Success => "success",
        Outcome::Failure => "failure",
        Outcome::Abandoned => "abandoned",
        Outcome::Unknown => "unknown",
    }
}

fn kind_histogram(tracks: &[Track]) -> [u64; 8] {
    let mut h = [0u64; 8];
    for t in tracks {
        h[t.kind as usize] += 1;
    }
    h
}

enum WallClock {
    Span(i64),
    CollapsedSnapshot,
    Unknown,
}

/// Wall-clock span across the track list. Falls back to
/// `CollapsedSnapshot` when every non-task/non-eject event shares one
/// `ts` (bug #5 — snapshot-imported cassettes), and `Unknown` when the
/// list is too short or timestamps don't parse. Lexical compare on the
/// RFC-3339 strings would work for ordering but not for ms-precision
/// arithmetic, so we parse to `chrono::DateTime` here.
fn wall_clock_ms(tracks: &[Track]) -> WallClock {
    if tracks.len() < 2 {
        return WallClock::Unknown;
    }
    let body: Vec<&Track> = tracks
        .iter()
        .filter(|t| t.kind != Kind::Task && t.kind != Kind::Eject)
        .collect();
    // Issue #5 snapshot-collapse fingerprint: ≥2 body events with one
    // shared `ts`. A single body event has nothing to compare against,
    // and an empty body is just a task+eject tape with no time data.
    if body.len() >= 2 {
        let first_ts = body[0].ts.as_str();
        if body.iter().all(|t| t.ts == first_ts) {
            return WallClock::CollapsedSnapshot;
        }
    }
    let first = parse_rfc3339(&tracks.first().unwrap().ts);
    let last = parse_rfc3339(&tracks.last().unwrap().ts);
    match (first, last) {
        (Some(a), Some(b)) => WallClock::Span(b - a),
        _ => WallClock::Unknown,
    }
}

fn parse_rfc3339(s: &str) -> Option<i64> {
    let dt = chrono_lite::parse(s)?;
    Some(dt)
}

/// Tiny chrono-free RFC-3339 parser. Returns the timestamp in
/// milliseconds since the Unix epoch. Tape-play doesn't depend on
/// `chrono` and pulling it in just for `tape stats` is overkill; this
/// handles the `%Y-%m-%dT%H:%M:%S(.%3f)?Z` shape every tape writer in
/// this repo emits.
mod chrono_lite {
    pub fn parse(s: &str) -> Option<i64> {
        // Expect "YYYY-MM-DDTHH:MM:SS" then optional ".fff" then "Z".
        let bytes = s.as_bytes();
        if bytes.len() < 20
            || bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes[10] != b'T'
            || bytes[13] != b':'
            || bytes[16] != b':'
            || !s.ends_with('Z')
        {
            return None;
        }
        let year: i64 = s.get(0..4)?.parse().ok()?;
        let month: u32 = s.get(5..7)?.parse().ok()?;
        let day: u32 = s.get(8..10)?.parse().ok()?;
        let hour: u32 = s.get(11..13)?.parse().ok()?;
        let minute: u32 = s.get(14..16)?.parse().ok()?;
        let second: u32 = s.get(17..19)?.parse().ok()?;

        let mut ms_frac: i64 = 0;
        // Optional ".fff" between seconds and Z.
        if bytes[19] == b'.' {
            // s = "YYYY-MM-DDTHH:MM:SS.fff...Z"
            let z_pos = s.len() - 1;
            let frac = s.get(20..z_pos)?;
            // Truncate or pad to 3 digits.
            let padded: String = frac.chars().chain("000".chars()).take(3).collect();
            ms_frac = padded.parse().ok()?;
        } else if bytes[19] != b'Z' {
            return None;
        }

        let days = days_from_civil(year, month, day);
        let secs = days * 86_400 + (hour as i64) * 3600 + (minute as i64) * 60 + second as i64;
        Some(secs * 1000 + ms_frac)
    }

    /// Howard Hinnant's `days_from_civil`. Returns days since 1970-01-01
    /// (Unix epoch) for a given (year, month, day). Correct over the
    /// full proleptic Gregorian range. Cited from
    /// <https://howardhinnant.github.io/date_algorithms.html>.
    pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m = m as i64;
        let d = d as i64;
        let doy = (153 * if m > 2 { m - 3 } else { m + 9 } + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    /// Parse a `YYYY-MM-DD` date string into days-since-epoch (UTC).
    /// Used by the pricing stale-guard at issue #168. Returns `None`
    /// on any malformed input; the caller treats an unparseable
    /// `PRICING_TABLE_LAST_UPDATED` as "stale check unavailable" and
    /// skips the warning rather than crashing.
    pub fn parse_date(s: &str) -> Option<i64> {
        let bytes = s.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let year: i64 = s.get(0..4)?.parse().ok()?;
        let month: u32 = s.get(5..7)?.parse().ok()?;
        let day: u32 = s.get(8..10)?.parse().ok()?;
        if !(1..=12).contains(&month) {
            return None;
        }
        if !(1..=31).contains(&day) {
            return None;
        }
        Some(days_from_civil(year, month, day))
    }

    /// Today's date in days-since-Unix-epoch, derived from
    /// `SystemTime::now()`. Floors to UTC midnight; the caller's
    /// staleness threshold operates in whole-day units, so any sub-
    /// day precision would be lost anyway. Returns `None` if the
    /// system clock is set before 1970 (impossible in practice; the
    /// caller skips the stale-guard rather than panicking).
    pub fn today_days_since_epoch() -> Option<i64> {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        let secs = i64::try_from(secs).ok()?;
        Some(secs / 86_400)
    }
}

fn token_totals(model_calls: &[&Track]) -> (u64, u64, u64) {
    let mut tin: u64 = 0;
    let mut tout: u64 = 0;
    let mut unknown: u64 = 0;
    for t in model_calls {
        let a = t.payload.get("tokens_in").and_then(Value::as_u64);
        let b = t.payload.get("tokens_out").and_then(Value::as_u64);
        match (a, b) {
            (Some(x), Some(y)) => {
                tin += x;
                tout += y;
            }
            _ => unknown += 1,
        }
    }
    (tin, tout, unknown)
}

/// Aggregate cost across a slice of `model_call` events. Step-3 of
/// #31 (issue #168). `priced` is the count that matched the bundled
/// pricing table AND had both `tokens_in` and `tokens_out`; `total`
/// is `model_calls.len()`. The caller renders one of three lines
/// depending on whether `priced` is 0 / partial / full.
///
/// The dollar total is accumulated as `f64` and rounded once at the
/// rendered-output boundary — per-event rounding would compound a
/// fraction-of-a-cent error across many calls.
#[derive(Debug, Clone, Copy)]
pub struct CostResult {
    pub dollars: f64,
    pub priced: u64,
    pub total: u64,
}

/// Append the `cost:` line (and optional stale-guard warning) for the
/// given `model_calls` using the supplied pricing table. The three
/// text branches (no-priceable / N-of-M / full) and the >90-day
/// warning follow the issue #168 / #181 bodies' specs.
fn render_cost_block(
    out: &mut String,
    model_calls: &[&Track],
    pricing_table: &pricing::PricingTable,
) {
    let cost = cost_total_in(model_calls, pricing_table);
    if cost.priced == 0 {
        let _ = writeln!(out, "cost: (no priceable model_call events)");
    } else {
        let qualifier = if cost.priced < cost.total {
            format!(
                "estimate; {} of {} model_calls priced; pricing table {}",
                cost.priced, cost.total, pricing_table.last_updated,
            )
        } else {
            format!("estimate; pricing table {}", pricing_table.last_updated)
        };
        let _ = writeln!(out, "cost: ${:.4}  ({qualifier})", cost.dollars);
    }
    if let Some(days) = pricing_age_days(pricing_table) {
        if days > pricing::PRICING_STALENESS_DAYS {
            let label = pricing_table.source_path.as_ref().map_or_else(
                || "bundled pricing table".to_owned(),
                |p| format!("pricing table {}", p.display()),
            );
            let _ = writeln!(
                out,
                "warning: {label} is {days} days old (>{} day threshold); cost figures may be stale",
                pricing::PRICING_STALENESS_DAYS,
            );
        }
    }
}

/// Bundled-table cost-totaling. Preserved for backward compatibility;
/// new call sites use [`cost_total_in`] with an explicit table.
#[must_use]
pub fn cost_total(model_calls: &[&Track]) -> CostResult {
    cost_total_in(model_calls, &pricing::PricingTable::bundled())
}

/// Total cost across a slice of `model_call` events, consulting an
/// arbitrary [`pricing::PricingTable`]. The replace-not-merge
/// semantics (issue #181) live here: events whose vendor/model
/// aren't in `table` fall through to the unpriced bucket even if the
/// bundled table would have priced them.
pub fn cost_total_in(model_calls: &[&Track], table: &pricing::PricingTable) -> CostResult {
    let mut dollars: f64 = 0.0;
    let mut priced: u64 = 0;
    let total = model_calls.len() as u64;
    for t in model_calls {
        if let Some((_, _, d)) = table.price_event(&t.payload) {
            dollars += d;
            priced += 1;
        }
    }
    CostResult {
        dollars,
        priced,
        total,
    }
}

/// Days elapsed since `table.last_updated`. Used for the >90-day
/// stale-guard warning. Returns `None` if the date is unparseable or
/// the system clock predates the Unix epoch — in either case the
/// caller skips the stale-guard rather than panicking.
pub fn pricing_age_days(table: &pricing::PricingTable) -> Option<i64> {
    let updated = chrono_lite::parse_date(&table.last_updated)?;
    let today = chrono_lite::today_days_since_epoch()?;
    Some(today - updated)
}

/// One-line semantic label for a track. Used by `tape ls`,
/// the deck's `tape.tracks` tool, and the diff aligner.
pub fn label(t: &Track) -> String {
    match t.kind {
        Kind::Task => format!(
            "{:?}",
            t.payload
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or("")
        ),
        Kind::ModelCall => {
            let vendor = t
                .payload
                .get("vendor")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let model = t
                .payload
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let tin = t
                .payload
                .get("tokens_in")
                .and_then(Value::as_u64)
                .map(|n| format!(" in:{n}"))
                .unwrap_or_default();
            let tout = t
                .payload
                .get("tokens_out")
                .and_then(Value::as_u64)
                .map(|n| format!(" out:{n}"))
                .unwrap_or_default();
            format!("{vendor}/{model}{tin}{tout}")
        }
        Kind::McpCall => {
            let server = t
                .payload
                .get("server")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let tool = t.payload.get("tool").and_then(Value::as_str).unwrap_or("?");
            let args_summary = t
                .payload
                .get("args")
                .map(summarize_args)
                .unwrap_or_else(|| "()".into());
            format!("{server}.{tool}{args_summary}")
        }
        Kind::Shell => {
            let cmd = t
                .payload
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("");
            truncate(cmd, 80)
        }
        Kind::FileRead => {
            let path = t.payload.get("path").and_then(Value::as_str).unwrap_or("?");
            format!("read({path})")
        }
        Kind::FileWrite => {
            let path = t.payload.get("path").and_then(Value::as_str).unwrap_or("?");
            format!("write({path})")
        }
        Kind::Annotation => t
            .payload
            .get("note")
            .and_then(Value::as_str)
            .map(|s| format!("{:?}", truncate(s, 80)))
            .unwrap_or_else(|| "(no note)".into()),
        Kind::Eject => t
            .payload
            .get("outcome")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .into(),
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

fn summarize_args(v: &Value) -> String {
    let s = v.to_string();
    let truncated = truncate(&s, 80);
    if truncated.starts_with('(') {
        truncated
    } else {
        format!("({truncated})")
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.replace('\n', " ").to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out.replace('\n', " ")
    }
}

/// Filter tracks by an optional kind name and step range.
pub fn filter<'a>(
    tracks: &'a [Track],
    step: Option<u64>,
    range: Option<(u64, u64)>,
    kind: Option<&str>,
) -> Vec<&'a Track> {
    let parsed_kind = kind.and_then(parse_kind);
    tracks
        .iter()
        .filter(|t| match step {
            Some(s) => t.step == s,
            None => true,
        })
        .filter(|t| match range {
            Some((lo, hi)) => t.step >= lo && t.step <= hi,
            None => true,
        })
        .filter(|t| match parsed_kind {
            Some(k) => t.kind == k,
            None => true,
        })
        .collect()
}

/// Parse a kind name from CLI input.
pub fn parse_kind(name: &str) -> Option<Kind> {
    match name {
        "task" => Some(Kind::Task),
        "model_call" => Some(Kind::ModelCall),
        "mcp_call" => Some(Kind::McpCall),
        "shell" => Some(Kind::Shell),
        "file_read" => Some(Kind::FileRead),
        "file_write" => Some(Kind::FileWrite),
        "annotation" => Some(Kind::Annotation),
        "eject" => Some(Kind::Eject),
        _ => None,
    }
}

/// Parse a `--range N..M` argument.
pub fn parse_range(s: &str) -> Option<(u64, u64)> {
    let (lo, hi) = s.split_once("..")?;
    Some((lo.parse().ok()?, hi.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn t(step: u64, kind: Kind, payload: Value) -> Track {
        Track {
            step,
            kind,
            ts: format!("2026-05-06T10:00:{step:02}Z"),
            payload,
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        }
    }

    #[test]
    fn label_task() {
        let track = t(1, Kind::Task, json!({"prompt": "Investigate"}));
        assert_eq!(label(&track), r#""Investigate""#);
    }

    #[test]
    fn render_track_block_shape() {
        let track = t(
            3,
            Kind::ModelCall,
            json!({
                "vendor": "anthropic",
                "model": "claude-opus-4-7",
                "tokens_in": 1234,
                "tokens_out": 567,
            }),
        );
        let block = render_track_block(&track);
        assert_eq!(
            block,
            "── step 3 · model_call · 2026-05-06T10:00:03Z ──\n\
             anthropic/claude-opus-4-7 in:1234 out:567\n\n"
        );
    }

    #[test]
    fn label_mcp_call() {
        let track = t(
            2,
            Kind::McpCall,
            json!({"server": "db", "tool": "query", "args": {"sql": "SELECT 1"}}),
        );
        assert!(label(&track).starts_with("db.query("));
    }

    #[test]
    fn render_ls_has_one_line_per_track() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_ls(&tracks);
        assert_eq!(s.lines().count(), 2);
    }

    #[test]
    fn parse_range_works() {
        assert_eq!(parse_range("3..7"), Some((3, 7)));
        assert_eq!(parse_range("not-a-range"), None);
    }

    fn fresh_meta() -> tape_format::meta::Meta {
        tape_format::meta::Meta {
            tape_version: "tape/v0".into(),
            id: "01h8xy00-0000-7000-b8aa-000000000031".into(),
            created_at: "2026-05-06T10:00:00Z".into(),
            ejected_at: "2026-05-06T10:00:42Z".into(),
            task: "test the stats".into(),
            recorder: tape_format::meta::Recorder {
                agent: "test/0.0.1".into(),
                user: None,
            },
            outcome: tape_format::meta::Outcome::Success,
            models: vec![],
            tools: vec![],
            tool_budget: None,
            redaction_summary: None,
            label: None,
            recap: None,
            recaps: vec![],
            tags: vec![],
            relinernotes: vec![],
            compactions: vec![],
            new_block: None,
        }
    }

    #[test]
    fn stats_renders_kind_histogram() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(
                2,
                Kind::ModelCall,
                json!({"vendor": "anthropic", "model": "x"}),
            ),
            t(
                3,
                Kind::ModelCall,
                json!({"vendor": "anthropic", "model": "x"}),
            ),
            t(4, Kind::Shell, json!({"command": "ls"})),
            t(5, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), false);
        assert!(s.contains("tracks: 5"), "{s}");
        assert!(s.contains("task: 1"), "{s}");
        assert!(s.contains("model_call: 2"), "{s}");
        assert!(s.contains("shell: 1"), "{s}");
        assert!(s.contains("eject: 1"), "{s}");
        // Kinds with zero count are not rendered (terseness).
        assert!(!s.contains("file_read: 0"), "{s}");
    }

    #[test]
    fn stats_reports_wall_clock_ms_for_normal_tape() {
        // The `t` helper builds ts as 2026-05-06T10:00:{step:02}Z, so tracks
        // 1..=5 span 4 seconds = 4000 ms.
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::ModelCall, json!({"vendor": "x", "model": "y"})),
            t(5, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), false);
        assert!(s.contains("(4000 ms)"), "{s}");
    }

    #[test]
    fn stats_marks_snapshot_collapse() {
        // All non-task/non-eject events share one ts → snapshot collapse.
        let same_ts = "2026-05-06T10:00:30Z";
        let mk = |step: u64, kind: Kind, payload: Value| Track {
            step,
            kind,
            ts: same_ts.to_string(),
            payload,
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        };
        let tracks = vec![
            mk(1, Kind::Task, json!({"prompt": "x"})),
            mk(2, Kind::ModelCall, json!({"vendor": "x", "model": "y"})),
            mk(3, Kind::ModelCall, json!({"vendor": "x", "model": "y"})),
            mk(4, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), false);
        assert!(s.contains("time accounting: N/A"), "{s}");
        assert!(s.contains("issue #5"), "{s}");
        // The other sections still render.
        assert!(s.contains("tracks: 4"), "{s}");
    }

    #[test]
    fn stats_sums_tokens_with_unknown_count() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(
                2,
                Kind::ModelCall,
                json!({"vendor": "x", "model": "y", "tokens_in": 100, "tokens_out": 50}),
            ),
            t(
                3,
                Kind::ModelCall,
                json!({"vendor": "x", "model": "y", "tokens_in": 200, "tokens_out": 80}),
            ),
            // Missing tokens — counts as unknown.
            t(4, Kind::ModelCall, json!({"vendor": "x", "model": "y"})),
            t(5, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), false);
        assert!(s.contains("tokens: in=300 + out=130"), "{s}");
        assert!(s.contains("across 2 model_call event(s)"), "{s}");
        assert!(
            s.contains("1 model_call event(s) missing token counts"),
            "{s}"
        );
    }

    #[test]
    fn stats_says_none_recorded_with_no_model_calls() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Shell, json!({"command": "ls"})),
            t(3, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), false);
        assert!(s.contains("tokens: (none recorded)"), "{s}");
    }

    #[test]
    fn stats_counts_tools_and_files() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::McpCall, json!({"server": "db", "tool": "q"})),
            t(3, Kind::McpCall, json!({"server": "db", "tool": "q"})),
            t(4, Kind::Shell, json!({"command": "ls"})),
            t(5, Kind::FileRead, json!({"path": "/a"})),
            t(6, Kind::FileRead, json!({"path": "/b"})),
            t(7, Kind::FileWrite, json!({"path": "/c"})),
            t(8, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), false);
        assert!(s.contains("tools: 2 mcp_call, 1 shell"), "{s}");
        assert!(s.contains("files: 2 read, 1 write"), "{s}");
    }

    #[test]
    fn stats_redactions_distinguishes_zero_from_missing() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Eject, json!({"outcome": "success"})),
        ];
        // Engine ran, zero hits.
        let s = render_stats(&fresh_meta(), &tracks, Some(0), false);
        assert!(s.contains("redactions: 0"), "{s}");

        // No redactions.json at all (older cassette format).
        let s = render_stats(&fresh_meta(), &tracks, None, false);
        assert!(s.contains("redactions: (none recorded)"), "{s}");

        // Non-zero count.
        let s = render_stats(&fresh_meta(), &tracks, Some(3), false);
        assert!(s.contains("redactions: 3"), "{s}");
    }

    #[test]
    fn stats_header_contains_meta_fields() {
        let s = render_stats(
            &fresh_meta(),
            &[t(1, Kind::Task, json!({"prompt": "x"}))],
            Some(0),
            false,
        );
        assert!(
            s.contains("id: 01h8xy00-0000-7000-b8aa-000000000031"),
            "{s}"
        );
        assert!(s.contains("task: test the stats"), "{s}");
        assert!(s.contains("outcome: success"), "{s}");
        assert!(
            s.contains("2026-05-06T10:00:00Z → 2026-05-06T10:00:42Z"),
            "{s}"
        );
    }

    #[test]
    fn chrono_lite_parses_basic_rfc3339() {
        // Exact-second precision.
        let a = chrono_lite::parse("2026-05-06T10:00:00Z").unwrap();
        let b = chrono_lite::parse("2026-05-06T10:00:42Z").unwrap();
        assert_eq!(b - a, 42_000);
    }

    #[test]
    fn chrono_lite_parses_millis() {
        let a = chrono_lite::parse("2026-05-06T10:00:00.000Z").unwrap();
        let b = chrono_lite::parse("2026-05-06T10:00:00.250Z").unwrap();
        assert_eq!(b - a, 250);
    }

    #[test]
    fn chrono_lite_rejects_malformed() {
        assert!(chrono_lite::parse("not-a-timestamp").is_none());
        assert!(chrono_lite::parse("2026-05-06T10:00:00").is_none()); // no Z
    }

    // --- chrono_lite::parse_date (issue #168 stale-guard) --------------

    #[test]
    fn parse_date_returns_days_since_epoch() {
        // 1970-01-01 is day 0; 1970-01-02 is day 1.
        assert_eq!(chrono_lite::parse_date("1970-01-01"), Some(0));
        assert_eq!(chrono_lite::parse_date("1970-01-02"), Some(1));
    }

    #[test]
    fn parse_date_handles_leap_year_boundary() {
        // Feb 29, 2024 — the day after Feb 28 in a leap year.
        let feb28 = chrono_lite::parse_date("2024-02-28").unwrap();
        let feb29 = chrono_lite::parse_date("2024-02-29").unwrap();
        let mar01 = chrono_lite::parse_date("2024-03-01").unwrap();
        assert_eq!(feb29 - feb28, 1);
        assert_eq!(mar01 - feb29, 1);
    }

    #[test]
    fn parse_date_rejects_malformed() {
        assert!(chrono_lite::parse_date("not-a-date").is_none());
        assert!(chrono_lite::parse_date("2026/05/15").is_none());
        assert!(chrono_lite::parse_date("2026-13-01").is_none()); // month out of range
        assert!(chrono_lite::parse_date("2026-05-32").is_none()); // day out of range
        assert!(chrono_lite::parse_date("2026-5-15").is_none()); // unpadded month
    }

    // --- cost_total / render_stats with_cost (issue #168) --------------

    #[test]
    fn cost_total_priced_full_when_all_pairs_known() {
        let tracks = [
            t(
                1,
                Kind::ModelCall,
                json!({
                    "vendor": "anthropic",
                    "model": "claude-opus-4-7",
                    "tokens_in": 1_000_000_u64,
                    "tokens_out": 100_000_u64,
                }),
            ),
            t(
                2,
                Kind::ModelCall,
                json!({
                    "vendor": "anthropic",
                    "model": "claude-haiku-4-5",
                    "tokens_in": 500_000_u64,
                    "tokens_out": 50_000_u64,
                }),
            ),
        ];
        let model_calls: Vec<&Track> = tracks.iter().collect();
        let res = cost_total(&model_calls);
        assert_eq!(res.priced, 2);
        assert_eq!(res.total, 2);
        // Opus: 1M*$15 + 100k*$75 = $15 + $7.50 = $22.50
        // Haiku: 500k*$1 + 50k*$5 = $0.50 + $0.25 = $0.75
        // Total: $23.25
        assert!((res.dollars - 23.25).abs() < 0.0001, "got {}", res.dollars);
    }

    #[test]
    fn cost_total_priced_partial_when_some_unknown() {
        let tracks = [
            t(
                1,
                Kind::ModelCall,
                json!({
                    "vendor": "anthropic",
                    "model": "claude-opus-4-7",
                    "tokens_in": 1_000_000_u64,
                    "tokens_out": 100_000_u64,
                }),
            ),
            t(
                2,
                Kind::ModelCall,
                json!({"vendor": "anthropic", "model": "unknown-model"}),
            ),
        ];
        let model_calls: Vec<&Track> = tracks.iter().collect();
        let res = cost_total(&model_calls);
        assert_eq!(res.priced, 1);
        assert_eq!(res.total, 2);
        assert!((res.dollars - 22.50).abs() < 0.0001);
    }

    #[test]
    fn cost_total_priced_zero_when_all_unknown() {
        let tracks = [t(
            1,
            Kind::ModelCall,
            json!({"vendor": "anthropic", "model": "no-such-model"}),
        )];
        let model_calls: Vec<&Track> = tracks.iter().collect();
        let res = cost_total(&model_calls);
        assert_eq!(res.priced, 0);
        assert_eq!(res.total, 1);
        // priced==0 already proves the dollar accumulator never ran;
        // checking it directly hits `clippy::float_cmp`. Leave the
        // priced/total assertions as the load-bearing contract.
    }

    #[test]
    fn render_stats_with_cost_off_has_no_cost_line() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(
                2,
                Kind::ModelCall,
                json!({
                    "vendor": "anthropic",
                    "model": "claude-opus-4-7",
                    "tokens_in": 1_000_u64,
                    "tokens_out": 200_u64,
                }),
            ),
            t(3, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), false);
        assert!(!s.contains("cost:"), "default output omits cost line: {s}");
    }

    #[test]
    fn render_stats_with_cost_on_emits_dollar_line() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(
                2,
                Kind::ModelCall,
                json!({
                    "vendor": "anthropic",
                    "model": "claude-opus-4-7",
                    "tokens_in": 1_000_000_u64,
                    "tokens_out": 100_000_u64,
                }),
            ),
            t(3, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), true);
        assert!(s.contains("cost: $22.5000"), "got:\n{s}");
        assert!(
            s.contains("pricing table"),
            "should name the pricing table date: {s}"
        );
    }

    #[test]
    fn render_stats_with_cost_no_priceable_branch() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(
                2,
                Kind::ModelCall,
                json!({"vendor": "anthropic", "model": "no-such-model"}),
            ),
            t(3, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), true);
        assert!(s.contains("cost: (no priceable model_call events)"), "{s}");
    }

    #[test]
    fn render_stats_with_cost_partial_branch_shows_n_of_m() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(
                2,
                Kind::ModelCall,
                json!({
                    "vendor": "anthropic",
                    "model": "claude-opus-4-7",
                    "tokens_in": 1_000_u64,
                    "tokens_out": 200_u64,
                }),
            ),
            t(
                3,
                Kind::ModelCall,
                json!({"vendor": "anthropic", "model": "unknown-model"}),
            ),
            t(4, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_stats(&fresh_meta(), &tracks, Some(0), true);
        assert!(s.contains("1 of 2 model_calls priced"), "{s}");
    }

    // --- render_stats_json (issue #157 / Phase 2) ----------------------

    #[test]
    fn json_pins_schema_version_1_0() {
        // The whole point of the pin is that this assertion fails
        // loudly if anyone bumps the const without an intentional
        // schema migration. The Phase-2 PR ships `1.0`.
        assert_eq!(STATS_SCHEMA_VERSION, "1.0");
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Eject, json!({"outcome": "success"})),
        ];
        let v = render_stats_json(&fresh_meta(), &tracks, None);
        assert_eq!(v["schema_version"], "1.0");
    }

    #[test]
    fn json_carries_top_level_fields() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(
                2,
                Kind::ModelCall,
                json!({"vendor": "anthropic", "model": "x",
                                          "tokens_in": 10, "tokens_out": 5}),
            ),
            t(3, Kind::Shell, json!({"command": "ls"})),
            t(4, Kind::Eject, json!({"outcome": "success"})),
        ];
        let v = render_stats_json(&fresh_meta(), &tracks, Some(4));
        assert_eq!(v["id"], fresh_meta().id);
        assert_eq!(v["task"], fresh_meta().task);
        assert_eq!(v["outcome"], "success");
        assert_eq!(v["span"]["created_at"], fresh_meta().created_at);
        assert_eq!(v["span"]["ejected_at"], fresh_meta().ejected_at);
        assert_eq!(v["span"]["time_accounting"], "ok");
        assert_eq!(v["tracks"]["total"], 4);
        assert_eq!(v["tools"]["mcp_call"], 0);
        assert_eq!(v["tools"]["shell"], 1);
        assert_eq!(v["files"]["read"], 0);
        assert_eq!(v["files"]["write"], 0);
        assert_eq!(v["redactions"]["recorded"], true);
        assert_eq!(v["redactions"]["count"], 4);
    }

    #[test]
    fn json_by_kind_only_includes_nonzero_kinds() {
        // The §3 schema says "only kinds with count > 0" — a tape with
        // a `task` and an `eject` must not surface `model_call: 0`,
        // `shell: 0`, etc. serde_json's default Map backing is
        // alphabetical (no insertion-order feature), so this assertion
        // is set-style rather than order-pinned.
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Eject, json!({"outcome": "success"})),
        ];
        let v = render_stats_json(&fresh_meta(), &tracks, None);
        let by_kind = v["tracks"]["by_kind"].as_object().unwrap();
        let keys: std::collections::BTreeSet<&str> = by_kind.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> = ["task", "eject"].into_iter().collect();
        assert_eq!(keys, expected);
        // None of the zero-count kinds should sneak in.
        for absent in [
            "model_call",
            "mcp_call",
            "shell",
            "file_read",
            "file_write",
            "annotation",
        ] {
            assert!(
                !by_kind.contains_key(absent),
                "{absent} must be omitted when count is 0"
            );
        }
        assert_eq!(by_kind["task"], 1);
        assert_eq!(by_kind["eject"], 1);
    }

    #[test]
    fn json_tokens_recorded_false_when_no_model_calls() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Eject, json!({"outcome": "success"})),
        ];
        let v = render_stats_json(&fresh_meta(), &tracks, Some(0));
        assert_eq!(v["tokens"]["recorded"], false);
        // Sub-fields MUST be omitted (not null) when recorded=false.
        let tokens = v["tokens"].as_object().unwrap();
        assert!(!tokens.contains_key("input"), "tokens={tokens:?}");
        assert!(!tokens.contains_key("output"), "tokens={tokens:?}");
        assert!(
            !tokens.contains_key("known_model_calls"),
            "tokens={tokens:?}"
        );
        assert!(
            !tokens.contains_key("missing_model_calls"),
            "tokens={tokens:?}"
        );
    }

    #[test]
    fn json_tokens_aggregates_when_model_calls_present() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(
                2,
                Kind::ModelCall,
                json!({"vendor": "anthropic", "model": "x",
                     "tokens_in": 100, "tokens_out": 25}),
            ),
            t(
                3,
                Kind::ModelCall,
                json!({"vendor": "anthropic", "model": "x",
                     "tokens_in": 50, "tokens_out": 10}),
            ),
            t(4, Kind::ModelCall, json!({"vendor": "x", "model": "y"})),
            t(5, Kind::Eject, json!({"outcome": "success"})),
        ];
        let v = render_stats_json(&fresh_meta(), &tracks, None);
        assert_eq!(v["tokens"]["recorded"], true);
        assert_eq!(v["tokens"]["input"], 150);
        assert_eq!(v["tokens"]["output"], 35);
        assert_eq!(v["tokens"]["known_model_calls"], 2);
        assert_eq!(v["tokens"]["missing_model_calls"], 1);
    }

    #[test]
    fn json_redactions_recorded_false_when_redactions_count_none() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Eject, json!({"outcome": "success"})),
        ];
        let v = render_stats_json(&fresh_meta(), &tracks, None);
        assert_eq!(v["redactions"]["recorded"], false);
        let red = v["redactions"].as_object().unwrap();
        assert!(!red.contains_key("count"), "red={red:?}");
    }

    #[test]
    fn json_snapshot_collapse_marks_time_accounting_and_nulls_wall_clock() {
        let same_ts = "2026-05-06T10:00:30Z";
        let mk = |step: u64, kind: Kind, payload: Value| Track {
            step,
            kind,
            ts: same_ts.to_string(),
            payload,
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        };
        let tracks = vec![
            mk(1, Kind::Task, json!({"prompt": "x"})),
            mk(2, Kind::ModelCall, json!({"vendor": "x", "model": "y"})),
            mk(3, Kind::ModelCall, json!({"vendor": "x", "model": "y"})),
            mk(4, Kind::Eject, json!({"outcome": "success"})),
        ];
        let v = render_stats_json(&fresh_meta(), &tracks, None);
        assert!(
            v["span"]["wall_clock_ms"].is_null(),
            "wall_clock_ms must be null on snapshot-collapse, got {v:?}"
        );
        assert_eq!(v["span"]["time_accounting"], "snapshot_collapsed");
    }

    #[test]
    fn json_wall_clock_ms_unknown_when_one_track() {
        // A single-event tape has no body diff; wall_clock_ms must be
        // null with time_accounting `unknown`.
        let tracks = vec![t(1, Kind::Task, json!({"prompt": "x"}))];
        let v = render_stats_json(&fresh_meta(), &tracks, None);
        assert!(v["span"]["wall_clock_ms"].is_null());
        assert_eq!(v["span"]["time_accounting"], "unknown");
    }
}

// =====================================================================
// Phase 1 of #254 (carved per #67): `tape view` renderer unit tests.
// =====================================================================

#[cfg(test)]
mod view_tests {
    use super::*;
    use serde_json::json;
    use tape_format::tracks::Annotation;

    fn t(step: u64, kind: Kind, payload: Value) -> Track {
        Track {
            step,
            kind,
            ts: format!("2026-05-06T10:00:{step:02}Z"),
            payload,
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        }
    }

    #[test]
    fn detail_renders_header_kind_step_ts() {
        let track = t(
            7,
            Kind::ModelCall,
            json!({"vendor": "anthropic", "model": "x"}),
        );
        let s = render_track_detail(&track, RedactionStatus::NotProcessed);
        assert!(s.contains("══ track step 7 ══"), "{s}");
        assert!(s.contains("kind:           model_call"), "{s}");
        assert!(s.contains("step:           7"), "{s}");
        assert!(s.contains("ts:             2026-05-06T10:00:07Z"), "{s}");
    }

    #[test]
    fn detail_surfaces_parent_step_when_present() {
        let mut track = t(7, Kind::ModelCall, json!({"vendor": "x", "model": "y"}));
        track.parent_step = Some(3);
        let s = render_track_detail(&track, RedactionStatus::NotProcessed);
        assert!(s.contains("parent_step:    3"), "{s}");
    }

    #[test]
    fn detail_omits_parent_step_line_when_none() {
        let track = t(7, Kind::ModelCall, json!({"vendor": "x", "model": "y"}));
        let s = render_track_detail(&track, RedactionStatus::NotProcessed);
        assert!(!s.contains("parent_step:"), "{s}");
    }

    #[test]
    fn detail_surfaces_refs_when_non_empty() {
        let mut track = t(7, Kind::FileRead, json!({"path": "/a"}));
        track.refs = vec!["sha256:abc".to_owned(), "sha256:def".to_owned()];
        let s = render_track_detail(&track, RedactionStatus::NotProcessed);
        assert!(s.contains("refs:           sha256:abc, sha256:def"), "{s}");
    }

    #[test]
    fn detail_omits_refs_line_when_empty() {
        let track = t(7, Kind::FileRead, json!({"path": "/a"}));
        let s = render_track_detail(&track, RedactionStatus::NotProcessed);
        assert!(!s.contains("refs:"), "{s}");
    }

    #[test]
    fn detail_renders_all_three_redaction_states() {
        let track = t(7, Kind::Task, json!({"prompt": "x"}));
        let s = render_track_detail(&track, RedactionStatus::NotProcessed);
        assert!(s.contains("redaction:      not processed"), "{s}");
        let s = render_track_detail(&track, RedactionStatus::NoneApplied);
        assert!(s.contains("redaction:      none applied"), "{s}");
        let s = render_track_detail(&track, RedactionStatus::Applied(3));
        assert!(
            s.contains("redaction:      3 replacement(s) applied (cassette-wide)"),
            "{s}"
        );
    }

    #[test]
    fn detail_renders_annotation_list_when_non_empty() {
        let mut track = t(7, Kind::Task, json!({"prompt": "x"}));
        track.annotations = vec![
            Annotation {
                by: "colin".to_owned(),
                note: "wrong tool choice, retry".to_owned(),
            },
            Annotation {
                by: "agent".to_owned(),
                note: "follow up".to_owned(),
            },
        ];
        let s = render_track_detail(&track, RedactionStatus::NotProcessed);
        assert!(s.contains("annotations:    2"), "{s}");
        assert!(
            s.contains(r#"- by: colin    note: "wrong tool choice, retry""#),
            "{s}"
        );
        assert!(s.contains(r#"- by: agent    note: "follow up""#), "{s}");
    }

    #[test]
    fn detail_renders_zero_annotations() {
        let track = t(7, Kind::Task, json!({"prompt": "x"}));
        let s = render_track_detail(&track, RedactionStatus::NotProcessed);
        assert!(s.contains("annotations:    0"), "{s}");
    }

    #[test]
    fn detail_pretty_prints_full_payload_under_divider() {
        let track = t(
            7,
            Kind::ModelCall,
            json!({"vendor": "anthropic", "model": "claude-opus-4-7", "tokens_in": 1234}),
        );
        let s = render_track_detail(&track, RedactionStatus::NotProcessed);
        assert!(s.contains("── payload ──"), "{s}");
        assert!(s.contains("\"vendor\": \"anthropic\""), "{s}");
        assert!(s.contains("\"tokens_in\": 1234"), "{s}");
    }

    #[test]
    fn index_summary_renders_header_and_counts() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::ModelCall, json!({"vendor": "x", "model": "y"})),
            t(3, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_index_summary("bug.tape", &tracks, RedactionStatus::Applied(2));
        assert!(s.contains("══ tape index: bug.tape ══"), "{s}");
        assert!(s.contains("tracks:         3"), "{s}");
        assert!(s.contains("first_ts:       2026-05-06T10:00:01Z"), "{s}");
        assert!(s.contains("last_ts:        2026-05-06T10:00:03Z"), "{s}");
        assert!(s.contains("redactions:     2 (cassette-wide)"), "{s}");
    }

    #[test]
    fn index_summary_lists_all_eight_kinds_in_enum_order() {
        let tracks = vec![
            t(1, Kind::Task, json!({"prompt": "x"})),
            t(2, Kind::Eject, json!({"outcome": "success"})),
        ];
        let s = render_index_summary("x.tape", &tracks, RedactionStatus::NotProcessed);
        let hist_section = s
            .split("── kind histogram ──")
            .nth(1)
            .expect("histogram section present");
        // Every kind must show up — including zero-count rows.
        for k in [
            "task",
            "model_call",
            "mcp_call",
            "shell",
            "file_read",
            "file_write",
            "annotation",
            "eject",
        ] {
            assert!(hist_section.contains(k), "missing kind {k}: {s}");
        }
        // Ordering: each kind's position in the rendered text must
        // match the enum declaration order, not alphabetical.
        let order = [
            "task",
            "model_call",
            "mcp_call",
            "shell",
            "file_read",
            "file_write",
            "annotation",
            "eject",
        ];
        let mut last_idx = 0usize;
        for name in order {
            let idx = hist_section.find(name).unwrap();
            assert!(idx >= last_idx, "{name} out of order: {s}");
            last_idx = idx;
        }
    }

    #[test]
    fn index_summary_omits_ts_lines_on_empty_cassette() {
        let s = render_index_summary("empty.tape", &[], RedactionStatus::NotProcessed);
        assert!(s.contains("tracks:         0"), "{s}");
        assert!(!s.contains("first_ts:"), "{s}");
        assert!(!s.contains("last_ts:"), "{s}");
        // Histogram still prints with all zeros.
        assert!(s.contains("── kind histogram ──"), "{s}");
        assert!(s.contains("task          0"), "{s}");
        assert!(s.contains("eject         0"), "{s}");
    }

    #[test]
    fn index_summary_renders_all_three_redaction_states() {
        let tracks = vec![t(1, Kind::Task, json!({"prompt": "x"}))];
        let s = render_index_summary("x.tape", &tracks, RedactionStatus::NotProcessed);
        assert!(s.contains("redactions:     not processed"), "{s}");
        let s = render_index_summary("x.tape", &tracks, RedactionStatus::NoneApplied);
        assert!(s.contains("redactions:     none applied"), "{s}");
        let s = render_index_summary("x.tape", &tracks, RedactionStatus::Applied(5));
        assert!(s.contains("redactions:     5 (cassette-wide)"), "{s}");
    }
}

// =====================================================================
// Phase 1 of #100 (carved per #250): `tape watch` renderer + helpers.
// =====================================================================

#[cfg(test)]
mod watch_tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn epoch_at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1536), "1.5 KiB");
        assert_eq!(format_size(1024 * 1024 + 1024 * 512), "1.5 MiB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GiB");
    }

    #[test]
    fn format_relative_time_buckets() {
        let now = epoch_at(1_000_000);
        assert_eq!(format_relative_time(epoch_at(1_000_000), now), "now");
        assert_eq!(format_relative_time(epoch_at(999_999), now), "now");
        assert_eq!(format_relative_time(epoch_at(999_958), now), "42s ago");
        assert_eq!(format_relative_time(epoch_at(999_700), now), "5m ago");
        assert_eq!(format_relative_time(epoch_at(992_800), now), "2h ago");
        assert_eq!(format_relative_time(epoch_at(913_600), now), "1d ago");
        // Clock skew: `then` after `now` collapses to "now" rather
        // than panicking on a signed-overflow.
        assert_eq!(format_relative_time(epoch_at(1_000_100), now), "now");
    }

    #[test]
    fn render_watch_two_row_snapshot() {
        let now = epoch_at(1_000_000);
        let rows = vec![
            WatchRow {
                path: PathBuf::from("a.tape"),
                size: 4_300,
                modified: epoch_at(999_997),
                tracks: Some(12),
            },
            WatchRow {
                path: PathBuf::from("b.tape"),
                size: 17,
                modified: epoch_at(999_700),
                tracks: None,
            },
        ];
        let rendered = render_watch(&rows, now);
        // Header is present.
        assert!(rendered.contains("path"), "{rendered}");
        assert!(rendered.contains("size"), "{rendered}");
        assert!(rendered.contains("modified"), "{rendered}");
        assert!(rendered.contains("tracks"), "{rendered}");
        // First row: parsed tracks count.
        assert!(rendered.contains("a.tape"), "{rendered}");
        assert!(rendered.contains("4.2 KiB"), "{rendered}");
        assert!(rendered.contains("3s ago"), "{rendered}");
        // Second row: unparsed → em-dash.
        assert!(rendered.contains("b.tape"), "{rendered}");
        assert!(rendered.contains("17 B"), "{rendered}");
        assert!(rendered.contains("5m ago"), "{rendered}");
        assert!(rendered.contains("—"), "{rendered}");
    }

    #[test]
    fn render_watch_empty_input_renders_header_only() {
        let rendered = render_watch(&[], epoch_at(1_000_000));
        // Just the header line — no row data.
        assert!(rendered.contains("path"));
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 1);
    }
}
