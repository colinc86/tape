//! Recording subsystem for `tape record`. See SPEC §8 and the
//! `tape-record-pipeline` skill.
//!
//! Public surface:
//! - [`session::Session`] — owns the in-flight recording; events are appended
//!   monotonically.
//! - [`proxy::anthropic`] — HTTP proxy that records `model_call` events while
//!   tee'ing streaming responses through to the child without buffering.
//! - [`eject::eject`] — finalizes a session into a `.tape` zip on disk.
//! - [`runtime`] — the runtime adapter framework (issue #106, Step 1).
//!   `tape record` reaches every runtime-specific surface (Claude Code today,
//!   Cursor / Continue / Codex in future PRs) via the [`runtime::RuntimeAdapter`]
//!   trait.

pub mod eject;
pub mod proxy;
pub mod run;
pub mod runtime;
pub mod session;
pub mod socket;

/// Backwards-compatibility re-export. The Claude Code settings overlay
/// lives at `crate::runtime::claude_code::overlay` after #106; the legacy
/// `crate::overlay` path is preserved so internal callers and tests keep
/// compiling without churn.
pub mod overlay {
    pub use crate::runtime::claude_code::overlay::{
        mcp_config, settings_overlay, write_overlay_files, McpServerSpec, OverlayInputs,
    };
}

/// Backwards-compatibility re-export. The Claude Code session-transcript
/// parser, discoverer, and converter live at
/// `crate::runtime::claude_code::transcript` after #106; the legacy
/// `crate::transcript` path is preserved so `tape-mcp` and other callers
/// keep compiling without churn (see the #106 principal hint, pitfall #2).
pub mod transcript {
    pub use crate::runtime::claude_code::transcript::{
        find_active_session, parse_jsonl, to_tracks, ConvertReport, ParseReport, RawEntry,
        TranscriptHandle,
    };
}
