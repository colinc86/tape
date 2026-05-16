//! `tape-mcp-wrap` — a JSON-RPC tee for MCP servers.
//!
//! When `tape record` runs, it generates a temporary `mcp.json` that points
//! at this binary instead of the user-configured MCP server. We re-spawn the
//! real server, splice ourselves between Claude Code and the server's
//! stdin/stdout, and post `mcp_call` events to the recorder Unix socket
//! whenever a `tools/call` request/response pair completes.
//!
//! The wrap is fully transparent: every byte from the client reaches the
//! server in order; every byte from the server reaches the client in order.
//! Recording happens on a side-channel.
//!
//! Configuration via env (set by `tape record` when generating the temp config):
//!   `TAPE_WRAP_CMD`          path to the real MCP server binary
//!   `TAPE_WRAP_ARGS_JSON`    JSON array of args to pass to the real server
//!   `TAPE_WRAP_SOCKET`       path to the recorder Unix socket
//!   `TAPE_WRAP_SERVER_NAME`  logical server name to attribute `mcp_call` events to

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cmd = std::env::var("TAPE_WRAP_CMD").context("TAPE_WRAP_CMD not set")?;
    let args_json = std::env::var("TAPE_WRAP_ARGS_JSON").unwrap_or_else(|_| "[]".to_owned());
    let args: Vec<String> =
        serde_json::from_str(&args_json).context("TAPE_WRAP_ARGS_JSON not a JSON array")?;
    let socket_path = std::env::var("TAPE_WRAP_SOCKET").context("TAPE_WRAP_SOCKET not set")?;
    let server_name =
        std::env::var("TAPE_WRAP_SERVER_NAME").unwrap_or_else(|_| "unknown".to_owned());

    // Connect to recorder socket. If unreachable, we still proxy traffic so
    // the user's session keeps working — recording is best-effort.
    let recorder = match UnixStream::connect(&socket_path).await {
        Ok(s) => Some(Arc::new(Mutex::new(s))),
        Err(e) => {
            tracing::warn!(?e, "could not connect to recorder socket; proxying without recording");
            None
        }
    };

    // Spawn the real MCP server.
    let mut child = Command::new(&cmd)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawn real MCP server: {cmd}"))?;

    let server_stdin = child.stdin.take().ok_or_else(|| anyhow!("no server stdin"))?;
    let server_stdout = child.stdout.take().ok_or_else(|| anyhow!("no server stdout"))?;

    let pending: Arc<Mutex<HashMap<String, PendingCall>>> = Arc::new(Mutex::new(HashMap::new()));

    // Task: client → server. Read newline-delimited JSON from stdin, parse,
    // remember any tools/call request keyed by id, forward verbatim.
    let pending_c2s = pending.clone();
    let server_stdin_arc = Arc::new(Mutex::new(server_stdin));
    let server_stdin_for_c2s = server_stdin_arc.clone();
    let c2s = tokio::spawn(client_to_server(
        tokio::io::stdin(),
        server_stdin_for_c2s,
        pending_c2s,
    ));

    // Task: server → client. Read newline-delimited JSON from server stdout,
    // forward to our stdout, and on response match emit mcp_call event.
    let pending_s2c = pending.clone();
    let recorder_for_s2c = recorder.clone();
    let s2c = tokio::spawn(server_to_client(
        server_stdout,
        tokio::io::stdout(),
        pending_s2c,
        recorder_for_s2c,
        server_name,
    ));

    // Wait for either direction to finish. When the client closes stdin, c2s
    // returns; we then close server stdin and give the server a brief grace
    // period to flush any pending response, then exit.
    //
    // We don't `child.wait()` indefinitely — if the server is misbehaving,
    // it gets killed when our process exits.
    use std::time::Duration;
    let either = tokio::select! {
        r = c2s => ("c2s", r),
        r = s2c => ("s2c", r),
    };
    tracing::debug!(direction = either.0, "wrap shutting down");
    // P2 #12: drop the Arc<Mutex<ChildStdin>> outright. This closes the FD
    // when the inner ChildStdin drops, signaling EOF to the server. No lock
    // contention with s2c (which holds an Arc<Mutex<UnixStream>>, not stdin).
    drop(server_stdin_arc);
    let _ = tokio::time::timeout(Duration::from_millis(500), child.wait()).await;
    let _ = child.start_kill();
    Ok(())
}

#[derive(Debug, Clone)]
struct PendingCall {
    request: Value,
    started: Instant,
}

/// Maximum age for a pending `tool_use` entry before it's evicted as stale.
///
/// Bounds memory if the server never replies to a request. Set to 1 hour
/// (well past any realistic single MCP tool call) so that slow tools — e.g.
/// long-running shell commands, large model inferences, network requests
/// with deep retries — don't get their pending entries evicted before the
/// response arrives, which would silently drop the `mcp_call` event from
/// the recording. The wrap process is short-lived (exits with its parent
/// Claude Code session), so the real memory ceiling is the process
/// lifetime, not this TTL. See issue #53.
const PENDING_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

