//! Generates Claude Code settings overlays that wire `tape-hook` into the
//! PreToolUse / PostToolUse lifecycle for the built-in tools we care to
//! record. Overlay files live in the temp dir for the recording's lifetime
//! and are removed during cleanup.
//!
//! The overlay also generates a temp `mcp.json` that re-points each
//! configured MCP server through `tape-mcp-wrap` (step 6 plumbing).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub use crate::runtime::McpServerSpec;

/// Inputs to overlay generation. Internal to the Claude Code adapter;
/// callers in the wider crate consume the vendor-neutral
/// [`crate::runtime::RecorderContext`] and let the adapter translate.
#[derive(Debug, Clone)]
pub struct OverlayInputs {
    pub tape_hook_bin: PathBuf,
    pub tape_mcp_wrap_bin: PathBuf,
    pub recorder_socket: PathBuf,
    /// MCP servers to wrap. Map server name → (cmd, args, env).
    pub mcp_servers: Vec<McpServerSpec>,
}

/// Produces JSON for a Claude Code settings overlay (passed via the
/// `--settings <file>` flag, or merged via `CLAUDE_SETTINGS_PATH` if the
/// CLI version supports it).
pub fn settings_overlay(inputs: &OverlayInputs) -> Value {
    let bin = inputs.tape_hook_bin.display().to_string();
    let socket = inputs.recorder_socket.display().to_string();

    let cmd = move |kind: &str| {
        // Claude Code hooks receive JSON on stdin and run the configured
        // shell command. We export TAPE_RECORDER_SOCKET so tape-hook knows
        // where to post.
        json!({
            "type": "command",
            "command": format!(
                "TAPE_RECORDER_SOCKET={socket:?} TAPE_HOOK_KIND={kind} {bin:?}"
            )
        })
    };

    json!({
        "hooks": {
            "PreToolUse": [
                { "matcher": "Bash",            "hooks": [cmd("shell_pre")] },
                { "matcher": "Write|Edit|MultiEdit|NotebookEdit", "hooks": [cmd("file_write_pre")] }
            ],
            "PostToolUse": [
                { "matcher": "Bash",            "hooks": [cmd("shell")] },
                { "matcher": "Read",            "hooks": [cmd("file_read")] },
                { "matcher": "Write|Edit|MultiEdit|NotebookEdit", "hooks": [cmd("file_write")] }
            ]
        }
    })
}

/// Produces a temp `mcp.json` config that points every configured MCP
/// server through `tape-mcp-wrap`.
pub fn mcp_config(inputs: &OverlayInputs) -> Value {
    let mut servers = serde_json::Map::new();
    for spec in &inputs.mcp_servers {
        let env_map: serde_json::Map<String, Value> = std::iter::once((
            "TAPE_WRAP_CMD".to_string(),
            Value::String(spec.command.clone()),
        ))
        .chain(std::iter::once((
            "TAPE_WRAP_ARGS_JSON".to_string(),
            Value::String(serde_json::to_string(&spec.args).unwrap_or_else(|_| "[]".into())),
        )))
        .chain(std::iter::once((
            "TAPE_WRAP_SOCKET".to_string(),
            Value::String(inputs.recorder_socket.display().to_string()),
        )))
        .chain(std::iter::once((
            "TAPE_WRAP_SERVER_NAME".to_string(),
            Value::String(spec.name.clone()),
        )))
        .chain(spec.env.iter().cloned().map(|(k, v)| (k, Value::String(v))))
        .collect();

        servers.insert(
            spec.name.clone(),
            json!({
                "command": inputs.tape_mcp_wrap_bin.display().to_string(),
                "args": [],
                "env": Value::Object(env_map),
            }),
        );
    }
    json!({"mcpServers": Value::Object(servers)})
}

