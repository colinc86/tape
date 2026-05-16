//! Claude Code runtime adapter — the first (and, in Step 1, only)
//! [`RuntimeAdapter`](super::RuntimeAdapter) implementation.
//!
//! This module owns every Claude-Code-specific surface:
//! - the settings overlay (`PreToolUse` / `PostToolUse` hook wiring) in
//!   [`overlay`];
//! - the JSONL transcript parser, discovery, and converter in
//!   [`transcript`].
//!
//! Future adapters (Cursor, Continue.dev, ...) will live as siblings under
//! `crate::runtime::`. The shared HTTP recording proxies in
//! [`crate::proxy`] are explicitly **not** Claude-Code-specific and stay
//! at their original module path.

use std::path::PathBuf;

use super::{
    OverlayHandle, RecorderContext, RuntimeAdapter, RuntimeCapabilities, RuntimeEnv, RuntimeError,
    SnapshotContext, TempDirState,
};

pub mod overlay;
pub mod transcript;

/// Stable id used in the registry, `--runtime` (Step 2), and the trailing
/// segment of `meta.recorder.agent` (Step 2).
pub const ID: &str = "claude-code";

/// Reference Claude Code adapter. Zero-sized; cheap to `Arc`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ClaudeCodeAdapter;

impl ClaudeCodeAdapter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RuntimeAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        ID
    }

    fn version(&self) -> &'static str {
        // Build-time stamp. Future versions may probe `claude --version`
        // at startup; the trait contract permits that.
        env!("CARGO_PKG_VERSION")
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::PROXY_ANTHROPIC
            | RuntimeCapabilities::PROXY_OPENAI
            | RuntimeCapabilities::HOOK_CAPTURE
            | RuntimeCapabilities::TRANSCRIPT
            | RuntimeCapabilities::MCP_WRAP
    }

    fn auto_detect(&self, cmd: &[String]) -> bool {
        // Conservative: only the literal `claude` binary auto-detects as
        // Claude Code. The Step 1 default-fallback path in `record()` is
        // separate and unconditional, so this is purely advisory until
        // Step 2 wires it in.
        cmd.first()
            .and_then(|s| std::path::Path::new(s).file_name())
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == "claude")
    }

    fn prepare_overlay(
        &self,
        env: &mut RuntimeEnv,
        ctx: &RecorderContext,
    ) -> Result<OverlayHandle, RuntimeError> {
        let inputs = overlay::OverlayInputs {
            tape_hook_bin: ctx.tape_hook_bin.clone(),
            tape_mcp_wrap_bin: ctx.tape_mcp_wrap_bin.clone(),
            recorder_socket: ctx.recorder_socket.clone(),
            mcp_servers: ctx.mcp_servers.clone(),
        };
        let (settings_path, mcp_path) = overlay::write_overlay_files(&ctx.overlay_dir, &inputs)
            .map_err(|e| RuntimeError::OverlayFailed(e.into()))?;

        // Forward the env vars `record()` previously set directly. The
        // adapter is the single source of truth for which env vars the
        // child needs to see; `record()` just copies these onto the child.
        env.vars.insert(
            "TAPE_OVERLAY_SETTINGS".to_string(),
            settings_path.display().to_string(),
        );
        env.vars.insert(
            "TAPE_OVERLAY_MCP_CONFIG".to_string(),
            mcp_path.display().to_string(),
        );
        env.overlay_paths
            .extend_from_slice(&[settings_path, mcp_path]);

        // Step 1 adapter owns no nested `TempDir`s — the outer recording
        // tempdir (owned by `record()`) already contains the overlay
        // files. We carry an empty `TempDirState` to demonstrate the
        // canonical pattern and make idempotent-cleanup testable.
        Ok(OverlayHandle {
            adapter_id: ID,
            state: Box::new(TempDirState::new(Vec::new())),
        })
    }

    fn discover_transcript(&self, ctx: &SnapshotContext) -> Result<PathBuf, RuntimeError> {
        let handle = transcript::find_active_session(&ctx.cwd).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => RuntimeError::TranscriptNotFound(ctx.cwd.clone()),
            _ => RuntimeError::Io(e),
        })?;
        Ok(handle.jsonl_path)
    }

    fn parse_transcript_entry(
        &self,
        raw: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, RuntimeError> {
        // Step 1 keeps the return shape free-form. The snapshot path in
        // `tape-mcp` still calls `transcript::parse_jsonl` + `to_tracks`
        // directly (the legacy public surface re-exported by `lib.rs`);
        // this trait method exists for the eventual Step 2+ migration but
        // is not yet on the critical path. We honor the contract by
        // returning the entry's `type` discriminator: `None` for unknown,
        // `Some` for known.
        let kind = raw.get("type").and_then(serde_json::Value::as_str);
        match kind {
            Some("user" | "assistant") => Ok(Some(raw.clone())),
            Some(_) | None => Ok(None),
        }
    }

    fn install_hooks(&self, _ctx: &RecorderContext) -> Result<(), RuntimeError> {
        // Claude Code's hooks are installed by the settings overlay (see
        // `overlay::settings_overlay`). There is nothing to do here.
        Ok(())
    }

    fn cleanup(&self, mut handle: OverlayHandle) -> Result<(), RuntimeError> {
        if handle.adapter_id != ID {
            return Err(RuntimeError::Unknown(format!(
                "claude-code adapter received an OverlayHandle for `{}`",
                handle.adapter_id
            )));
        }
        // Idempotent: take the `Vec<TempDir>` and drop it. A second call
        // finds `dirs == None` and returns `Ok(())`. The outer recording
        // tempdir is NOT owned by this handle — `record()` owns it.
        if let Some(state) = handle.state.downcast_mut::<TempDirState>() {
            if let Some(dirs) = state.dirs.take() {
                drop(dirs);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Capability;
    use std::sync::Arc;

    fn ctx_at(dir: &std::path::Path) -> RecorderContext {
        RecorderContext {
            tape_hook_bin: "/usr/local/bin/tape-hook".into(),
            tape_mcp_wrap_bin: "/usr/local/bin/tape-mcp-wrap".into(),
            recorder_socket: dir.join("recorder.sock"),
            mcp_servers: Vec::new(),
            overlay_dir: dir.to_path_buf(),
        }
    }

    #[test]
    fn id_and_capabilities() {
        let a = ClaudeCodeAdapter::new();
        assert_eq!(a.id(), "claude-code");
        let caps = a.capabilities();
        assert!(caps.contains(RuntimeCapabilities::PROXY_ANTHROPIC));
        assert!(caps.contains(RuntimeCapabilities::PROXY_OPENAI));
        assert!(caps.contains(RuntimeCapabilities::HOOK_CAPTURE));
        assert!(caps.contains(RuntimeCapabilities::TRANSCRIPT));
        assert!(caps.contains(RuntimeCapabilities::MCP_WRAP));
    }

    #[test]
    fn auto_detect_matches_claude_binary() {
        let a = ClaudeCodeAdapter::new();
        assert!(a.auto_detect(&["claude".to_string()]));
        assert!(a.auto_detect(&["/usr/local/bin/claude".to_string()]));
        assert!(!a.auto_detect(&["cursor".to_string()]));
        assert!(!a.auto_detect(&[]));
    }

    #[test]
    fn prepare_overlay_writes_files_and_sets_env() {
        let dir = tempfile::tempdir().unwrap();
        let a = ClaudeCodeAdapter::new();
        let mut env = RuntimeEnv::default();
        let handle = a.prepare_overlay(&mut env, &ctx_at(dir.path())).unwrap();
        assert_eq!(handle.adapter_id, "claude-code");
        assert!(env.vars.contains_key("TAPE_OVERLAY_SETTINGS"));
        assert!(env.vars.contains_key("TAPE_OVERLAY_MCP_CONFIG"));
        assert_eq!(env.overlay_paths.len(), 2);
        assert!(dir.path().join("settings.json").is_file());
        assert!(dir.path().join("mcp.json").is_file());
        a.cleanup(handle).unwrap();
    }

    #[test]
    fn cleanup_is_idempotent_via_double_call() {
        // The single `OverlayHandle` can't be passed to `cleanup` twice
        // because it's consumed by value (a deliberate type-level
        // protection against use-after-cleanup). The idempotency
        // contract we DO test: two distinct `prepare_overlay` ->
        // `cleanup` round-trips against the same adapter instance both
        // succeed, and the second one doesn't observe any residual
        // state from the first.
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        let a = ClaudeCodeAdapter::new();

        let mut env = RuntimeEnv::default();
        let h1 = a.prepare_overlay(&mut env, &ctx_at(dir1.path())).unwrap();
        a.cleanup(h1).expect("first cleanup ok");

        let mut env = RuntimeEnv::default();
        let h2 = a.prepare_overlay(&mut env, &ctx_at(dir2.path())).unwrap();
        a.cleanup(h2).expect("second cleanup ok");
    }

    #[test]
    fn cleanup_rejects_handle_from_other_adapter() {
        let a = ClaudeCodeAdapter::new();
        let bogus = OverlayHandle {
            adapter_id: "not-claude-code",
            state: Box::new(TempDirState::new(Vec::new())),
        };
        let err = a.cleanup(bogus).unwrap_err();
        match err {
            RuntimeError::Unknown(msg) => assert!(msg.contains("not-claude-code")),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn discover_transcript_uses_override_env() {
        // `transcript::find_active_session` honors `TAPE_TRANSCRIPT_OVERRIDE`
        // — exercise that path so we don't depend on a real
        // `~/.claude/projects/...` layout in unit tests.
        let dir = tempfile::tempdir().unwrap();
        let jsonl = dir.path().join("session.jsonl");
        std::fs::write(&jsonl, "").unwrap();
        std::env::set_var("TAPE_TRANSCRIPT_OVERRIDE", &jsonl);
        let a = ClaudeCodeAdapter::new();
        let got = a
            .discover_transcript(&SnapshotContext {
                cwd: dir.path().to_path_buf(),
            })
            .unwrap();
        std::env::remove_var("TAPE_TRANSCRIPT_OVERRIDE");
        assert_eq!(got, jsonl);
    }

    #[test]
    fn parse_transcript_entry_skips_unknown_kinds() {
        let a = ClaudeCodeAdapter::new();
        let user = serde_json::json!({"type": "user", "message": {"content": "hi"}});
        let unknown = serde_json::json!({"type": "future-thing"});
        let missing = serde_json::json!({"no_type": true});
        assert!(matches!(a.parse_transcript_entry(&user), Ok(Some(_))));
        assert!(matches!(a.parse_transcript_entry(&unknown), Ok(None)));
        assert!(matches!(a.parse_transcript_entry(&missing), Ok(None)));
    }

    #[test]
    fn capability_missing_round_trip() {
        // The Capability enum exists primarily for `RuntimeError`. Smoke-
        // test that a `CapabilityMissing(Transcript)` Displays sensibly,
        // since downstream `tape doctor` will surface it verbatim.
        let err = RuntimeError::CapabilityMissing(Capability::Transcript);
        let s = err.to_string();
        assert!(s.contains("Transcript"), "got: {s}");
    }

    #[test]
    fn registered_via_builtin_function() {
        // Sanity: `claude_code_adapter()` gives us an Arc whose `id()` is
        // `claude-code`, suitable for the `RecordOptions::runtime` field.
        let a: Arc<dyn RuntimeAdapter> = super::super::claude_code_adapter();
        assert_eq!(a.id(), ID);
    }
}
