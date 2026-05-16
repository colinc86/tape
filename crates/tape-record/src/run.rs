//! Top-level `tape record` orchestration: spawn proxy(ies), spawn child,
//! await exit, run eject.
//!
//! Runtime-specific surfaces (settings overlay, transcript ingestion, hook
//! installation, MCP-wrap injection) go through the
//! [`runtime::RuntimeAdapter`](crate::runtime::RuntimeAdapter) trait. Step 1
//! of #106 has exactly one bundled adapter (`claude-code`); the trait dispatch
//! looks like overkill today, on purpose — it's the surface the Cursor /
//! Continue / Codex adapters plug into in Steps 2-5.

use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;

use tape_format::meta::Outcome;
use tokio::process::Command;
use tracing::{info, warn};

use crate::eject::{eject, EjectOptions, EjectResult};
use crate::proxy::common::{spawn as spawn_proxy, ProxyConfig};
use crate::runtime::{
    claude_code_adapter, McpServerSpec, RecorderContext, RuntimeAdapter, RuntimeEnv,
};
use crate::session::Session;
use crate::socket;

#[derive(Clone)]
pub struct RecordOptions {
    pub task: String,
    pub recorder_agent: String,
    pub out_path: PathBuf,
    pub upstream_anthropic: String,
    pub upstream_openai: String,
    pub label: Option<String>,
    pub command: Vec<String>,
    pub env: Vec<(String, String)>,
    /// MCP servers to wrap. Empty = no MCP wrapping.
    pub mcp_servers: Vec<McpServerSpec>,
    /// Path to the `tape-hook` binary to use in the settings overlay.
    /// Defaults to looking up next to the current exe.
    pub tape_hook_bin: Option<PathBuf>,
    /// Path to the `tape-mcp-wrap` binary.
    /// Defaults to looking up next to the current exe.
    pub tape_mcp_wrap_bin: Option<PathBuf>,
    /// Runtime adapter used to wire this recording. Defaults to the
    /// Claude Code adapter (`claude-code`), which preserves v0.1
    /// behavior bit-for-bit; Step 2+ adds the `--runtime` CLI flag and
    /// auto-detection so this field can be set without the caller naming
    /// the adapter directly.
    pub runtime: Arc<dyn RuntimeAdapter>,
}

impl std::fmt::Debug for RecordOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordOptions")
            .field("task", &self.task)
            .field("recorder_agent", &self.recorder_agent)
            .field("out_path", &self.out_path)
            .field("upstream_anthropic", &self.upstream_anthropic)
            .field("upstream_openai", &self.upstream_openai)
            .field("label", &self.label)
            .field("command", &self.command)
            .field("env", &self.env)
            .field("mcp_servers", &self.mcp_servers)
            .field("tape_hook_bin", &self.tape_hook_bin)
            .field("tape_mcp_wrap_bin", &self.tape_mcp_wrap_bin)
            .field("runtime", &self.runtime.id())
            .finish()
    }
}

impl RecordOptions {
    /// Convenience constructor matching the v0.1 call-site that fills in
    /// the `runtime` field with the Claude Code adapter. Existing callers
    /// that build the struct via literal initialization can keep doing so
    /// — this is just the minimum-typing path for new callers and tests.
    #[must_use]
    pub fn new(task: String, recorder_agent: String, command: Vec<String>) -> Self {
        Self {
            task,
            recorder_agent,
            out_path: PathBuf::from("session.tape"),
            upstream_anthropic: "https://api.anthropic.com".to_string(),
            upstream_openai: "https://api.openai.com".to_string(),
            label: None,
            command,
            env: Vec::new(),
            mcp_servers: Vec::new(),
            tape_hook_bin: None,
            tape_mcp_wrap_bin: None,
            runtime: claude_code_adapter(),
        }
    }
}

#[derive(Debug)]
pub struct RecordResult {
    pub child_status: ExitStatus,
    pub eject: EjectResult,
}