/// Write both overlay files into `dir` and return their paths.
pub fn write_overlay_files(
    dir: &Path,
    inputs: &OverlayInputs,
) -> std::io::Result<(PathBuf, PathBuf)> {
    let settings_path = dir.join("settings.json");
    let mcp_path = dir.join("mcp.json");
    std::fs::write(
        &settings_path,
        serde_json::to_vec_pretty(&settings_overlay(inputs))
            .expect("settings overlay serializes"),
    )?;
    std::fs::write(
        &mcp_path,
        serde_json::to_vec_pretty(&mcp_config(inputs))
            .expect("mcp config serializes"),
    )?;
    Ok((settings_path, mcp_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> OverlayInputs {
        OverlayInputs {
            tape_hook_bin: "/usr/local/bin/tape-hook".into(),
            tape_mcp_wrap_bin: "/usr/local/bin/tape-mcp-wrap".into(),
            recorder_socket: "/tmp/tape-1234/recorder.sock".into(),
            mcp_servers: vec![McpServerSpec {
                name: "filesystem".into(),
                command: "mcp-server-filesystem".into(),
                args: vec!["/tmp".into()],
                env: vec![],
            }],
        }
    }

    #[test]
    fn settings_overlay_has_expected_hooks() {
        let s = settings_overlay(&inputs());
        let pre = &s["hooks"]["PreToolUse"];
        let post = &s["hooks"]["PostToolUse"];
        assert!(pre.is_array());
        assert!(post.is_array());
        assert_eq!(
            pre.as_array().unwrap().len(),
            2,
            "Bash (shell_pre) + Write|Edit|MultiEdit|NotebookEdit (file_write_pre)"
        );
        assert_eq!(
            post.as_array().unwrap().len(),
            3,
            "Bash + Read + Write|Edit|MultiEdit|NotebookEdit"
        );
    }

    #[test]
    fn settings_overlay_pre_hook_matches_write_edit_multiedit() {
        let s = settings_overlay(&inputs());
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap().clone();
        let has_bash_pre = pre.iter().any(|h| h["matcher"] == "Bash");
        let has_write_pre = pre.iter().any(|h| h["matcher"] == "Write|Edit|MultiEdit|NotebookEdit");
        assert!(has_bash_pre, "expected a PreToolUse hook matching Bash");
        assert!(
            has_write_pre,
            "expected a PreToolUse hook matching Write|Edit|MultiEdit|NotebookEdit"
        );
        // Confirm the file-write PreToolUse hook carries the `file_write_pre`
        // discriminator so tape-hook routes it correctly.
        let write_pre = pre
            .iter()
            .find(|h| h["matcher"] == "Write|Edit|MultiEdit|NotebookEdit")
            .unwrap();
        let cmd = write_pre["hooks"][0]["command"].as_str().unwrap();
        assert!(
            cmd.contains("TAPE_HOOK_KIND=file_write_pre"),
            "expected file_write_pre kind in command, got: {cmd}"
        );
    }

    /// Issue #75: the overlay must cover every tool that `tape-hook`'s
    /// `file_write_event` accepts. `NotebookEdit` was dispatched by the
    /// hook and by the transcript converter but missing from the
    /// PreToolUse / PostToolUse matchers, so live recordings dropped
    /// notebook edits.
    #[test]
    fn overlay_matchers_cover_every_file_write_tool() {
        let s = settings_overlay(&inputs());
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap().clone();
        let post = s["hooks"]["PostToolUse"].as_array().unwrap().clone();

        for tool in &["Write", "Edit", "MultiEdit", "NotebookEdit"] {
            let pre_hit = pre
                .iter()
                .filter_map(|h| h["matcher"].as_str())
                .any(|m| m.split('|').any(|alt| alt == *tool));
            let post_hit = post
                .iter()
                .filter_map(|h| h["matcher"].as_str())
                .any(|m| m.split('|').any(|alt| alt == *tool));
            assert!(
                pre_hit,
                "no overlay PreToolUse matcher covers tool {tool}; got {pre:?}"
            );
            assert!(
                post_hit,
                "no overlay PostToolUse matcher covers tool {tool}; got {post:?}"
            );
        }
    }

    #[test]
    fn mcp_config_wraps_each_server() {
        let cfg = mcp_config(&inputs());
        let servers = &cfg["mcpServers"];
        assert!(servers["filesystem"].is_object());
        let env = &servers["filesystem"]["env"];
        assert_eq!(env["TAPE_WRAP_CMD"], "mcp-server-filesystem");
        assert_eq!(env["TAPE_WRAP_SERVER_NAME"], "filesystem");
    }
}
