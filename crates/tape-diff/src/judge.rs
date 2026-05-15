//! Judge-narrated diff: ask an LLM to describe the behavioral delta
//! between each pair of aligned tracks the structural classifier marked
//! `Substantive`.
//!
//! This is the first downstream consumer of `tape-judge` (#149). The
//! structural diff stays authoritative — narration is purely additive
//! and never affects exit codes or alignment classification.

use tape_judge::record::ScanOutcome;
use tape_judge::{JudgeCallRecord, JudgeClient, JudgeError, JudgeOpts};

use crate::{AlignedPair, Class, Diff};

/// Result of one narration request. Always exactly one of `narration_text`
/// or `note` is populated; `record` is set only on a successful judge call.
struct PairOutcome {
    narration_text: Option<String>,
    record: Option<JudgeCallRecord>,
}

/// Walk a diff and narrate every `Substantive` pair, in order, until the
/// per-invocation budget is exhausted. Modifies `diff.alignment[*].narration`
/// in place and appends successful audit rows to `diff.judge_calls`.
///
/// `max_input_chars` mirrors `JudgeConfig::max_input_chars` and is the
/// pre-call gate that turns oversized prompts into a
/// `[narration skipped — input exceeds max_input_chars]` marker rather
/// than relying on the client to truncate the prompt itself (AC #4).
///
/// Returns the count of judge calls actually made (i.e. attempts that
/// reached the upstream — retries inside one call don't add to this).
pub async fn narrate_substantive_pairs(
    diff: &mut Diff,
    client: &JudgeClient,
    budget: u32,
    max_input_chars: usize,
) -> u32 {
    let mut calls_used: u32 = 0;
    let mut new_records: Vec<JudgeCallRecord> = Vec::new();

    for pair in &mut diff.alignment {
        if pair.class != Class::Substantive {
            continue;
        }
        // Budget guardrail. Once we cross the cap, every remaining
        // Substantive pair is annotated with a "budget exceeded" note
        // so the user can see why the latter half of the diff is
        // unnarrated, then we stop spending calls. AC #5.
        if calls_used >= budget {
            pair.narration = Some(BUDGET_EXCEEDED.to_owned());
            continue;
        }

        let prompt = render_prompt(pair);
        // Pre-flight truncation gate. The judge client *would* truncate
        // silently and stamp `truncated: true` on the audit row, but the
        // acceptance criteria specifically forbid that for the diff
        // consumer — the prompt is structured small text, and silent
        // truncation here would mean handing the model the head of A
        // and zero context for B, producing misleading narration. AC #4.
        if prompt.chars().count() > max_input_chars {
            pair.narration = Some(INPUT_TOO_LONG.to_owned());
            continue;
        }

        let outcome = narrate_one(client, &prompt).await;
        calls_used += 1;
        pair.narration = outcome.narration_text;
        if let Some(record) = outcome.record {
            new_records.push(record);
        }
    }

    diff.judge_calls.extend(new_records);
    calls_used
}

const BUDGET_EXCEEDED: &str = "[narration skipped — budget exceeded]";
const INPUT_TOO_LONG: &str = "[narration skipped — input exceeds max_input_chars]";

async fn narrate_one(client: &JudgeClient, prompt: &str) -> PairOutcome {
    match client.complete(prompt, JudgeOpts::default()).await {
        Ok(out) => {
            let trimmed = out.text.trim();
            // The prompt instructs the model to emit literal `N/A` when
            // the change is cosmetic noise. Collapse those to `None` so
            // the rendered text output stays clean.
            let text = if trimmed.eq_ignore_ascii_case("n/a") {
                None
            } else {
                Some(trimmed.to_owned())
            };
            PairOutcome {
                narration_text: text,
                record: Some(out.record),
            }
        }
        Err(JudgeError::Rejected(hit)) => PairOutcome {
            narration_text: Some(format!(
                "[narration redacted — defense-in-depth scanner: {}]",
                hit.rule_id
            )),
            // Synthesize a partial record so callers can still see that
            // a call happened and was rejected. `prompt_hash`/
            // `output_hash` use the same blake3 helper from
            // `tape_judge::record`.
            record: Some(JudgeCallRecord {
                ts: chrono::Utc::now()
                    .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                    .to_string(),
                model: "<rejected>".to_owned(),
                prompt_hash: tape_judge::record::hash_blake3(prompt),
                output_hash: String::new(),
                scan_result: ScanOutcome::Rejected {
                    rule_id: hit.rule_id.to_owned(),
                },
                retry_count: 0,
                truncated: false,
            }),
        },
        Err(e) => PairOutcome {
            narration_text: Some(format!("[narration failed: {e}]")),
            record: None,
        },
    }
}

fn render_prompt(pair: &AlignedPair) -> String {
    let a = pair.a_label.as_deref().unwrap_or("(missing)");
    let b = pair.b_label.as_deref().unwrap_or("(missing)");
    format!(
        "You are reviewing the behavioral difference between two steps of an agent recording.\n\
         \n\
         Track A: {a}\n\
         Track B: {b}\n\
         \n\
         Describe the behavioral delta between A and B in one to three short sentences. \
         Focus on what the agent did differently, not on the structural shape of the tracks. \
         Be concrete: name the user-visible difference, not metaphors. Plain text, no markdown.\n\
         \n\
         If the change is purely cosmetic (whitespace, ordering, formatting), output exactly: N/A\n\
         Do not speculate beyond what is visible above."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(class: Class, label_a: &str, label_b: &str) -> AlignedPair {
        AlignedPair {
            a_step: Some(1),
            b_step: Some(1),
            class,
            narration: None,
            downstream_b: vec![],
            a_label: Some(label_a.into()),
            b_label: Some(label_b.into()),
        }
    }

    #[test]
    fn prompt_includes_both_labels() {
        let p = pair(
            Class::Substantive,
            "model_call:anthropic/x in:5",
            "shell:ls",
        );
        let text = render_prompt(&p);
        assert!(
            text.contains("Track A: model_call:anthropic/x in:5"),
            "{text}"
        );
        assert!(text.contains("Track B: shell:ls"), "{text}");
        assert!(text.contains("output exactly: N/A"), "{text}");
    }
}
