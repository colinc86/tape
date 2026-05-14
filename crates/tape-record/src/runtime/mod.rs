//! Runtime adapter framework — issue #106, Step 1.
//!
//! A [`RuntimeAdapter`] is the contract that lets `tape record` and
//! `tape.snapshot` capture sessions from any MCP-supporting agent runtime
//! (Claude Code today; Cursor, Continue.dev, Codex, ... in future PRs).
//! Adapters are the only place where runtime-specific paths, settings
//! overlays, hook payload contracts, and transcript formats live —
//! everything downstream consumes vendor-neutral `tape/v0` events.
//!
//! This Step-1 PR is a **pure refactor**: the existing Claude Code recording
//! pipeline is moved behind the trait as the first (and only) `impl`.
//! No user-visible behavior changes; no CLI flag yet (Step 2); no second
//! adapter yet (Step 3). The trait surface, registry, error types, and
//! capability bitflags land here so subsequent steps have somewhere to plug.

use std::any::Any;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bitflags::bitflags;
use tempfile::TempDir;

pub mod claude_code;

/// Vendor-neutral MCP server specification. Adapters consume these from
/// [`RecorderContext::mcp_servers`] and decide how (or whether) to wire
/// each server through `tape-mcp-wrap`.
///
/// The fields are intentionally minimal — name, command, args, env. Anything
/// more specialised (sandbox flags, working directory, etc.) is currently
/// handled outside the adapter framework.
#[derive(Debug, Clone)]
pub struct McpServerSpec {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

bitflags! {
    /// Capability bits an adapter advertises via [`RuntimeAdapter::capabilities`].
    ///
    /// The CLI (Step 2+) consults these to either fulfill or reject flags
    /// against an adapter that doesn't implement the requested behaviour.
    /// Per the §3.7 community contract, capabilities omitted MUST return
    /// [`RuntimeError::CapabilityMissing`] when invoked through the
    /// corresponding trait method.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct RuntimeCapabilities: u32 {
        /// Anthropic HTTP proxy capture.
        const PROXY_ANTHROPIC = 0b0000_0001;
        /// OpenAI HTTP proxy capture.
        const PROXY_OPENAI    = 0b0000_0010;
        /// `PostToolUse`-style hook capture (synchronous tool observation).
        const HOOK_CAPTURE    = 0b0000_0100;
        /// On-disk session transcript ingestion (snapshot path).
        const TRANSCRIPT      = 0b0000_1000;
        /// MCP server wrap (`tape-mcp-wrap` injection).
        const MCP_WRAP        = 0b0001_0000;
    }
}

/// Named capability for error reporting. Mirrors the [`RuntimeCapabilities`]
/// bits, but carries no flag arithmetic; used as the payload of
/// [`RuntimeError::CapabilityMissing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    ProxyAnthropic,
    ProxyOpenAi,
    HookCapture,
    Transcript,
    McpWrap,
}

