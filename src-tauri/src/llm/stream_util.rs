use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use std::time::Duration;

/// Default idle timeout per chunk: if no data arrives for this long, the stream is stalled.
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Wraps a byte stream with a per-chunk idle timeout.
/// Returns `Err` if a single chunk takes longer than `timeout` to arrive.
/// Returns `Ok(None)` when the stream ends normally.
pub async fn next_chunk_with_timeout<S, E>(
    stream: &mut Pin<&mut S>,
    timeout: Duration,
) -> Result<Option<bytes::Bytes>, String>
where
    S: Stream<Item = Result<bytes::Bytes, E>>,
    E: std::fmt::Display,
{
    if crate::utils::cancel::is_cancelled() {
        return Ok(None);
    }
    match tokio::time::timeout(timeout, stream.next()).await {
        Ok(Some(Ok(chunk))) => Ok(Some(chunk)),
        Ok(Some(Err(e))) => Err(format!("Stream error: {}", e)),
        Ok(None) => Ok(None), // Stream ended
        Err(_) => {
            if crate::utils::cancel::is_cancelled() {
                Ok(None)
            } else {
                Err(format!(
                    "Stream stalled — no data received for {}s. The LLM may be unresponsive.",
                    timeout.as_secs()
                ))
            }
        }
    }
}
