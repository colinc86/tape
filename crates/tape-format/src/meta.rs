//! `meta.yaml` schema. See SPEC.md §3.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Meta {
    pub tape_version: String,
    pub id: String,
    pub created_at: String,
    pub ejected_at: String,
    pub task: String,
    pub recorder: Recorder,
    pub outcome: Outcome,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub models: Vec<ModelSummary>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tools: Vec<ToolSummary>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_budget: Option<ToolBudget>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_summary: Option<RedactionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Recorder {
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Success,
    Failure,
    Abandoned,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSummary {
    pub vendor: String,
    pub model: String,
    pub calls: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolSummary {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    pub calls: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolBudget {
    pub total_calls: u64,
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub wall_clock_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactionSummary {
    pub rules_applied: Vec<String>,
    pub redaction_count: u64,
}

impl Meta {
    /// Parse `meta.yaml` content.
    pub fn parse(yaml: &str) -> crate::Result<Self> {
        Ok(serde_yaml::from_str(yaml)?)
    }

    /// Serialize to YAML.
    pub fn to_yaml(&self) -> crate::Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }
}