/// Errors an adapter can return. Maps 1:1 to the §3.9 diagnostic codes the
/// CLI surface (Step 2) will exit with.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// Catch-all for runtime-specific surface problems.
    #[error("runtime: {0}")]
    Unknown(String),
    /// The capability is not implemented by this adapter.
    #[error("runtime capability not implemented: {0:?}")]
    CapabilityMissing(Capability),
    /// `prepare_overlay` failed — adapter couldn't wire its overlay files,
    /// env vars, or hook plumbing.
    #[error("runtime overlay failed: {0}")]
    OverlayFailed(#[source] anyhow::Error),
    /// `discover_transcript` found no transcript file at the expected path.
    #[error("runtime transcript not found: {0}")]
    TranscriptNotFound(PathBuf),
    /// `parse_transcript_entry` could not decode a raw entry.
    #[error("runtime transcript parse: {0}")]
    Parse(#[source] anyhow::Error),
    /// Wrapped IO error.
    #[error("runtime io: {0}")]
    Io(#[from] std::io::Error),
}

/// Inputs to [`RuntimeAdapter::prepare_overlay`]. Carries the recording-time
/// plumbing every adapter needs to wire its overlay against.
///
/// Subsequent steps may grow this struct (e.g. with adapter-private
/// `serde_json::Value` config from `.taperc::runtime:`); for now it tracks
/// exactly the fields the existing Claude Code overlay consumes.
#[derive(Debug, Clone)]
pub struct RecorderContext {
    /// Path to the `tape-hook` binary the overlay wires into the runtime's
    /// hook lifecycle (Claude Code's `PreToolUse` / `PostToolUse`).
    pub tape_hook_bin: PathBuf,
    /// Path to the `tape-mcp-wrap` binary the overlay routes MCP server
    /// invocations through.
    pub tape_mcp_wrap_bin: PathBuf,
    /// Unix-domain socket the recorder is listening on. Hook and MCP-wrap
    /// processes post `model_call` / `mcp_call` / `shell` events here.
    pub recorder_socket: PathBuf,
    /// MCP servers to wrap. Empty = the overlay still wires the runtime's
    /// built-in hooks but does not rewrite any MCP server definitions.
    pub mcp_servers: Vec<McpServerSpec>,
    /// Temp dir scoped to this recording. Overlay files (settings,
    /// `mcp.json`, etc.) should be written here. The adapter does NOT own
    /// the lifetime of this dir — `record()` does; the adapter's
    /// [`OverlayHandle`] may *contain* a child `TempDir` for adapter-private
    /// state, but the dir referenced here outlives all of them.
    pub overlay_dir: PathBuf,
}

/// Inputs to [`RuntimeAdapter::discover_transcript`]. Carries the working
/// directory plus any test-injected overrides.
#[derive(Debug, Clone)]
pub struct SnapshotContext {
    /// Working directory the runtime was launched from. Adapters with a
    /// per-project transcript layout (Claude Code) resolve their on-disk
    /// path relative to this.
    pub cwd: PathBuf,
}

/// State the overlay mutates: env vars to set on the child process, extra
/// arguments to inject before the user's command, and (informationally)
/// the overlay file paths the adapter wrote.
///
/// `record()` consumes this after `prepare_overlay` returns so it can
/// forward the env vars to the spawned child.
#[derive(Debug, Default, Clone)]
pub struct RuntimeEnv {
    /// Env vars to set on the child process.
    pub vars: BTreeMap<String, String>,
    /// Extra positional args injected before the user's command (currently
    /// unused; reserved for adapters that need a `--settings-file <path>`
    /// style flag — Cursor likely will).
    pub prepend_args: Vec<String>,
    /// Informational: overlay files the adapter wrote (recorded in the
    /// log line `record()` emits for visibility).
    pub overlay_paths: Vec<PathBuf>,
}

/// Handle returned by [`RuntimeAdapter::prepare_overlay`]. Owns whatever
/// adapter-private state needs to live for the recording's duration
/// (typically temp files, original-config backups, MCP wrap port numbers).
///
/// Cleanup is double-routed: the adapter's [`RuntimeAdapter::cleanup`]
/// MUST be idempotent, and the handle's `state` may use Drop as a fallback.
/// `record()` always calls `cleanup` explicitly before the handle is
/// dropped, but the Drop fallback exists so a panic between
/// `prepare_overlay` and `cleanup` doesn't leak temp resources.
pub struct OverlayHandle {
    /// Stable adapter id (the `id()` of the adapter that produced this
    /// handle). Lets `cleanup` defensive-check the caller passed the right
    /// handle back to the right adapter.
    pub adapter_id: &'static str,
    /// Adapter-private state. Boxed `Any` so adapters can stash arbitrary
    /// values (temp dirs, port maps, backup file paths) without polluting
    /// the trait. The framework never inspects this.
    pub state: Box<dyn Any + Send + Sync>,
}

impl std::fmt::Debug for OverlayHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayHandle")
            .field("adapter_id", &self.adapter_id)
            .field("state", &"<opaque>")
            .finish()
    }
}

/// The contract every runtime adapter implements. See module docs for the
/// big picture; see [`claude_code::ClaudeCodeAdapter`] for the reference
/// implementation.
///
/// Adapters are `Send + Sync` so a single `Arc<dyn RuntimeAdapter>` can be
/// shared across the recording's tasks (proxy threads, socket handler,
/// snapshot path).
pub trait RuntimeAdapter: Send + Sync {
    /// Stable, lowercase, hyphenated id. Used as a registry key, as the
    /// `--runtime <id>` flag value (Step 2), and as the trailing
    /// segment of `meta.recorder.agent` (Step 2 — Step 1 keeps the legacy
    /// value to preserve bit-exact cassette output).
    fn id(&self) -> &'static str;

