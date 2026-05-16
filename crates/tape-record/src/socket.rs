//! Recorder-side Unix domain socket. External processes (the MCP wrapper,
//! Claude Code hooks) connect and POST line-delimited JSON events. Each line
//! is a partial track (everything except `step`); the receiver assigns the
//! step monotonically via `Session::append`.
//!
//! Wire format: one JSON object per line, terminated by `\n`. Required:
//!   { "kind": "<kind>", "payload": { ... } }
//! Optional:
//!   { "ts": "<iso8601>" }   // recorder injects current time if absent

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tape_format::tracks::Kind;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, warn};

use crate::session::Session;

#[derive(Debug, Deserialize)]
struct WireEvent {
    kind: WireKind,
    payload: serde_json::Value,
    #[allow(dead_code)]
    ts: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireKind {
    Task,
    ModelCall,
    McpCall,
    Shell,
    FileRead,
    FileWrite,
    Annotation,
    Eject,
}

impl From<WireKind> for Kind {
    fn from(value: WireKind) -> Self {
        match value {
            WireKind::Task => Kind::Task,
            WireKind::ModelCall => Kind::ModelCall,
            WireKind::McpCall => Kind::McpCall,
            WireKind::Shell => Kind::Shell,
            WireKind::FileRead => Kind::FileRead,
            WireKind::FileWrite => Kind::FileWrite,
            WireKind::Annotation => Kind::Annotation,
            WireKind::Eject => Kind::Eject,
        }
    }
}

/// Handle to the running socket listener.
pub struct SocketHandle {
    pub path: PathBuf,
    shutdown: tokio::sync::oneshot::Sender<()>,
    join: tokio::task::JoinHandle<()>,
}

impl SocketHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.join.await;
        // best-effort cleanup of the socket file
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Spawn a Unix-socket listener on `path`. Each connection is served line-by-line:
/// every line is parsed as a `WireEvent` and appended to the session.
pub async fn spawn(path: PathBuf, session: Session) -> std::io::Result<SocketHandle> {
    // Clean any stale socket file from a previous run.
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let session = Arc::new(session);
    let path_for_handle = path.clone();

    let join = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accept = listener.accept() => match accept {
                    Ok((stream, _)) => {
                        let s = session.clone();
                        tokio::spawn(handle_connection(stream, s));
                    }
                    Err(e) => {
                        warn!(?e, "recorder socket accept failed");
                        break;
                    }
                }
            }
        }
    });

    Ok(SocketHandle {
        path: path_for_handle,
        shutdown: shutdown_tx,
        join,
    })
}

/// Maximum time to wait for the next line on an open recorder connection.
/// A client that connects and never sends data closes after this elapses,
/// so a misbehaving hook can't tie up a tokio task forever.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

async fn handle_connection(stream: UnixStream, session: Arc<Session>) {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    loop {
        match tokio::time::timeout(IDLE_TIMEOUT, lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<WireEvent>(&line) {
                    Ok(ev) => {
                        let kind: Kind = ev.kind.into();
                        let step = session.append(kind, ev.payload);
                        debug!(step, ?kind, "recorder socket appended event");
                    }
                    Err(e) => {
                        warn!(%e, %line, "malformed event on recorder socket");
                    }
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(e)) => {
                warn!(%e, "recorder socket read error");
                break;
            }
            Err(_elapsed) => {
                // Idle timeout — close the connection. Sender can reconnect.
                debug!("recorder socket idle timeout");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn socket_round_trips_events() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("rec.sock");
        let session = Session::start("t", "test/0.0.1");
        let handle = spawn(sock.clone(), session.clone()).await.unwrap();

        let mut client = UnixStream::connect(&sock).await.unwrap();
        let line = serde_json::to_string(&serde_json::json!({
            "kind": "mcp_call",
            "payload": {
                "server": "fs",
                "tool": "read_file",
                "args": {"path": "/etc/hosts"},
                "result": {"bytes": 0}
            }
        }))
        .unwrap();
        client.write_all(line.as_bytes()).await.unwrap();
        client.write_all(b"\n").await.unwrap();
        client.shutdown().await.unwrap();
        drop(client);

        // Give the listener a moment to drain.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(session.track_count(), 2, "task + mcp_call");

        handle.shutdown().await;
    }
}
