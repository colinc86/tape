//! End-to-end tests for the Anthropic recording proxy.
//!
//! Strategy: stand up a tiny mock upstream (axum) that emits SSE chunks at a
//! controlled cadence, point the proxy at it, then drive a client through
//! the proxy and assert:
//!  1. The client observes streaming preserved (chunk-arrival cadence).
//!  2. The recorded `model_call` event captured all SSE chunks.
//!  3. The non-streaming path also records correctly.

use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::State as AxState,
    http::{header, StatusCode},
    response::Response,
    routing::post,
    Router,
};
use bytes::Bytes;
use futures::{stream, StreamExt};
use tape_format::tracks::Kind;
use tape_record::proxy::anthropic::{spawn, ProxyConfig};
use tape_record::session::Session;

const STREAM_CHUNKS: usize = 8;
const CHUNK_SPACING_MS: u64 = 60;

#[derive(Clone)]
struct MockState;

async fn mock_messages(AxState(_): AxState<MockState>, body: axum::body::Bytes) -> Response<Body> {
    let req: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    let stream_requested = req.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    if stream_requested {
        let chunks: Vec<Result<Bytes, std::io::Error>> = (0..STREAM_CHUNKS)
            .map(|i| {
                let line = format!(
                    "data: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"chunk{i} \"}}}}\n\n"
                );
                Ok(Bytes::from(line))
            })
            .collect();
        let body_stream = stream::iter(chunks).then(|c| async {
            tokio::time::sleep(Duration::from_millis(CHUNK_SPACING_MS)).await;
            c
        });
        let body = Body::from_stream(body_stream);
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(body)
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "id": "msg_test",
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "text", "text": "ok"}]
                }))
                .unwrap(),
            ))
            .unwrap()
    }
}

async fn spawn_mock_upstream() -> (std::net::SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let app: Router = Router::new()
        .route("/v1/messages", post(mock_messages))
        .with_state(MockState);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });
    (addr, tx)
}

/// A canned-response upstream used by the HTTP-error tests below. The handler
/// echoes back a fixed status code, content-type, and body for any POST to
/// `/v1/messages`, regardless of the request payload.
#[derive(Clone)]
struct CannedState {
    status: StatusCode,
    content_type: &'static str,
    body: Vec<u8>,
}

async fn canned_messages(AxState(state): AxState<CannedState>, _body: axum::body::Bytes) -> Response<Body> {
    Response::builder()
        .status(state.status)
        .header(header::CONTENT_TYPE, state.content_type)
        .body(Body::from(state.body))
        .unwrap()
}

async fn spawn_canned_upstream(
    status: StatusCode,
    content_type: &'static str,
    body: Vec<u8>,
) -> (std::net::SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let app: Router = Router::new()
        .route("/v1/messages", post(canned_messages))
        .with_state(CannedState {
            status,
            content_type,
            body,
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });
    (addr, tx)
}

/// Drive a non-streaming POST through the proxy and return the recorded
/// model_call payload.
async fn record_non_streaming_against(
    upstream_addr: std::net::SocketAddr,
    task: &'static str,
) -> serde_json::Value {
    let session = Session::start(task, "test/0.0.1");
    let mut cfg = ProxyConfig::anthropic();
    cfg.upstream = format!("http://{upstream_addr}");
    cfg.request_timeout = None;
    let proxy = spawn(cfg, session.clone()).await.unwrap();
    let proxy_url = proxy.base_url();

    let client = reqwest::Client::new();
    // We don't assert on the client-side status here — the tests calling this
    // helper care about the recorded payload, not the proxied response.
    let _ = client
        .post(format!("{proxy_url}/v1/messages"))
        .json(&serde_json::json!({
            "model": "claude-opus-4-7",
            "messages": [{"role": "user", "content": "x"}]
        }))
        .send()
        .await
        .unwrap();

    let snap = session.snapshot();
    assert_eq!(snap.tracks.len(), 2, "expected task + model_call");
    let mc = &snap.tracks[1];
    assert_eq!(mc.kind, Kind::ModelCall);
    let payload = mc.payload.clone();

    proxy.shutdown().await;
    payload
}

