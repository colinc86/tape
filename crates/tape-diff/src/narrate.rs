//! Judge-narrated alignment — the `--judge` mode of `tape diff`.
//!
//! Wires `tape_judge::JudgeClient` into the existing structural diff so
//! `Class::Substantive` entries get a one-to-three-sentence prose
//! description of the **behavioral** delta attached as
//! [`AlignedPair::narration`]. The structural classifier is unchanged —
//! every taxonomy decision is still made by [`crate::classify_pair`].
//!
//! ## Decoration rules (issue #149 ACs 3–5)
//!
//! - `[narration skipped — input exceeds max_input_chars]` — the entry's
//!   prompt is larger than `JudgeConfig::max_input_chars` (AC4). The
//!   structural diff still renders normally; no judge call is made.
//! - `[narration skipped — budget exceeded]` — the per-invocation
//!   `--judge-budget` cap has been reached. Remaining substantive
//!   entries get this marker (AC5). The structural diff still renders.
//! - `[narration redacted — defense-in-depth scanner: <rule>]` — the
//!   judge response triggered [`tape_judge::scan_for_injection`] (AC3).
//!   The structural diff's exit code is unchanged; the structural diff
//!   is still authoritative.
//! - Otherwise, the narration prose is attached verbatim. The prompt
//!   instructs the model to emit a literal `N/A` for purely structural
//!   noise so noisy entries collapse cleanly in the rendered output
//!   (AC2).
//!
//! Cassettes are **not** mutated by this path — every audit row goes
//! onto [`crate::Diff::judge_calls`] (in-memory only). The user can
//! redirect `--format json` output to a file to persist them.

use serde_json::Value;
use tape_judge::{JudgeCallRecord, JudgeClient, JudgeError, JudgeOpts};

use crate::{AlignedPair, Class, Diff};

/// The narration prompt template. One template, in-tree, no expression
/// language — the rule is "grep `{{` should show every active
/// placeholder". The prompt:
///
/// - Asks for the **behavioral** delta (not the structural fields the
///   user can already see).
/// - Caps the response at one-to-three sentences so noisy entries
///   collapse to a single line.
/// - Tells the model to refuse to speculate beyond the provided
///   context. The structural classifier already made the taxonomy
///   decision; the model's job is prose, not classification.
/// - Tells the model to emit a literal `N/A` when the change is purely
///   structural noise (whitespace, formatting, reorderings) so the
///   text renderer can elide the line in a future pass.
pub const NARRATION_PROMPT: &str = "\
You are summarizing the behavioral delta between two agent steps from \
a `tape` cassette diff. Describe in 1–3 short sentences what changed \
between the BEFORE and AFTER step that a reader of the cassette would \
need to know — focus on intent, tool choice, argument substance, or \
response content. Do NOT restate the step kind or step number. Do NOT \
speculate beyond the provided context. If the change is purely \
structural noise (whitespace, key ordering, equivalent rewordings) \
with no behavioral effect, reply with the literal string `N/A` and \
nothing else.

BEFORE (step {{a_step}}, {{a_kind}}):
{{a_payload}}

AFTER (step {{b_step}}, {{b_kind}}):
{{b_payload}}";

/// What [`Narrator`] decided to do for one substantive entry. Embedded
/// into [`AlignedPair::narration`] as text + (for `Narrated`) appended
/// to [`Diff::judge_calls`].
#[derive(Debug)]
enum EntryOutcome {
    Narrated {
        text: String,
        record: JudgeCallRecord,
    },
    SkippedOversize,
    SkippedBudget,
    Redacted {
        rule_id: String,
    },
}

/// Per-invocation budget governor. The CLI sets the cap; the
/// [`Narrator::narrate_pair`] call decrements it for each judge call
/// made (successful or scanner-rejected — a rejected call still
/// consumed an upstream attempt-quota).
pub struct Budget {
    remaining: u32,
}

impl Budget {
    /// Build a fresh budget. A cap of `0` disables narration entirely;
    /// every substantive entry gets a `budget exceeded` marker.
    pub fn new(cap: u32) -> Self {
        Self { remaining: cap }
    }

