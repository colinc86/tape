//! Shared HTTP recording proxy. Each vendor (Anthropic, OpenAI, …) wraps this
//! with its own URL and path defaults.
//!
//! Behavior:
//!  - `POST <recorded_path>` is forwarded to upstream and recorded as
//!    a `model_call` event tagged with `vendor`.
//!  - Any other path is forwarded transparently and not recorded.
//!  - Streaming responses (SSE) are tee'd to the recorder while bytes flow
//!    through to the child unbuffered.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::Response,
    routing::any,
    Router,
};
use bytes::Bytes;
use futures::TryStreamExt;
use reqwest::Client;
use serde_json::Value;
use tape_format::tracks::Kind;
use tracing::warn;

use crate::proxy::stream::{drain, TeeStream};
use crate::session::Session;

/// Configuration for one vendor's proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Real upstream URL (e.g. `https://api.anthropic.com`).
    pub upstream: String,
    /// Vendor name as it appears in the recorded `model_call.payload.vendor`.
    pub vendor: String,
    /// HTTP path that should be recorded (anything else passes through).
    pub recorded_path: String,
    /// Optional bind address override (default: `127.0.0.1:0`).
    pub bind: String,
    /// Per-request timeout. None = no timeout.
    pub request_timeout: Option<Duration>,
}

impl ProxyConfig {
    pub fn anthropic() -> Self {
        Self {
            upstream: "https://api.anthropic.com".into(),
            vendor: "anthropic".into(),
            recorded_path: "/v1/messages".into(),
            bind: "127.0.0.1:0".into(),
            request_timeout: Some(Duration::from_secs(120)),
        }
    }

    pub fn openai() -> Self {
        Self {
            upstream: "https://api.openai.com".into(),
            vendor: "openai".into(),
            recorded_path: "/v1/chat/completions".into(),
            bind: "127.0.0.1:0".into(),
            request_timeout: Some(Duration::from_secs(120)),
        }
    }
}

/// Handle to a running proxy. Drop the handle to shut it down.
pub struct ProxyHandle {
    pub local_addr: std::net::SocketAddr,
    shutdown: tokio::sync::oneshot::Sender<()>,
    join: tokio::task::JoinHandle<()>,
}

impl ProxyHandle {
    pub fn base_url(&self) -> String {
        format!("http://{}", self.local_addr)
    }

    pub async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.join.await;
    }
}

#[derive(Clone)]
struct AppState {
    upstream: Arc<String>,
    vendor: Arc<String>,
    recorded_path: Arc<String>,
    client: Client,
    session: Session,
}

/// Spawn the proxy on `cfg.bind` and return a handle.
pub async fn spawn(cfg: ProxyConfig, session: Session) -> std::io::Result<ProxyHandle> {
    let mut builder = reqwest::Client::builder();
    if let Some(t) = cfg.request_timeout {
        builder = builder.timeout(t);
    }
    let client = builder.build().expect("reqwest client builds");

    let recorded_path_for_router = cfg.recorded_path.clone();
    let state = AppState {
        upstream: Arc::new(cfg.upstream),
        vendor: Arc::new(cfg.vendor),
        recorded_path: Arc::new(cfg.recorded_path),
        client,
        session,
    };

    let app: Router = Router::new()
        .route(&recorded_path_for_router, any(handle_recorded))
        .fallback(handle_passthrough)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    let local_addr = listener.local_addr()?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let join = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });
    Ok(ProxyHandle {
        local_addr,
        shutdown: tx,
        join,
    })
}

async fn handle_passthrough(State(state): State<AppState>, req: Request<Body>) -> Response<Body> {
    forward(&state, req, false).await
}

async fn handle_recorded(State(state): State<AppState>, req: Request<Body>) -> Response<Body> {
    // Defensive: if the routed path doesn't match recorded_path, treat as passthrough.
    let path = req.uri().path().to_owned();
    let record = path == state.recorded_path.as_ref().as_str();
    forward(&state, req, record).await
}