#[tokio::test]
async fn streaming_chunks_are_not_buffered() {
    let (mock_addr, mock_shutdown) = spawn_mock_upstream().await;
    let session = Session::start("stream test", "test/0.0.1");
    let mut cfg = ProxyConfig::anthropic();
    cfg.upstream = format!("http://{mock_addr}");
    cfg.request_timeout = None;
    let proxy = spawn(cfg, session.clone()).await.unwrap();
    let proxy_url = proxy.base_url();

    // Drive a client through the proxy and time inter-chunk arrivals.
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .json(&serde_json::json!({
            "model": "claude-opus-4-7",
            "stream": true,
            "messages": [{"role": "user", "content": "x"}]
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let mut stream = resp.bytes_stream();

    let start = Instant::now();
    let mut chunk_times = Vec::new();
    let mut total_bytes = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        total_bytes += chunk.len();
        chunk_times.push(start.elapsed());
    }
    let span = chunk_times.last().unwrap().as_millis() as u64
        - chunk_times.first().unwrap().as_millis() as u64;

    // Streaming preserved: spread between first and last chunk should be at
    // least ~half the upstream's emission span (allowing for minor coalescing).
    let expected_min_span = (STREAM_CHUNKS as u64 - 1) * CHUNK_SPACING_MS / 2;
    assert!(
        span >= expected_min_span,
        "chunk arrivals only spanned {span} ms; upstream emitted across {} ms (proxy is buffering!)",
        (STREAM_CHUNKS as u64 - 1) * CHUNK_SPACING_MS
    );
    assert!(total_bytes > 0);

    // Give the recorder side a moment to drain.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let snap = session.snapshot();
    assert_eq!(snap.tracks.len(), 2, "expected task + model_call");
    let mc = &snap.tracks[1];
    assert_eq!(mc.kind, Kind::ModelCall);
    let chunks = mc.payload["stream_chunks"].as_u64().unwrap();
    assert_eq!(
        chunks, STREAM_CHUNKS as u64,
        "all SSE chunks should be recorded"
    );

    proxy.shutdown().await;
    let _ = mock_shutdown.send(());
}

#[tokio::test]
async fn non_streaming_request_records() {
    let (mock_addr, mock_shutdown) = spawn_mock_upstream().await;
    let session = Session::start("non-stream test", "test/0.0.1");
    let mut cfg = ProxyConfig::anthropic();
    cfg.upstream = format!("http://{mock_addr}");
    cfg.request_timeout = None;
    let proxy = spawn(cfg, session.clone()).await.unwrap();
    let proxy_url = proxy.base_url();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .json(&serde_json::json!({
            "model": "claude-opus-4-7",
            "messages": [{"role": "user", "content": "x"}]
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["content"][0]["text"], "ok");

    let snap = session.snapshot();
    assert_eq!(snap.tracks.len(), 2, "expected task + model_call");
    let mc = &snap.tracks[1];
    assert_eq!(mc.kind, Kind::ModelCall);
    assert!(mc.payload["response"].is_object());
    // Regression for #6: 2xx must NOT inject an `error` field, but should
    // still carry the non-normative `status_code` diagnostic.
    assert!(
        mc.payload.get("error").is_none(),
        "2xx payload must not carry an `error` field, got: {:?}",
        mc.payload.get("error")
    );
    assert_eq!(mc.payload["status_code"].as_u64(), Some(200));

    proxy.shutdown().await;
    let _ = mock_shutdown.send(());
}

// --- HTTP failure recording (issue #6) -------------------------------------

#[tokio::test]
async fn http_429_records_error_field() {
    let body = serde_json::to_vec(&serde_json::json!({
        "type": "error",
        "error": {"type": "rate_limit_error", "message": "slow down"}
    }))
    .unwrap();
    let (addr, shutdown) =
        spawn_canned_upstream(StatusCode::TOO_MANY_REQUESTS, "application/json", body).await;

    let payload = record_non_streaming_against(addr, "429 test").await;

    assert_eq!(
        payload["error"]["code"].as_str(),
        Some("HTTP_429"),
        "payload: {payload}"
    );
    assert_eq!(payload["status_code"].as_u64(), Some(429));
    let _ = shutdown.send(());
}

#[tokio::test]
async fn http_401_records_error_and_preserves_json_body() {
    let body_json = serde_json::json!({
        "type": "error",
        "error": {"type": "authentication_error", "message": "invalid x-api-key"}
    });
    let body = serde_json::to_vec(&body_json).unwrap();
    let (addr, shutdown) =
        spawn_canned_upstream(StatusCode::UNAUTHORIZED, "application/json", body).await;

    let payload = record_non_streaming_against(addr, "401 test").await;

    // Error field present and well-formed.
    assert_eq!(payload["error"]["code"].as_str(), Some("HTTP_401"));
    assert!(
        payload["error"]["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "error.message should be non-empty, got: {:?}",
        payload["error"]["message"]
    );
    assert_eq!(payload["status_code"].as_u64(), Some(401));
    // Response body is preserved as parsed JSON.
    assert_eq!(payload["response"], body_json);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn http_500_plaintext_body_records_error_and_raw_response() {
    let plain = b"upstream blew up".to_vec();
    let (addr, shutdown) =
        spawn_canned_upstream(StatusCode::INTERNAL_SERVER_ERROR, "text/plain", plain).await;

    let payload = record_non_streaming_against(addr, "500 plaintext test").await;

    assert_eq!(payload["error"]["code"].as_str(), Some("HTTP_500"));
    assert_eq!(payload["status_code"].as_u64(), Some(500));
    // Plaintext body is preserved verbatim as a JSON string.
    assert_eq!(payload["response"].as_str(), Some("upstream blew up"));

    let _ = shutdown.send(());
}