    fn try_consume(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

/// Decorate every `Class::Substantive` entry in `diff` with a judge
/// narration, applying the budget + scanner + truncation rules above.
/// All other entry classes are untouched.
///
/// Returns the number of judge calls made (handy for budget reporting
/// and tests). Audit rows for every successful call are appended to
/// `diff.judge_calls` in place; rejected/skipped entries do not add an
/// audit row (the embedded marker is the user-visible record).
///
/// Designed for sequential iteration — the budget is per-invocation
/// and the calls are inherently order-dependent (the user's eye
/// reads entries top-to-bottom; the budget should drain in the same
/// direction so the visible cap behaves intuitively).
pub async fn narrate_diff(
    diff: &mut Diff,
    client: &JudgeClient,
    max_input_chars: usize,
    budget: &mut Budget,
) -> Result<u32, JudgeError> {
    let mut calls_made: u32 = 0;
    // Index-based iteration so we can both mutate `diff.alignment[i]`
    // and push onto `diff.judge_calls` (which would otherwise alias
    // `&mut` borrows).
    for i in 0..diff.alignment.len() {
        if diff.alignment[i].class != Class::Substantive {
            continue;
        }
        let outcome = narrate_one(&diff.alignment[i], client, max_input_chars, budget).await?;
        // `narrate_one` only returns `Narrated` when an HTTP call
        // landed; account for the increment here so the caller sees
        // it.
        match outcome {
            EntryOutcome::Narrated { text, record } => {
                diff.alignment[i].narration = Some(text);
                diff.judge_calls.push(record);
                calls_made += 1;
            }
            EntryOutcome::SkippedOversize => {
                diff.alignment[i].narration =
                    Some("[narration skipped — input exceeds max_input_chars]".to_owned());
            }
            EntryOutcome::SkippedBudget => {
                diff.alignment[i].narration =
                    Some("[narration skipped — budget exceeded]".to_owned());
            }
            EntryOutcome::Redacted { rule_id } => {
                diff.alignment[i].narration = Some(format!(
                    "[narration redacted — defense-in-depth scanner: {rule_id}]"
                ));
                // A scanner-rejected call still hit the network; count
                // it against the cap so a pathological response stream
                // can't drain the upstream budget silently.
                calls_made += 1;
            }
        }
    }
    Ok(calls_made)
}

async fn narrate_one(
    pair: &AlignedPair,
    client: &JudgeClient,
    max_input_chars: usize,
    budget: &mut Budget,
) -> Result<EntryOutcome, JudgeError> {
    if !budget.try_consume() {
        return Ok(EntryOutcome::SkippedBudget);
    }
    let prompt = build_prompt(pair);
    if prompt.chars().count() > max_input_chars {
        // AC4: oversize entries are explicitly skipped — no silent
        // head-truncation, since the cut would erase the AFTER context
        // the narration depends on. The structural classifier already
        // made the decision; the narration is a "nice-to-have" that
        // gracefully degrades.
        return Ok(EntryOutcome::SkippedOversize);
    }

    match client.complete(&prompt, JudgeOpts::default()).await {
        Ok(out) => Ok(EntryOutcome::Narrated {
            text: out.text.trim().to_owned(),
            record: out.record,
        }),
        Err(JudgeError::Rejected(hit)) => Ok(EntryOutcome::Redacted {
            rule_id: hit.rule_id.to_owned(),
        }),
        Err(other) => Err(other),
    }
}

fn build_prompt(pair: &AlignedPair) -> String {
    // The aligner stores only `a_step` / `b_step` / `a_label` /
    // `b_label` on the pair itself — we don't get full payloads here.
    // Caller plumbs payloads through via the rendered labels and the
    // step numbers; that's enough to convey the behavioral intent. A
    // future ticket can re-thread the full track payloads through if
    // the label-only context turns out to be too thin for high-fidelity
    // narration.
    let a_step = pair.a_step.map_or("?".to_owned(), |s| s.to_string());
    let b_step = pair.b_step.map_or("?".to_owned(), |s| s.to_string());
    let a_label = pair.a_label.as_deref().unwrap_or("(missing)");
    let b_label = pair.b_label.as_deref().unwrap_or("(missing)");
    NARRATION_PROMPT
        .replace("{{a_step}}", &a_step)
        .replace("{{b_step}}", &b_step)
        .replace("{{a_kind}}", "step")
        .replace("{{b_kind}}", "step")
        .replace("{{a_payload}}", a_label)
        .replace("{{b_payload}}", b_label)
}

/// Inspect a `serde_json` payload for a "trim me first" pass. Used
/// by callers (e.g. the CLI) that have access to the original
/// `Track` payloads and want to give the model fuller context than
/// the short label-only prompt above. Kept tiny on purpose — JSON
/// `Value::to_string` already minifies.
pub fn payload_to_prompt_context(v: &Value) -> String {
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlignedPair, Class};

    fn pair_with_labels(a: &str, b: &str) -> AlignedPair {
        AlignedPair {
            a_step: Some(3),
            b_step: Some(3),
            class: Class::Substantive,
            narration: None,
            downstream_b: vec![],
            a_label: Some(a.to_owned()),
            b_label: Some(b.to_owned()),
        }
    }

    #[test]
    fn budget_zero_blocks_first_call() {
        let mut b = Budget::new(0);
        assert!(!b.try_consume());
        assert!(!b.try_consume());
    }

    #[test]
    fn budget_drains_then_blocks() {
        let mut b = Budget::new(2);
        assert!(b.try_consume());
        assert!(b.try_consume());
        assert!(!b.try_consume());
    }

    #[test]
    fn prompt_substitutes_step_and_labels() {
        let pair = pair_with_labels("read foo.rs", "read bar.rs");
        let prompt = build_prompt(&pair);
        assert!(prompt.contains("read foo.rs"), "{prompt}");
        assert!(prompt.contains("read bar.rs"), "{prompt}");
        assert!(prompt.contains("step 3"), "{prompt}");
        // No unfilled placeholders.
        assert!(!prompt.contains("{{"), "unfilled placeholders in: {prompt}");
    }

    #[test]
    fn narration_prompt_has_all_placeholders() {
        // Pin the placeholder set so a future template edit doesn't
        // silently drop a substitution.
        for tok in ["{{a_step}}", "{{b_step}}", "{{a_payload}}", "{{b_payload}}"] {
            assert!(
                NARRATION_PROMPT.contains(tok),
                "missing placeholder {tok} in NARRATION_PROMPT"
            );
        }
    }
}
