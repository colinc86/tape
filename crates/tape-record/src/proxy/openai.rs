//! `OpenAI` API recording proxy. Thin re-export over `proxy::common` —
//! the only `OpenAI`-specific bit is the default config (path
//! `/v1/chat/completions`, vendor `openai`).

pub use crate::proxy::common::{spawn, ProxyConfig, ProxyHandle};

/// Construct an OpenAI-defaulted proxy config.
pub fn config() -> ProxyConfig {
    ProxyConfig::openai()
}
