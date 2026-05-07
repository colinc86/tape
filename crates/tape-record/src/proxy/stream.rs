//! Streaming-tee primitive shared by all proxies.
//!
//! The hard requirement (brief §"Streaming non-buffered"): bytes flow from
//! upstream to the child as they arrive. At the same time, a clone of every
//! byte goes to the recorder. **No buffering of the full response before
//! yielding to the child.**
//!
//! Implementation: take an upstream `Stream<Item = Result<Bytes, _>>`; for
//! each chunk, send it to a `tokio::sync::mpsc` (the recorder) in addition
//! to yielding it from the new stream. The mpsc is not `await`-blocking in
//! the hot path — we use `try_send` and fall back to spawning if the channel
//! is full, so a slow recorder can never wedge the child.

use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A stream wrapper that copies each chunk to a recorder channel as it
/// passes through to the consumer.
pub struct TeeStream<S> {
    inner: S,
    sink: tokio::sync::mpsc::UnboundedSender<Bytes>,
}

impl<S> TeeStream<S> {
    pub fn new(inner: S, sink: tokio::sync::mpsc::UnboundedSender<Bytes>) -> Self {
        Self { inner, sink }
    }
}

impl<S, E> Stream for TeeStream<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                // Unbounded send is non-blocking; we only fail if the receiver
                // dropped, which means the recorder shut down — in that case
                // we still want to deliver the chunk to the child.
                let _ = self.sink.send(chunk.clone());
                Poll::Ready(Some(Ok(chunk)))
            }
            other => other,
        }
    }
}

/// Drain a chunk receiver into a single concatenated `Bytes`. Used after the
/// upstream stream is exhausted to assemble the recorded response.
pub async fn drain(mut rx: tokio::sync::mpsc::UnboundedReceiver<Bytes>) -> Bytes {
    let mut total = Vec::new();
    while let Some(chunk) = rx.recv().await {
        total.extend_from_slice(&chunk);
    }
    Bytes::from(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn tee_yields_in_order_and_records_all_bytes() {
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from_static(b"foo")),
            Ok(Bytes::from_static(b"bar")),
            Ok(Bytes::from_static(b"baz")),
        ];
        let upstream = futures::stream::iter(chunks);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let tee = TeeStream::new(upstream, tx);

        let received: Vec<Bytes> = tee.map(|r| r.unwrap()).collect().await;
        let recorded = drain(rx).await;
        assert_eq!(
            received.iter().fold(Vec::<u8>::new(), |mut a, b| {
                a.extend_from_slice(b);
                a
            }),
            b"foobarbaz".to_vec()
        );
        assert_eq!(recorded.as_ref(), b"foobarbaz");
    }
}
