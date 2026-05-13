//! Top-level `tape record` orchestration: spawn proxy(ies), spawn child,
//! await exit, run eject.

use std::path::PathBuf;
use std::process::ExitStatus;

use tape_format::meta::Outcome;
use tokio::process::Command;
use tracing::{info, warn};

use crate::eject::{eject, EjectOptions, EjectResult};
use crate::overlay::{write_overlay_files, McpServerSpec, OverlayInputs};
use crate::proxy::common::{spawn as spawn_proxy, ProxyConfig};
use crate::session::Session;
use crate::socket;

#[derive(Debug, Clone)]
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
    // etc. This is the cleanup invariant the brief is strict about.
    let temp_dir = tempfile::Builder::new()
        .prefix("tape-")
        .tempdir()?;
    let recorder_socket_path = temp_dir.path().join("recorder.sock");
    // Per-recording dir for the PreToolUse hook to buffer before-hashes.
    // Lives inside `temp_dir` so it's cleaned up automatically on drop.
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

    // Write overlay files. Even with no mcp_servers, the settings overlay
    // wires Bash/Read/Write hooks — that's the v0-distinguishing feature.
    let overlay_inputs = OverlayInputs {
        tape_hook_bin,
        tape_mcp_wrap_bin,
        recorder_socket: recorder_socket_path.clone(),
        mcp_servers: opts.mcp_servers.clone(),
    };
    let (settings_path, mcp_path) =
        write_overlay_files(temp_dir.path(), &overlay_inputs)?;

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
        %anthropic_url,
        anthropic_upstream = %opts.upstream_anthropic,
        %openai_url,
        openai_upstream = %opts.upstream_openai,
        socket = %recorder_socket_path.display(),
        settings = %settings_path.display(),
        mcp = %mcp_path.display(),
        "tape recording: proxies + recorder socket + overlay ready"
    );

    // Spawn the child.
    let mut cmd = Command::new(&opts.command[0]);
    cmd.args(&opts.command[1..]);
    cmd.env("ANTHROPIC_BASE_URL", &anthropic_url);
    cmd.env("OPENAI_BASE_URL", &openai_url);
    cmd.env("TAPE_RECORDER_SOCKET", &recorder_socket_path);
    cmd.env("TAPE_OVERLAY_SETTINGS", &settings_path);
    cmd.env("TAPE_OVERLAY_MCP_CONFIG", &mcp_path);
    cmd.env("TAPE_BEFORE_DIR", &before_dir);
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
            return Err(e.into());
        }
    };

    // Tear down the recording-time resources before eject.
    anthropic_proxy.shutdown().await;
    openai_proxy.shutdown().await;
    socket_handle.shutdown().await;

    let outcome = if child_status.success() {
        Outcome::Success
    } else {
        Outcome::Failure
    };

    let eject_result = eject(
        &session,
        &EjectOptions {
            task: opts.task,
            recorder_agent: opts.recorder_agent,
            outcome,
            stub_liner_notes: true,
            out_path: opts.out_path,
            redact_engine: Some(tape_redact::Engine::with_default_rules()),
        },
    )?;

    drop(temp_dir); // explicit; ensures cleanup even if Drop reorders.
    Ok(RecordResult {
        child_status,
        eject: eject_result,
    })
}

use std::path::Path;
