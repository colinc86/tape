//! `tape anon` — strip identifying tokens from a cassette and write a
//! new cassette. Phase 1 of issue #42; ships exactly one rule
//! (`unix_home_path`). See `crates/tape-anon/src/rules.rs` for the
//! ruleset and the ticket text for what's deliberately deferred.
//!
//! Public surface:
//! - [`AnonOptions`] — input/output paths.
//! - [`run_anon`] — end-to-end orchestrator: read input, walk all
//!   anonymizable surfaces, write to a tmp path, re-scan the tmp for
//!   leftover identifiers, on pass `rename(tmp, out)`, on leak delete
//!   tmp + return [`AnonError::PostAnonLeak`].

pub mod engine;
pub mod pseudonym;
pub mod rules;

use std::path::{Path, PathBuf};

use crate::engine::{anonymize_string, anonymize_value};
use crate::pseudonym::Pseudonymizer;
use crate::rules::{built_in_rules, AnonRule};

/// Input/output configuration for [`run_anon`].
#[derive(Debug, Clone)]
pub struct AnonOptions {
    pub in_path: PathBuf,
    pub out_path: PathBuf,
}

/// Per-invocation summary for the CLI stderr line.
#[derive(Debug, Default)]
pub struct RunReport {
    pub n_replacements: usize,
    /// How many spilled `artifacts/` entries were left untouched
    /// (Phase 1 does not scan binary content — see ticket §"What the
    /// engine walks").
    pub n_artifacts_skipped: usize,
}

#[derive(Debug)]
pub enum AnonError {
    /// Input cassette failed to open or parse (mirrors `tape verify`'s
    /// exit-3 path).
    InputUnreadable(anyhow::Error),
    /// `meta.yaml` / `liner-notes.md` / `tracks.jsonl` failed to
    /// re-serialize after mutation.
    Serialize(anyhow::Error),
    /// Defense-in-depth re-scan found a match in the would-be output —
    /// abort, leave nothing on disk at the output path.
    PostAnonLeak {
        rule_id: &'static str,
        field_path: String,
        step: u64,
        sample: String,
    },
    /// Writer / filesystem error.
    Io(anyhow::Error),
}

impl std::fmt::Display for AnonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputUnreadable(e) => write!(f, "tape anon: failed to read input cassette: {e}"),
            Self::Serialize(e) => write!(f, "tape anon: failed to re-serialize after anon: {e}"),
            Self::PostAnonLeak {
                rule_id,
                field_path,
                step,
                sample,
            } => write!(
                f,
                "tape anon: LEAKED_IDENTIFIER_POST_ANON\n  rule_id: {rule_id}\n  field_path: {field_path}\n  step: {step}\n  sample: \"{sample}\"",
            ),
            Self::Io(e) => write!(f, "tape anon: I/O error: {e}"),
        }
    }
}

impl std::error::Error for AnonError {}

/// End-to-end Phase-1 anon. See module docs for the contract.
pub fn run_anon(opts: AnonOptions) -> Result<RunReport, AnonError> {
    let rules = built_in_rules();
    let mut pseudo = Pseudonymizer::new().map_err(AnonError::Io)?;
    run_anon_with(opts, &rules, &mut pseudo)
}

