//! Anthropic API recording proxy. Thin re-export over `proxy::common` —
//! the only Anthropic-specific bit is the default config.

pub use crate::proxy::common::{spawn, ProxyConfig, ProxyHandle};

/// Construct an Anthropic-defaulted proxy config (`/v1/messages`,
/// `https://api.anthropic.com`, vendor `anthropic`).
pub fn config() -> ProxyConfig {
    ProxyConfig::anthropic()
}