async fn client_to_server<R>(
    client_stdin: R,
    server_stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<String, PendingCall>>>,
) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut reader = BufReader::new(client_stdin);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if !trimmed.is_empty() {
            // Try to parse as JSON-RPC request to track tools/call invocations.
            if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                if let Some(method) = v.get("method").and_then(Value::as_str) {
                    if method == "tools/call" {
                        if let Some(id) = id_to_string(v.get("id")) {
                            let mut p = pending.lock().await;
                            // Opportunistic GC: drop entries older than TTL
                            // before inserting. Cheap (HashMap walk).
                            let cutoff = Instant::now()
                                .checked_sub(PENDING_TTL)
                                .unwrap_or_else(Instant::now);
                            p.retain(|_, call| call.started >= cutoff);
                            p.insert(
                                id,
                                PendingCall {
                                    request: v.clone(),
                                    started: Instant::now(),
                                },
                            );
                        }
                    }
                }
            }
        }
        let mut g = server_stdin.lock().await;
        g.write_all(line.as_bytes()).await?;
        g.flush().await?;
    }
    Ok(())
}

async fn server_to_client<W>(
    server_stdout: ChildStdout,
    mut client_stdout: W,
    pending: Arc<Mutex<HashMap<String, PendingCall>>>,
    recorder: Option<Arc<Mutex<UnixStream>>>,
    server_name: String,
) -> Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(server_stdout);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        // Forward verbatim.
        client_stdout.write_all(line.as_bytes()).await?;
        client_stdout.flush().await?;

        let trimmed = line.trim_end_matches(['\n', '\r']);
        if let Ok(resp) = serde_json::from_str::<Value>(trimmed) {
            if let Some(id) = id_to_string(resp.get("id")) {
                let mut p = pending.lock().await;
                if let Some(pending_call) = p.remove(&id) {
                    drop(p);
                    let event = build_mcp_call_event(
                        &pending_call.request,
                        &resp,
                        pending_call.started.elapsed().as_millis() as u64,
                        &server_name,
                    );
                    if let Some(recorder) = recorder.as_ref() {
                        post_event(recorder, &event).await.ok();
                    }
                }
            }
        }
    }
    Ok(())
}

fn id_to_string(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn build_mcp_call_event(req: &Value, resp: &Value, duration_ms: u64, server: &str) -> Value {
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let tool = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);

    let (result, error) = if let Some(err) = resp.get("error") {
        (Value::Null, Some(err.clone()))
    } else {
        (resp.get("result").cloned().unwrap_or(Value::Null), None)
    };

    let mut payload = serde_json::json!({
        "server": server,
        "tool": tool,
        "args": args,
        "result": result,
        "duration_ms": duration_ms,
    });
    if let Some(err) = error {
        payload["error"] = err;
    }
    serde_json::json!({"kind": "mcp_call", "payload": payload})
}

async fn post_event(recorder: &Arc<Mutex<UnixStream>>, event: &Value) -> Result<()> {
    let line = serde_json::to_string(event)?;
    let mut g = recorder.lock().await;
    g.write_all(line.as_bytes()).await?;
    g.write_all(b"\n").await?;
    g.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for issue #53.
    ///
    /// The wrap evicts pending tools/call entries older than PENDING_TTL when a
    /// new tools/call arrives. With the old 5-minute TTL, slow tools (>5 min)
    /// had their pending entries evicted before the response arrived, so the
    /// response could not be paired with its request — the `mcp_call` event was
    /// silently dropped from the recording. The TTL must be large enough to
    /// survive any realistic single MCP tool call.
    #[test]
    fn pending_ttl_is_at_least_one_hour() {
        assert!(
            PENDING_TTL >= std::time::Duration::from_secs(3600),
            "PENDING_TTL ({:?}) must be ≥ 1h to avoid evicting in-flight slow MCP \
             tool calls (issue #53). Lowering this risks silently dropping \
             mcp_call events for tools that take longer than the TTL to reply.",
            PENDING_TTL,
        );
    }

    /// Exercise the same eviction-cutoff math the c2s loop uses to confirm
    /// that, under the new TTL, a 10-minute-old pending entry would survive
    /// (the old 5-min TTL would have evicted it and dropped its response),
    /// while a 2-hour-old entry would still be reaped.
    #[test]
    fn eviction_cutoff_keeps_ten_minute_old_entries_drops_two_hour_old() {
        let now = Instant::now();
        // Reproduce the c2s eviction expression verbatim.
        let cutoff = now.checked_sub(PENDING_TTL).unwrap_or(now);

        // 10 minutes old — the canonical "slow tool" case that the old
        // 5-minute TTL dropped. Under the 1-hour TTL it must survive.
        let ten_min_old = now
            .checked_sub(std::time::Duration::from_secs(10 * 60))
            .expect("Instant arithmetic");
        assert!(
            ten_min_old >= cutoff,
            "10-minute-old pending entry must survive eviction under PENDING_TTL={:?}",
            PENDING_TTL,
        );

        // 2 hours old — well past any sane MCP tool call. Should still be
        // reaped so an unresponsive server can't leak memory forever.
        let two_hours_old = now
            .checked_sub(std::time::Duration::from_secs(2 * 3600))
            .expect("Instant arithmetic");
        assert!(
            two_hours_old < cutoff,
            "2-hour-old pending entry must be evicted under PENDING_TTL={:?}",
            PENDING_TTL,
        );
    }
}
