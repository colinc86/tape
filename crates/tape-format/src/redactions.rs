//! `redactions.json` schema. See SPEC.md §6.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Redaction {
    pub step: u64,
    pub field_path: String,
    pub rule_id: String,
    pub replacement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_range: Option<[u64; 2]>,
}

pub fn parse(content: &str) -> crate::Result<Vec<Redaction>> {
    Ok(serde_json::from_str(content)?)
}

pub fn to_json(records: &[Redaction]) -> crate::Result<String> {
    Ok(serde_json::to_string_pretty(records)?)
}
