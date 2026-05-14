//! Shared judge-model client + defense-in-depth scanner.
//!
//! Four consumers (per #145): `tape diff --judge`, `tape recap --auto`,
//! `tape relinernote`, `tape tag --auto`. Each used to need its own HTTP
//! client, retry handling, content scanner, and audit-record shape;
//! this crate factors that out.
//!
//! ## Surface
//!
//! - [`JudgeClient::complete`] is the one HTTP entry point — posts an
//!   OpenAI-shaped `{model, messages, max_tokens}` body, retries
//!   network failures / 429 / 5xx with exponential backoff, bails on
//!   4xx, applies CLI-side truncation if the prompt exceeds
//!   `max_input_chars`, and finally runs the response through
//!   [`defense_in_depth::scan`].
//! - [`defense_in_depth`] holds the prompt-injection scanner. The
//!   starter rule set is conservative and **flagged for security review**
//!   — see the module's doc-comment.
//! - [`JudgeCallRecord`] is the structured audit row consumers append
//!   to their feature-specific `meta.X[]` array.
//! - [`JudgeConfig`] mirrors the `tape-redact` config-loading shape: a
//!   `deny_unknown_fields` serde struct loaded from `.taperc::judge:`
//!   so typos fail at the boundary instead of silently disabling
//!   things. (Issue #36 precedent.)
//!
//! ## Out of scope for v0.1.3
//!
//! - Token-aware truncation (model-specific tokenizer crates).
//! - Streaming responses.
//! - Provider-specific adapters beyond the generic `OpenAI` shape. The
//!   PR body documents how to add one as a follow-up.
//! - Switching `tape diff --judge` over to use this client; that's a
//!   separate ticket gated on this one merging.

pub mod config;
pub mod defense_in_depth;
pub mod record;

use std::time::Duration;

use thiserror::Error;

pub use config::JudgeConfig;
pub use defense_in_depth::{scan as scan_for_injection, ScanHit};
pub use record::JudgeCallRecord;

/// Wraps every reason a judge call can fail. Consumers match on the
/// variants to decide whether to surface, retry at a higher level, or
/// fall back to a non-LLM path.
#[derive(Debug, Error)]
pub enum JudgeError {
    #[error("judge config: {0}")]
    Config(String),
    #[error("transport failure after {attempts} attempt(s): {source}")]
    Transport {
        attempts: u32,
        #[source]
        source: reqwest::Error,
    },
    #[error("upstream returned status {status} — body: {body}")]
    Upstream { status: u16, body: String },
    #[error("upstream response body did not parse as expected JSON: {0}")]
    InvalidResponse(String),
    #[error("defense-in-depth: {0:?}")]
    Rejected(ScanHit),
}

/// Per-call knobs that the caller can override beyond what's in
/// `.taperc::judge:`. Keep this small — most settings belong in
/// [`JudgeConfig`] so they apply consistently across consumers.
#[derive(Debug, Clone, Default)]
pub struct JudgeOpts {
    /// Optional override of `JudgeConfig::max_tokens` for one call.
    pub max_tokens: Option<u32>,
}

/// Successful judge output plus the audit row consumers persist.
#[derive(Debug, Clone)]
pub struct JudgeOutput {
    pub text: String,
    pub record: JudgeCallRecord,
}

/// HTTP client wrapping a `reqwest::Client` plus the config / scanner
/// the consumer locked in via `.taperc`. Cheap to clone — the inner
/// `reqwest::Client` is `Arc`-backed.
#[derive(Debug, Clone)]
pub struct JudgeClient {
    config: JudgeConfig,
    http: reqwest::Client,
    /// Rule set baked in at construction time so a config reload is
    /// the only way to change scanner behavior mid-run.
    scanner: defense_in_depth::Scanner,
}

impl JudgeClient {
    /// Build a client from a parsed config. Resolves the API key
    /// against the environment up front so misconfiguration surfaces
    /// before the first HTTP request goes out.
    pub fn new(config: JudgeConfig) -> Result<Self, JudgeError> {
        let key_env = &config.api_key_env;
        if std::env::var(key_env).is_err() {
            return Err(JudgeError::Config(format!(
                "env var {key_env:?} (named in .taperc::judge::api_key_env) is not set; \
                 set it before invoking any --judge / --auto feature"
            )));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| JudgeError::Config(format!("build reqwest client: {e}")))?;
        let scanner = defense_in_depth::Scanner::with_defaults();
        Ok(Self {
            config,
            http,
            scanner,
        })
    }

