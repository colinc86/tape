//! Smoke test for the OpenAI recording proxy. Same shape as the Anthropic
//! test but pointed at `/v1/chat/completions` and verifying `vendor: "openai"`.

use std::time::Duration;

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
use tape_record::proxy::openai::{spawn, ProxyConfig};
use tape_record::session::Session;

const STREAM_CHUNKS: usize = 6;
const CHUNK_SPACING_MS: u64 = 50;

#[derive(Clone)]
struct MockState;

async fn mock_chat(AxState(_): AxState<MockState>, body: axum::body::Bytes) -> Response<Body> {
    let req: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    let stream_requested = req.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    if stream_requested {
        let mut chunks: Vec<Result<Bytes, std::io::Error>> = (0..STREAM_CHUNKS)
            .map(|i| {
                Ok(Bytes::from(format!(
                    "data: {{\"choices\":[{{\"delta\":{{\"content\":\"chunk{i} \"}}}}]}}\n\n"
                )))
            })
            .collect();
        chunks.push(Ok(Bytes::from_static(b"data: [DONE]\n\n")));

        let body_stream = stream::iter(chunks).then(|c| async {
            tokio::time::sleep(Duration::from_millis(CHUNK_SPACING_MS)).await;
            c
        });
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(body_stream))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "id": "chatcmpl_test",
                    "object": "chat.completion",
                    "choices": [{"message": {"role": "assistant", "content": "ok"}}]
                }))
                .unwrap(),
            ))
            .unwrap()
    }
}

async fn spawn_mock_upstream() -> (std::net::SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let app: Router = Router::new()
        .route("/v1/chat/completions", post(mock_chat))
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

#[tokio::test]
async fn openai_streaming_records_with_vendor_label() {
    let (mock_addr, mock_shutdown) = spawn_mock_upstream().await;
    let session = Session::start("openai stream", "test/0.0.1");
    let mut cfg: ProxyConfig = ProxyConfig::openai();
    cfg.upstream = format!("http://{mock_addr}");
    cfg.request_timeout = None;
    let proxy = spawn(cfg, session.clone()).await.unwrap();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/chat/completions", proxy.base_url()))
        .json(&serde_json::json!({
            "model": "gpt-4o",
            "stream": true,
            "messages": [{"role": "user", "content": "x"}]
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let mut s = resp.bytes_stream();
    while s.next().await.is_some() {}

    tokio::time::sleep(Duration::from_millis(150)).await;
    let snap = session.snapshot();
    assert_eq!(snap.tracks.len(), 2);
    let mc = &snap.tracks[1];
    assert_eq!(mc.kind, Kind::ModelCall);
    assert_eq!(mc.payload["vendor"], "openai");
    assert_eq!(mc.payload["model"], "gpt-4o");
    let chunks = mc.payload["stream_chunks"].as_u64().unwrap();
    // STREAM_CHUNKS data lines + the [DONE] terminator are both counted as chunks
    // (count_sse_chunks counts all `data: ` lines), but parse_sse_to_value drops [DONE].
    assert_eq!(chunks, (STREAM_CHUNKS as u64) + 1);

    proxy.shutdown().await;
    let _ = mock_shutdown.send(());
}

#[tokio::test]
async fn openai_non_streaming_records() {
    let (mock_addr, mock_shutdown) = spawn_mock_upstream().await;
    let session = Session::start("openai", "test/0.0.1");
    let mut cfg: ProxyConfig = ProxyConfig::openai();
    cfg.upstream = format!("http://{mock_addr}");
    cfg.request_timeout = None;
    let proxy = spawn(cfg, session.clone()).await.unwrap();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/chat/completions", proxy.base_url()))
        .json(&serde_json::json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "x"}]
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["choices"][0]["message"]["content"], "ok");

    let snap = session.snapshot();
    assert_eq!(snap.tracks.len(), 2);
    let mc = &snap.tracks[1];
    assert_eq!(mc.payload["vendor"], "openai");
    assert!(mc.payload["response"].is_object());

    proxy.shutdown().await;
    let _ = mock_shutdown.send(());
}