async fn forward(state: &AppState, req: Request<Body>, record: bool) -> Response<Body> {
    let started = Instant::now();
    let method = req.method().clone();
    let uri_path_query = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned());
    let req_headers = req.headers().clone();

    let body_bytes = match axum::body::to_bytes(req.into_body(), 32 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            warn!(?e, "failed to read request body");
            return error_response(StatusCode::BAD_REQUEST, "request body read error");
        }
    };

    let req_json: Option<Value> = serde_json::from_slice(&body_bytes).ok();
    let stream_requested = req_json
        .as_ref()
        .and_then(|v| v.get("stream"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let model = req_json
        .as_ref()
        .and_then(|v| v.get("model"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let upstream_url = format!("{}{}", state.upstream, uri_path_query);
    let mut up_req = state
        .client
        .request(method.clone(), &upstream_url)
        .body(body_bytes.clone());
    for (name, value) in req_headers.iter() {
        if matches!(
            name.as_str(),
            "host" | "content-length" | "connection" | "transfer-encoding"
        ) {
            continue;
        }
        up_req = up_req.header(name, value);
    }

    let vendor = state.vendor.as_ref().clone();

    let upstream_resp = match up_req.send().await {
        Ok(r) => r,
        Err(e) => {
            if record {
                state.session.append(
                    Kind::ModelCall,
                    serde_json::json!({
                        "vendor": vendor,
                        "model": model,
                        "request": req_json.unwrap_or_else(|| Value::String(String::from_utf8_lossy(&body_bytes).into_owned())),
                        "error": {"code": "UPSTREAM_UNREACHABLE", "message": e.to_string()},
                        "duration_ms": started.elapsed().as_millis() as u64,
                    }),
                );
            }
            return error_response(StatusCode::BAD_GATEWAY, &format!("upstream: {e}"));
        }
    };

    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();

    let mut response_builder = Response::builder().status(status);
    for (name, value) in resp_headers.iter() {
        if matches!(
            name.as_str(),
            "content-length" | "connection" | "transfer-encoding"
        ) {
            continue;
        }
        response_builder = response_builder.header(name, value);
    }

    if stream_requested && status.is_success() {
        let upstream_stream = upstream_resp.bytes_stream();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Bytes>();
        let teed = TeeStream::new(upstream_stream.map_err(std::io::Error::other), tx);
        let body = Body::from_stream(teed);

        if record {
            let session = state.session.clone();
            let req_json = req_json.clone();
            let model = model.clone();
            let vendor = vendor.clone();
            let status_code = status.as_u16();
            tokio::spawn(async move {
                let assembled = drain(rx).await;
                let response_view = parse_sse_to_value(&assembled);
                let chunk_count = count_sse_chunks(&assembled);
                // Streaming branch is gated on status.is_success() above, so
                // no `error` field is needed here. `status_code` is included
                // for diagnostics, mirroring the non-streaming branch.
                let payload = serde_json::json!({
                    "vendor": vendor,
                    "model": model,
                    "request": req_json.unwrap_or(Value::Null),
                    "response": response_view,
                    "stream_chunks": chunk_count,
                    "status_code": status_code,
                    "duration_ms": started.elapsed().as_millis() as u64,
                });
                session.append(Kind::ModelCall, payload);
            });
        }

        response_builder.body(body).expect("valid response")
    } else {
        let resp_bytes = match upstream_resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return error_response(StatusCode::BAD_GATEWAY, &format!("upstream body: {e}"))
            }
        };

        if record {
            let resp_json: Value = serde_json::from_slice(&resp_bytes).unwrap_or_else(|_| {
                Value::String(String::from_utf8_lossy(&resp_bytes).into_owned())
            });
            let mut payload = serde_json::json!({
                "vendor": vendor,
                "model": model,
                "request": req_json.unwrap_or(Value::Null),
                "response": resp_json,
                "status_code": status.as_u16(),
                "duration_ms": started.elapsed().as_millis() as u64,
            });
            // SPEC §5.5.2: `error` MUST be present on failure. HTTP 4xx/5xx
            // is a failure even though the network round-trip succeeded.
            if !status.is_success() {
                payload["error"] = serde_json::json!({
                    "code": format!("HTTP_{}", status.as_u16()),
                    "message": status.canonical_reason().unwrap_or("upstream error").to_string(),
                });
            }
            state.session.append(Kind::ModelCall, payload);
        }

        let body = Body::from(resp_bytes);
        response_builder.body(body).expect("valid response")
    }
}

fn error_response(status: StatusCode, message: &str) -> Response<Body> {
    use axum::http::HeaderName;
    Response::builder()
        .status(status)
        .header(HeaderName::from_static("content-type"), "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({"error": message})).unwrap(),
        ))
        .expect("valid error response")
}

/// Parse an SSE stream into a `{"events": [...]}` Value. Common across
/// vendors — both Anthropic and OpenAI emit `data: <json>\n\n` framing,
/// with OpenAI also emitting `data: [DONE]` as the terminator.
fn parse_sse_to_value(bytes: &Bytes) -> Value {
    let s = String::from_utf8_lossy(bytes);
    let mut events = Vec::new();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("data: ") {
            if rest == "[DONE]" {
                continue;
            }
            match serde_json::from_str::<Value>(rest) {
                Ok(v) => events.push(v),
                Err(_) => events.push(Value::String(rest.to_owned())),
            }
        }
    }
    serde_json::json!({"events": events})
}

fn count_sse_chunks(bytes: &Bytes) -> u64 {
    let s = String::from_utf8_lossy(bytes);
    s.lines().filter(|l| l.starts_with("data: ")).count() as u64
}