/// Run a recording. Returns once the child exits and the tape has been
/// written to disk. All temp resources are cleaned up before return.
pub async fn record(opts: RecordOptions) -> anyhow::Result<RecordResult> {
    if opts.command.is_empty() {
        anyhow::bail!("record: no command supplied");
    }

    let session = Session::start(opts.task.clone(), opts.recorder_agent.clone());

    // Per-run temp dir. Dropping `temp_dir` removes the dir and everything
    // in it — overlay files, socket file (if it survives socket shutdown),
    // etc. This is the cleanup invariant the brief is strict about; the
    // RuntimeAdapter's `cleanup` runs BEFORE this dir is dropped, so the
    // two cleanup paths are sequential and non-racing.
    let temp_dir = tempfile::Builder::new().prefix("tape-").tempdir()?;
    let recorder_socket_path = temp_dir.path().join("recorder.sock");
    // Per-recording dir for the PreToolUse hook to buffer before-hashes
    // (#9 PR 2). Lives inside `temp_dir` so it's cleaned up automatically
    // when the TempDir is dropped.
    let before_dir = temp_dir.path().join("before-hashes");
    std::fs::create_dir_all(&before_dir)?;

    let socket_handle = socket::spawn(recorder_socket_path.clone(), session.clone()).await?;

    // Resolve sibling binaries for the overlay.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let tape_hook_bin = opts
        .tape_hook_bin
        .clone()
        .unwrap_or_else(|| exe_dir.join("tape-hook"));
    let tape_mcp_wrap_bin = opts
        .tape_mcp_wrap_bin
        .clone()
        .unwrap_or_else(|| exe_dir.join("tape-mcp-wrap"));

    // Prepare the overlay via the runtime adapter. The adapter is the
    // single source of truth for which env vars the child sees and which
    // overlay files exist on disk.
    let ctx = RecorderContext {
        tape_hook_bin,
        tape_mcp_wrap_bin,
        recorder_socket: recorder_socket_path.clone(),
        mcp_servers: opts.mcp_servers.clone(),
        overlay_dir: temp_dir.path().to_path_buf(),
    };
    let mut runtime_env = RuntimeEnv::default();
    let overlay_handle = opts
        .runtime
        .prepare_overlay(&mut runtime_env, &ctx)
        .map_err(|e| anyhow::anyhow!("runtime overlay failed: {e}"))?;
    opts.runtime
        .install_hooks(&ctx)
        .map_err(|e| anyhow::anyhow!("runtime install_hooks failed: {e}"))?;

    // Anthropic proxy.
    let mut anthropic_cfg = ProxyConfig::anthropic();
    anthropic_cfg.upstream = opts.upstream_anthropic.clone();
    let anthropic_proxy = spawn_proxy(anthropic_cfg, session.clone()).await?;
    let anthropic_url = anthropic_proxy.base_url();

    // OpenAI proxy.
    let mut openai_cfg = ProxyConfig::openai();
    openai_cfg.upstream = opts.upstream_openai.clone();
    let openai_proxy = spawn_proxy(openai_cfg, session.clone()).await?;
    let openai_url = openai_proxy.base_url();

    info!(
        runtime = %opts.runtime.id(),
        %anthropic_url,
        anthropic_upstream = %opts.upstream_anthropic,
        %openai_url,
        openai_upstream = %opts.upstream_openai,
        socket = %recorder_socket_path.display(),
        overlay_paths = ?runtime_env.overlay_paths,
        "tape recording: proxies + recorder socket + overlay ready"
    );

    // Spawn the child.
    let mut cmd = Command::new(&opts.command[0]);
    cmd.args(&opts.command[1..]);
    cmd.env("ANTHROPIC_BASE_URL", &anthropic_url);
    cmd.env("OPENAI_BASE_URL", &openai_url);
    cmd.env("TAPE_RECORDER_SOCKET", &recorder_socket_path);
    cmd.env("TAPE_BEFORE_DIR", &before_dir);
    for (k, v) in &runtime_env.vars {
        cmd.env(k, v);
    }
    for (k, v) in &opts.env {
        cmd.env(k, v);
    }
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let child_status = match cmd.status().await {
        Ok(s) => s,
        Err(e) => {
            warn!(?e, "failed to spawn child");
            anthropic_proxy.shutdown().await;
            openai_proxy.shutdown().await;
            socket_handle.shutdown().await;
            // Best-effort adapter cleanup before propagating. The
            // adapter contract guarantees `cleanup` is idempotent and
            // does not panic; we swallow its return so the original IO
            // error surfaces to the caller.
            if let Err(ce) = opts.runtime.cleanup(overlay_handle) {
                warn!(error = %ce, "runtime cleanup after spawn failure");
            }
            return Err(e.into());
        }
    };

    // Tear down the recording-time resources before eject.
    anthropic_proxy.shutdown().await;
    openai_proxy.shutdown().await;
    socket_handle.shutdown().await;
    if let Err(e) = opts.runtime.cleanup(overlay_handle) {
        warn!(error = %e, "runtime cleanup");
    }

    let outcome = if child_status.success() {
        Outcome::Success
    } else {
        Outcome::Failure
    };

    // Issue #17: load `.taperc` (workspace ancestor walk → $HOME) so custom
    // rules, enable_optional, and disable_default actually take effect.
    // Bad config aborts the eject — better to fail loudly than silently
    // drop a user's intended redactions.
    let cwd = std::env::current_dir()?;
    let redact_engine = tape_redact::engine_with_taperc(&cwd)?;
    let eject_result = eject(
        &session,
        &EjectOptions {
            task: opts.task,
            recorder_agent: opts.recorder_agent,
            outcome,
            stub_liner_notes: true,
            out_path: opts.out_path,
            redact_engine: Some(redact_engine),
            // Live recording — no source tape to inherit artifacts from.
            inherited_artifacts: std::collections::BTreeMap::new(),
            // Issue #72: surface the caller's --label in meta.yaml.
            label: opts.label,
        },
    )?;

    drop(temp_dir); // explicit; ensures cleanup even if Drop reorders.
    Ok(RecordResult {
        child_status,
        eject: eject_result,
    })
}

use std::path::Path;