    /// Best-effort runtime version stamp. Step 1 returns a build-time
    /// constant; future adapters MAY probe the runtime binary
    /// (`cursor --version`) at registration time. Never panics; an unknown
    /// version returns `"unknown"`.
    fn version(&self) -> &'static str;

    /// Capabilities this adapter implements. The CLI consults this to
    /// either fulfill or reject capability-gated flags. Capabilities
    /// omitted here MUST return [`RuntimeError::CapabilityMissing`] when
    /// the corresponding trait method is invoked.
    fn capabilities(&self) -> RuntimeCapabilities;

    /// True iff `cmd` is plausibly an invocation of this runtime. Used
    /// by Step 2's auto-detection logic; safe to return `false` always
    /// from Step 1.
    fn auto_detect(&self, cmd: &[String]) -> bool;

    /// Prepare the recording overlay: write any temp config files inside
    /// `ctx.overlay_dir`, populate `env` with the env vars to forward to
    /// the spawned child, and return an [`OverlayHandle`] owning any
    /// adapter-private state. Idempotent if re-called against a fresh
    /// `overlay_dir`.
    fn prepare_overlay(
        &self,
        env: &mut RuntimeEnv,
        ctx: &RecorderContext,
    ) -> Result<OverlayHandle, RuntimeError>;

    /// Locate the runtime's session transcript file, if it has one.
    /// Adapters whose `capabilities()` does not include
    /// [`RuntimeCapabilities::TRANSCRIPT`] MUST return
    /// `Err(RuntimeError::CapabilityMissing(Capability::Transcript))`.
    fn discover_transcript(
        &self,
        ctx: &SnapshotContext,
    ) -> Result<PathBuf, RuntimeError>;

    /// Convert a raw transcript entry into a typed value. Step 1 keeps
    /// the Claude-Code-shaped return type intentionally — generalising
    /// the return shape is a Step-2+ concern (it'll be `TrackEvent`-y
    /// once a second transcript-bearing adapter exists to motivate the
    /// abstraction).
    ///
    /// Returns `Ok(None)` for entries the adapter chose to skip (unknown
    /// types, system events, etc.); the snapshot caller still increments
    /// its skip counter and proceeds. Returns `Err` only for malformed
    /// entries the adapter wanted to surface as parse errors.
    fn parse_transcript_entry(
        &self,
        raw: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, RuntimeError>;

    /// Install hook-based capture if the runtime supports it. Claude Code
    /// installs its hooks via the settings overlay (so this is a no-op
    /// for `claude-code`); future adapters with a separate hook
    /// installation step (Cursor phase-2, Continue.dev) put their wiring
    /// here. Adapters without [`RuntimeCapabilities::HOOK_CAPTURE`]
    /// return `Ok(())` (the no-op) rather than `Err(CapabilityMissing)`,
    /// because "no hook surface" and "hooks installed via overlay" both
    /// look the same to the caller.
    fn install_hooks(&self, _ctx: &RecorderContext) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Tear down the overlay. Idempotent — `record()` always calls this
    /// in the success path, and may call it from an error path too;
    /// adapters MUST tolerate a second call as a no-op.
    fn cleanup(&self, handle: OverlayHandle) -> Result<(), RuntimeError>;
}

/// Process-wide registry of runtime adapters.
///
/// Step 1 has exactly one entry (`claude-code`). Step 2 will add Cursor;
/// later steps add more. The registry is `Send + Sync`-able so the CLI
/// can build it once at startup and share an `Arc` across the recording
/// tasks.
#[derive(Default, Clone)]
pub struct Registry {
    adapters: BTreeMap<&'static str, Arc<dyn RuntimeAdapter>>,
}

impl Registry {
    /// Empty registry. Callers typically prefer
    /// [`register_builtin_adapters`] which produces a registry populated
    /// with every adapter bundled into this binary.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an adapter. If an adapter with the same `id()` was already
    /// registered, it is replaced; the previous adapter is returned.
    /// (Replacement is intentional — it lets tests substitute mocks for
    /// the built-in adapter without rebuilding the registry from scratch.)
    pub fn register(
        &mut self,
        adapter: Arc<dyn RuntimeAdapter>,
    ) -> Option<Arc<dyn RuntimeAdapter>> {
        self.adapters.insert(adapter.id(), adapter)
    }