/// Lower-level entry point exposed for tests that need to supply a
/// deterministic salt. The `--salt` CLI flag is Phase 2+ work; until
/// then the public CLI surface always uses the random-salt path
/// through [`run_anon`].
pub fn run_anon_with(
    opts: AnonOptions,
    rules: &[AnonRule],
    pseudo: &mut Pseudonymizer,
) -> Result<RunReport, AnonError> {
    // 1. Read input cassette.
    let raw = tape_format::reader::RawTape::open(&opts.in_path)
        .map_err(|e| AnonError::InputUnreadable(anyhow::anyhow!("{e}")))?;

    // 2. Anonymize meta.yaml as plain text (the ticket explicitly
    //    accepts this rather than a Meta-struct mutation — see
    //    "What the engine walks" in #204; the home-path scan operates
    //    on rendered text identically across `task`, `recap`,
    //    `label`, `tags[]`, `recaps[].prior_recap`/`new_recap`,
    //    `relinernotes[]` text fields).
    let mut n = 0;
    let mut meta_yaml = raw
        .meta_yaml
        .clone()
        .ok_or_else(|| AnonError::InputUnreadable(anyhow::anyhow!("missing meta.yaml")))?;
    n += anonymize_string(rules, pseudo, &mut meta_yaml);

    // 3. Anonymize liner-notes.md as a single text body.
    let mut liner_md = raw.liner_md.clone().unwrap_or_default();
    n += anonymize_string(rules, pseudo, &mut liner_md);

    // 4. Anonymize every track payload Value. We re-parse the JSONL
    //    rather than walking via tape-format's typed Track so the
    //    walker stays unbiased about which kinds carry user-text
    //    (Phase-1 wants to scrub every string in every payload).
    let mut tracks_lines: Vec<String> = Vec::new();
    if let Some(tracks_jsonl) = raw.tracks_jsonl.as_deref() {
        for line in tracks_jsonl.lines() {
            if line.is_empty() {
                continue;
            }
            let mut v: serde_json::Value = serde_json::from_str(line).map_err(|e| {
                AnonError::InputUnreadable(anyhow::anyhow!("tracks.jsonl line not JSON: {e}"))
            })?;
            // Mutate only payload + annotations[].note (the two
            // user-text-bearing fields per SPEC §5). Skip ts/step/
            // kind/parent_step which are structural.
            if let Some(payload) = v.get_mut("payload") {
                n += anonymize_value(rules, pseudo, payload);
            }
            if let Some(annots) = v.get_mut("annotations") {
                n += anonymize_value(rules, pseudo, annots);
            }
            let serialized = serde_json::to_string(&v)
                .map_err(|e| AnonError::Serialize(anyhow::anyhow!("{e}")))?;
            tracks_lines.push(serialized);
        }
    }
    let mut tracks_jsonl = tracks_lines.join("\n");
    if !tracks_jsonl.is_empty() {
        tracks_jsonl.push('\n');
    }

    // 5. redactions.json: leave untouched (per ticket — Phase 1 does
    //    not rewrite the audit log; doing so could break audit chains).
    // 6. Artifacts: leave untouched (Phase 1 doesn't scan binary
    //    content; that's --aggressive in Phase 4).
    let n_artifacts_skipped = raw.artifacts.len();

    // 7. Build the would-be output and write it to a tmp path so the
    //    leak rescan happens BEFORE the final output appears. Per
    //    ticket open Q3.
    let tmp_path = tmp_path_for(&opts.out_path);
    let pending = tape_format::writer::PendingTape {
        meta_yaml: meta_yaml.clone(),
        liner_md: liner_md.clone(),
        tracks_jsonl: tracks_jsonl.clone(),
        redactions_json: raw.redactions_json.clone(),
        artifacts: raw.artifacts.clone().into_iter().collect(),
    };
    pending
        .write_to(&tmp_path)
        .map_err(|e| AnonError::Io(anyhow::anyhow!("write {}: {e}", tmp_path.display())))?;

    // 8. Defense-in-depth re-scan. Walk the post-anon text + payloads
    //    once more with the same rule set; any match is a bug — abort
    //    and delete the tmp file so the output path stays empty.
    if let Some(leak) = scan_for_leak(rules, &meta_yaml, &liner_md, &tracks_jsonl)? {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(leak);
    }

    // 9. Promote tmp → final via atomic rename.
    std::fs::rename(&tmp_path, &opts.out_path).map_err(|e| {
        AnonError::Io(anyhow::anyhow!(
            "rename {} → {}: {e}",
            tmp_path.display(),
            opts.out_path.display()
        ))
    })?;

    Ok(RunReport {
        n_replacements: n,
        n_artifacts_skipped,
    })
}

/// Compute the tmp-path next to `out` that the writer atomic-rename
/// pivots on.
fn tmp_path_for(out: &Path) -> PathBuf {
    let mut s = out.as_os_str().to_owned();
    s.push(".anon.tmp");
    PathBuf::from(s)
}

/// Defense-in-depth re-scan. Returns `Some(PostAnonLeak)` if any rule
/// finds a match in the post-anon surfaces. Symmetric with
/// `tape_redact::Engine::scan` but scoped to the anon ruleset and to
/// the surfaces anon walked.
fn scan_for_leak(
    rules: &[AnonRule],
    meta_yaml: &str,
    liner_md: &str,
    tracks_jsonl: &str,
) -> Result<Option<AnonError>, AnonError> {
    for rule in rules {
        if let Some(m) = rule.regex.find(meta_yaml) {
            return Ok(Some(AnonError::PostAnonLeak {
                rule_id: rule.id,
                field_path: "meta.yaml".to_owned(),
                step: 0,
                sample: sample(m.as_str()),
            }));
        }
        if let Some(m) = rule.regex.find(liner_md) {
            return Ok(Some(AnonError::PostAnonLeak {
                rule_id: rule.id,
                field_path: "liner-notes.md".to_owned(),
                step: 0,
                sample: sample(m.as_str()),
            }));
        }
        for (i, line) in tracks_jsonl.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            if let Some(m) = rule.regex.find(line) {
                // Try to extract the step number; fall back to line index.
                let step = serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|v| v.get("step").and_then(serde_json::Value::as_u64))
                    .unwrap_or((i as u64) + 1);
                return Ok(Some(AnonError::PostAnonLeak {
                    rule_id: rule.id,
                    field_path: format!("tracks[{i}]"),
                    step,
                    sample: sample(m.as_str()),
                }));
            }
        }
    }
    Ok(None)
}

fn sample(s: &str) -> String {
    let mut out = String::with_capacity(32);
    for c in s.chars().take(32) {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}
