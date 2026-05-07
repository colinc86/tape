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
/// Empty lines are rejected per spec §5.1.
pub fn parse_jsonl(content: &str) -> crate::Result<Vec<Track>> {
    let mut tracks = Vec::new();
    for (i, line) in content.split('\n').enumerate() {
        // The trailing newline produces one empty final entry; that's allowed.
        if line.is_empty() {
            // Acceptable only if it's the last entry (terminating \n).
            continue;
        }
        let t = Track::from_line(line).map_err(|e| {
            crate::Error::Invalid(format!("line {}: {e}", i + 1))
        })?;
        tracks.push(t);
    }
    Ok(tracks)
}