    /// Look up an adapter by `id`. Returns `None` if no adapter with that
    /// id is registered.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Arc<dyn RuntimeAdapter>> {
        self.adapters.get(id).cloned()
    }

    /// Iterator over registered adapter ids, in stable lexical order
    /// (`BTreeMap` ordering). Used by `tape runtime list` (Step 2) and by
    /// the auto-detection probe.
    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.adapters.keys().copied()
    }

    /// Iterator over registered adapters, in stable lexical order by id.
    pub fn adapters(&self) -> impl Iterator<Item = &Arc<dyn RuntimeAdapter>> + '_ {
        self.adapters.values()
    }

    /// Number of registered adapters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// True iff no adapters are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("ids", &self.ids().collect::<Vec<_>>())
            .finish()
    }
}

/// Build a registry populated with every adapter bundled into this binary.
/// Step 1 ships exactly the `claude-code` adapter.
#[must_use]
pub fn register_builtin_adapters() -> Registry {
    let mut reg = Registry::new();
    reg.register(claude_code_adapter());
    reg
}

/// Convenience constructor for the built-in Claude Code adapter. Used by
/// `RecordOptions` callers that want the legacy v0.1 behavior without
/// going through the registry.
#[must_use]
pub fn claude_code_adapter() -> Arc<dyn RuntimeAdapter> {
    Arc::new(claude_code::ClaudeCodeAdapter::new())
}

// --------------------------------------------------------------------
// Internal helper: build an `OverlayHandle` whose `state` carries a
// `TempDir`. The `TempDir`'s Drop is the secondary cleanup path; the
// adapter's explicit `cleanup` is the primary path. Adapters that don't
// need a nested temp dir can construct `OverlayHandle` directly.
// --------------------------------------------------------------------

/// Adapter-private state wrapper that owns a (possibly empty) list of
/// nested `TempDir`s. Used by the Claude Code adapter today; usable by
/// any future adapter with similar needs.
pub(crate) struct TempDirState {
    /// `Option` so [`cleanup`] can `take` and drop the dirs explicitly.
    pub(crate) dirs: Option<Vec<TempDir>>,
}

impl TempDirState {
    pub(crate) fn new(dirs: Vec<TempDir>) -> Self {
        Self { dirs: Some(dirs) }
    }
}

/// Recover a `&Path` ergonomic accessor; not used today, but documented
/// so future adapter code knows the canonical accessor pattern.
#[allow(dead_code)]
pub(crate) fn overlay_dir_of(ctx: &RecorderContext) -> &Path {
    &ctx.overlay_dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_round_trip() {
        let reg = register_builtin_adapters();
        assert_eq!(reg.len(), 1, "step 1 registers exactly claude-code");
        assert!(!reg.is_empty());
        let cc = reg.get("claude-code").expect("claude-code is registered");
        assert_eq!(cc.id(), "claude-code");
        assert!(reg.get("nope").is_none(), "unknown id returns None");
        let ids: Vec<&'static str> = reg.ids().collect();
        assert_eq!(ids, vec!["claude-code"]);
    }

    #[test]
    fn registry_replace_returns_previous() {
        let mut reg = Registry::new();
        let first = claude_code_adapter();
        assert!(reg.register(first.clone()).is_none());
        let prev = reg.register(claude_code_adapter()).expect("replacement returns previous");
        assert_eq!(prev.id(), "claude-code");
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn capabilities_bitflags_are_disjoint() {
        let all = RuntimeCapabilities::PROXY_ANTHROPIC
            | RuntimeCapabilities::PROXY_OPENAI
            | RuntimeCapabilities::HOOK_CAPTURE
            | RuntimeCapabilities::TRANSCRIPT
            | RuntimeCapabilities::MCP_WRAP;
        // 5 bits set, all disjoint.
        assert_eq!(all.bits().count_ones(), 5);
    }
}
