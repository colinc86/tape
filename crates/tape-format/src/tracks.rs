//! `tracks.jsonl` schema. See SPEC.md §5.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub step: u64,
    pub kind: Kind,
    pub ts: String,
    pub payload: Value,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_step: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub refs: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Task,
    ModelCall,
    McpCall,
    Shell,
    FileRead,
    FileWrite,
    Annotation,
    Eject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Annotation {
    pub by: String,
    pub note: String,
}

impl Track {
    /// Parse one JSONL line into a Track.
    pub fn from_line(line: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(line)?)
    }

    /// Serialize this track as a single JSONL line (without trailing newline).
    pub fn to_line(&self) -> crate::Result<String> {
        Ok(serde_json::to_string(self)?)
    }
}

/// Parse the full `tracks.jsonl` file into a vector of tracks.
///
/// Per SPEC §5.1: each line is a complete JSON object terminated by exactly
/// one `\n`. Empty and whitespace-only lines MUST NOT appear, except for
/// the (single) terminating newline at end-of-file.
pub fn parse_jsonl(content: &str) -> crate::Result<Vec<Track>> {
    let mut tracks = Vec::new();
    let segments: Vec<&str> = content.split('\n').collect();
    let last_idx = segments.len().saturating_sub(1);
    for (i, line) in segments.iter().enumerate() {
        let is_terminator = i == last_idx && line.is_empty();
        if is_terminator {
            // The trailing newline produces one empty final entry; allowed.
            continue;
        }
        if line.is_empty() || line.bytes().all(|b| b == b' ' || b == b'\t' || b == b'\r') {
            return Err(crate::Error::Invalid(format!(
                "line {}: empty or whitespace-only line not permitted (spec §5.1)",
                i + 1
            )));
        }
        let t = Track::from_line(line)
            .map_err(|e| crate::Error::Invalid(format!("line {}: {e}", i + 1)))?;
        tracks.push(t);
    }
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_empty_line_in_middle() {
        let content = "{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-06T10:00:00Z\",\"payload\":{\"prompt\":\"x\"}}\n\n{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-06T10:00:01Z\",\"payload\":{\"outcome\":\"success\"}}\n";
        let err = parse_jsonl(content).unwrap_err();
        assert!(err.to_string().contains("empty"), "got: {err}");
    }

    #[test]
    fn parse_rejects_whitespace_only_line() {
        let content = "{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-06T10:00:00Z\",\"payload\":{\"prompt\":\"x\"}}\n   \n{\"step\":2,\"kind\":\"eject\",\"ts\":\"2026-05-06T10:00:01Z\",\"payload\":{\"outcome\":\"success\"}}\n";
        let err = parse_jsonl(content).unwrap_err();
        assert!(err.to_string().contains("whitespace"), "got: {err}");
    }

    #[test]
    fn parse_accepts_trailing_newline() {
        let content = "{\"step\":1,\"kind\":\"task\",\"ts\":\"2026-05-06T10:00:00Z\",\"payload\":{\"prompt\":\"x\"}}\n";
        let tracks = parse_jsonl(content).unwrap();
        assert_eq!(tracks.len(), 1);
    }
}
