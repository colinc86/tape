//! Stream a Claude Code session JSONL and yield typed `RawEntry` values.
//! Permissive — unknown `type` discriminators map to `RawEntry::Skip` plus a
//! warning rather than a parse error.

use std::io::BufRead;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub enum RawEntry {
    User(UserEntry),
    Assistant(AssistantEntry),
    /// `system`, `permission-mode`, `ai-title`, `attachment`,
    /// `file-history-snapshot`, `last-prompt`, `queue-operation`, `agent-name`,
    /// or anything else we don't convert.
    Skip {
        kind: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserEntry {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default, rename = "promptId")]
    pub prompt_id: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    pub message: UserMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserMessage {
    #[serde(default)]
    pub role: Option<String>,
    /// Either a bare string (top-level user prompt) or an array of content
    /// blocks (e.g. `tool_result` follow-ups). Stored as `Value` so the
    /// converter can match on shape.
    pub content: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantEntry {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default, rename = "requestId")]
    pub request_id: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    pub message: AssistantMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<Value>,
    /// Array of content blocks: `text`, `thinking`, or `tool_use`.
    pub content: Vec<Value>,
}

/// Counts of skipped / unknown event types — surfaced in `meta.task` or as
/// an annotation track at convert time.
#[derive(Debug, Default, Clone)]
pub struct ParseReport {
    pub total_lines: u64,
    pub user_count: u64,
    pub assistant_count: u64,
    /// Map from unknown `type` value → count.
    pub skipped: std::collections::BTreeMap<String, u64>,
    /// Lines that failed JSON parsing entirely.
    pub malformed_lines: u64,
}

/// Stream-parse a JSONL transcript. Returns the kept entries plus a
/// `ParseReport`. Never panics on malformed lines — they're counted.
pub fn parse_jsonl<R: BufRead>(reader: R) -> std::io::Result<(Vec<RawEntry>, ParseReport)> {
    let mut entries = Vec::new();
    let mut report = ParseReport::default();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        report.total_lines += 1;

        // Best-effort JSON parse. Anything that doesn't parse is counted and skipped.
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                report.malformed_lines += 1;
                continue;
            }
        };

        let kind = v.get("type").and_then(Value::as_str).unwrap_or("missing");
        match kind {
            "user" => match serde_json::from_value::<UserEntry>(v) {
                Ok(e) => {
                    report.user_count += 1;
                    entries.push(RawEntry::User(e));
                }
                Err(_) => {
                    report.malformed_lines += 1;
                }
            },
            "assistant" => match serde_json::from_value::<AssistantEntry>(v) {
                Ok(e) => {
                    report.assistant_count += 1;
                    entries.push(RawEntry::Assistant(e));
                }
                Err(_) => {
                    report.malformed_lines += 1;
                }
            },
            other => {
                *report.skipped.entry(other.to_string()).or_insert(0) += 1;
                entries.push(RawEntry::Skip {
                    kind: other.to_string(),
                });
            }
        }
    }

    Ok((entries, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    fn parse_str(s: &str) -> (Vec<RawEntry>, ParseReport) {
        parse_jsonl(BufReader::new(s.as_bytes())).unwrap()
    }

    #[test]
    fn parses_minimal_user_assistant() {
        let s = include_str!("../../../../tests/fixtures/transcripts/minimal.jsonl");
        let (entries, report) = parse_str(s);
        assert_eq!(report.total_lines, 2);
        assert_eq!(report.user_count, 1);
        assert_eq!(report.assistant_count, 1);
        assert_eq!(report.malformed_lines, 0);
        assert_eq!(entries.len(), 2);
        match &entries[0] {
            RawEntry::User(u) => match &u.message.content {
                Value::String(s) => assert_eq!(s, "Say hello"),
                _ => panic!("expected string content"),
            },
            _ => panic!("expected user"),
        }
    }

    #[test]
    fn parses_tool_use_blocks() {
        let s = include_str!("../../../../tests/fixtures/transcripts/with_bash.jsonl");
        let (entries, _report) = parse_str(s);
        let assistant = entries
            .iter()
            .find_map(|e| match e {
                RawEntry::Assistant(a) => Some(a),
                _ => None,
            })
            .unwrap();
        let tool_use = assistant
            .message
            .content
            .iter()
            .find(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
            .unwrap();
        assert_eq!(tool_use["name"], "Bash");
        assert_eq!(tool_use["input"]["command"], "ls /tmp");
    }

    #[test]
    fn unknown_type_is_skipped_with_count() {
        let s = include_str!("../../../../tests/fixtures/transcripts/unknown_type.jsonl");
        let (entries, report) = parse_str(s);
        assert_eq!(report.user_count, 1);
        assert_eq!(report.assistant_count, 1);
        assert_eq!(report.skipped.get("future-thing"), Some(&1));
        // skip entries are still in the vec for ordering visibility
        assert!(entries.iter().any(|e| matches!(e, RawEntry::Skip { .. })));
    }

    #[test]
    fn tolerates_malformed_line() {
        let s = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"ok\"}}\nthis is not json\n{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"x\"}]}}\n";
        let (entries, report) = parse_str(s);
        assert_eq!(report.malformed_lines, 1);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn user_content_can_be_array() {
        let s = include_str!("../../../../tests/fixtures/transcripts/with_bash.jsonl");
        let (entries, _) = parse_str(s);
        let tool_result_user = entries
            .iter()
            .filter_map(|e| match e {
                RawEntry::User(u) => Some(u),
                _ => None,
            })
            .find(|u| u.message.content.is_array())
            .expect("at least one user with array content");
        let arr = tool_result_user.message.content.as_array().unwrap();
        assert_eq!(arr[0]["type"], "tool_result");
        assert_eq!(arr[0]["tool_use_id"], "toolu_bash_01");
    }
}