    /// Run one prompt through the model. The returned `JudgeOutput`
    /// carries the model text and the audit row the caller persists
    /// into the cassette they own.
    pub async fn complete(&self, prompt: &str, opts: JudgeOpts) -> Result<JudgeOutput, JudgeError> {
        // 1. CLI-side truncation. Token-aware truncation is out of
        //    scope (different tokenizer per model); char-counts are
        //    a defensible proxy for the initial cut.
        let (effective_prompt, truncated) = if prompt.chars().count() > self.config.max_input_chars
        {
            let truncated: String = prompt.chars().take(self.config.max_input_chars).collect();
            (truncated, true)
        } else {
            (prompt.to_owned(), false)
        };

        // 2. Hash both ends before the network call. The audit row
        //    pins what was *asked*, not what was *received*, so an
        //    upstream retry that returns a different output is still
        //    attributable to the same prompt.
        let prompt_hash = record::hash_blake3(&effective_prompt);

        // 3. Retry-with-backoff over transient failures. Idempotency
        //    is fine for the OpenAI-shaped "complete this prompt"
        //    request — there are no side effects upstream.
        let max_tokens = opts.max_tokens.unwrap_or(self.config.max_tokens);
        let body = serde_json::json!({
            "model": &self.config.model,
            "messages": [{"role": "user", "content": effective_prompt}],
            "max_tokens": max_tokens,
        });
        let api_key = std::env::var(&self.config.api_key_env).map_err(|_| {
            JudgeError::Config(format!(
                "env var {:?} disappeared between client init and call",
                self.config.api_key_env
            ))
        })?;

        let mut last_err: Option<reqwest::Error> = None;
        let mut attempt: u32 = 0;
        let text = loop {
            attempt += 1;
            let req = self
                .http
                .post(&self.config.endpoint)
                .bearer_auth(&api_key)
                .json(&body);
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let parsed: serde_json::Value = resp
                            .json()
                            .await
                            .map_err(|e| JudgeError::InvalidResponse(format!("body json: {e}")))?;
                        let text = parsed["choices"][0]["message"]["content"]
                            .as_str()
                            .ok_or_else(|| {
                                JudgeError::InvalidResponse(format!(
                                    "missing choices[0].message.content in {parsed}"
                                ))
                            })?
                            .to_owned();
                        break text;
                    }
                    let code = status.as_u16();
                    let body_text = resp.text().await.unwrap_or_default();
                    let is_retryable = code == 429 || (500..600).contains(&code);
                    if !is_retryable || attempt >= self.config.max_attempts {
                        return Err(JudgeError::Upstream {
                            status: code,
                            body: body_text,
                        });
                    }
                    // Retryable status — fall through to backoff sleep.
                }
                Err(e) => {
                    last_err = Some(e);
                    if attempt >= self.config.max_attempts {
                        // Drop into the post-loop branch which surfaces the error.
                        break String::new();
                    }
                }
            }
            // Exponential backoff with a 5s ceiling per gap so a
            // pathological 429 storm doesn't stall the whole process.
            let backoff_ms = (100u64 * (1u64 << (attempt - 1).min(6))).min(5_000);
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        };
        // Network-error fallthrough from the loop.
        if text.is_empty() {
            if let Some(e) = last_err {
                return Err(JudgeError::Transport {
                    attempts: attempt,
                    source: e,
                });
            }
        }

        // 4. Defense-in-depth on the response. A hit fails the call
        //    rather than redacting — the consumer can decide whether
        //    to fall back to a non-LLM path or surface to the user.
        let scan_result = match self.scanner.scan(&text) {
            Ok(()) => record::ScanOutcome::Clean,
            Err(hit) => {
                return Err(JudgeError::Rejected(hit));
            }
        };

        let output_hash = record::hash_blake3(&text);
        let record = JudgeCallRecord {
            ts: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
            model: self.config.model.clone(),
            prompt_hash,
            output_hash,
            scan_result,
            retry_count: attempt.saturating_sub(1),
            truncated,
        };
        Ok(JudgeOutput { text, record })
    }
}
