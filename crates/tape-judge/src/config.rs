//! `.taperc::judge:` config block.
//!
//! Mirrors `tape_redact::config`'s deny-unknown-fields posture (#36):
//! typos like `max_token` instead of `max_tokens` fail the loader
//! rather than silently using a default. The block is OPTIONAL —
//! cassettes that never call the judge don't need to declare anything.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JudgeConfig {
    /// Model identifier passed verbatim in the API request body. Not
    /// validated against any list — the API will reject unknown models
    /// with a 4xx and that's the right surface for "wrong model name".
    pub model: String,

    /// HTTP endpoint receiving the OpenAI-shaped `POST /v1/chat/completions`
    /// body. Default: the upstream `OpenAI` endpoint.
    #[serde(default = "default_endpoint")]
    pub endpoint: String,

    /// Name of the env var holding the API key. We never hold the key
    /// itself in `.taperc` (it'd be checked into the user's dotfiles
    /// or worse, the cassette). Default: `OPENAI_API_KEY`.
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,

    /// Total request budget per call, in milliseconds. Wraps every
    /// retry attempt's network call. Default: 30s.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Max tokens to ask the model to generate. Default: 1024.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Char-count ceiling for the prompt; values above this get
    /// truncated head-first with a warning attached to the audit
    /// record. Token-aware truncation is out of scope for v0.1.3.
    /// Default: 32000 chars (≈8k `OpenAI` tokens at 4 chars/token).
    #[serde(default = "default_max_input_chars")]
    pub max_input_chars: usize,

    /// Maximum retry attempts before bailing. 1 disables retries.
    /// Default: 4 attempts (so an initial 5xx + three retries).
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
}

fn default_endpoint() -> String {
    "https://api.openai.com/v1/chat/completions".into()
}
fn default_api_key_env() -> String {
    "OPENAI_API_KEY".into()
}
fn default_timeout_ms() -> u64 {
    30_000
}
fn default_max_tokens() -> u32 {
    1024
}
fn default_max_input_chars() -> usize {
    32_000
}
fn default_max_attempts() -> u32 {
    4
}

impl JudgeConfig {
    /// Load the `judge:` block from a serialized `.taperc` document.
    /// Returns `Ok(None)` when the `judge:` key is absent — the
    /// consumer decides whether that's fatal (e.g. `--judge` always
    /// requires it) or fine (e.g. `tape recap --auto` is opt-in).
    pub fn from_taperc_yaml(yaml: &str) -> anyhow::Result<Option<Self>> {
        let value: serde_yaml::Value = serde_yaml::from_str(yaml)?;
        let Some(judge) = value.get("judge") else {
            return Ok(None);
        };
        let cfg: JudgeConfig = serde_yaml::from_value(judge.clone())?;
        Ok(Some(cfg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        // Minimal valid block — model is the only required field.
        let yaml = "judge:\n  model: gpt-4o\n";
        let cfg = JudgeConfig::from_taperc_yaml(yaml).unwrap().unwrap();
        assert_eq!(cfg.model, "gpt-4o");
        assert_eq!(cfg.endpoint, "https://api.openai.com/v1/chat/completions");
        assert_eq!(cfg.api_key_env, "OPENAI_API_KEY");
        assert_eq!(cfg.timeout_ms, 30_000);
        assert_eq!(cfg.max_tokens, 1024);
        assert_eq!(cfg.max_input_chars, 32_000);
        assert_eq!(cfg.max_attempts, 4);
    }

    #[test]
    fn unknown_field_fails_loader() {
        let yaml = "judge:\n  model: gpt-4o\n  max_token: 100\n";
        let err = JudgeConfig::from_taperc_yaml(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("max_token"),
            "expected the unknown field in the error: {msg}"
        );
    }

    #[test]
    fn missing_judge_block_returns_none() {
        let yaml = "redact:\n  enable_optional: []\n";
        assert!(JudgeConfig::from_taperc_yaml(yaml).unwrap().is_none());
    }

    #[test]
    fn missing_required_field_fails() {
        let yaml = "judge:\n  endpoint: https://api.openai.com/v1\n";
        // `model` is REQUIRED.
        assert!(JudgeConfig::from_taperc_yaml(yaml).is_err());
    }

    #[test]
    fn full_block_round_trips() {
        let yaml = r"judge:
  model: gpt-4o-mini
  endpoint: https://example.com/v1/chat/completions
  api_key_env: MY_KEY
  timeout_ms: 10000
  max_tokens: 512
  max_input_chars: 16000
  max_attempts: 2
";
        let cfg = JudgeConfig::from_taperc_yaml(yaml).unwrap().unwrap();
        assert_eq!(cfg.endpoint, "https://example.com/v1/chat/completions");
        assert_eq!(cfg.api_key_env, "MY_KEY");
        assert_eq!(cfg.timeout_ms, 10_000);
        assert_eq!(cfg.max_attempts, 2);
    }
}
