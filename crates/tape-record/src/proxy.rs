//! HTTP recording proxies. Each vendor gets its own submodule but they share
//! a streaming-tee primitive (see `stream.rs`).

pub mod anthropic;
pub mod common;
pub mod openai;
pub mod stream;
