//! Process-wide shared HTTP client for every LLM provider (and any other
//! outbound HTTPS this crate does).
//!
//! Fixes a real Windows port-exhaustion bug reported in the field:
//! constructing a fresh `reqwest::Client` per request creates a new
//! connection pool per call. On Windows every closed client socket sits in
//! TIME_WAIT for 2–4 minutes; the default ephemeral port range is ~16k.
//! A tight refresh loop drains the pool machine-wide, breaking not just
//! cdout but any other process reaching the same host (e.g. Ollama).
//!
//! RULE FOR THE WHOLE CRATE: never call `reqwest::Client::new()` or
//! `reqwest::Client::builder().build()` directly. Always go through
//! `shared_client()`. One `reqwest::Client` = one connection pool = keep-alive
//! reuse. Clones are cheap (`Arc<Inner>`).
//!
//! Pool tuning:
//! - `pool_max_idle_per_host = 8` — enough for parallel streaming without
//!   holding hundreds of idle sockets.
//! - `pool_idle_timeout = 90s` — long enough that a chatty UI reuses the
//!   same socket, short enough that a WiFi handoff doesn't leave zombies.
//! - `tcp_keepalive = 30s` — nudges Windows to keep ESTABLISHED sockets alive.
//! - No global timeout — streaming responses run for minutes; per-chunk
//!   idle is enforced by `llm::stream_util::next_chunk_with_timeout`.

use reqwest::Client;
use std::sync::OnceLock;
use std::time::Duration;

const POOL_MAX_IDLE_PER_HOST: usize = 8;
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
const TCP_KEEPALIVE: Duration = Duration::from_secs(30);
/// TCP-connect ceiling — separate from the (absent) global request timeout.
/// The old anthropic client used to have a 120s global timeout; removing it
/// was correct (streaming responses run for minutes), but a bad DNS entry or
/// a black-holed SYN-ACK would otherwise hang the retry wrapper for the
/// entire Windows SYN-retry window (~20s) before the retry loop can back off.
/// 10s is generous for LAN Ollama, tight enough to fail fast on bad routes.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

static CLIENT: OnceLock<Client> = OnceLock::new();

fn build_client() -> Client {
    match Client::builder()
        .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST)
        .pool_idle_timeout(POOL_IDLE_TIMEOUT)
        .tcp_keepalive(TCP_KEEPALIVE)
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            // Log the specific reqwest error before we panic. Without this a
            // TLS-backend init failure surfaces as a bare "shared reqwest
            // client should build" message with no clue about the actual
            // cause. Both targets use hyper-tls (schannel on Windows,
            // Secure Transport on macOS) so this is theoretical, but easy
            // hardening.
            eprintln!("[llm::http] FATAL: could not build shared reqwest client: {e:?}");
            panic!("shared reqwest client should build: {e}");
        }
    }
}

/// Get a cloneable handle to the shared HTTP client. Clones are cheap
/// (`Arc` under the hood); each provider function is free to `.clone()` if
/// it needs to move the handle into a closure.
pub fn shared_client() -> Client {
    CLIENT.get_or_init(build_client).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_client_returns_a_client() {
        // Confirms the builder configuration is compatible with the current
        // reqwest version — a future dependency bump that removes
        // `.tcp_keepalive` / `.pool_idle_timeout` would panic here.
        let _c = shared_client();
    }

    #[test]
    fn shared_client_is_a_singleton() {
        // Successive calls must go through the OnceLock — not a fresh
        // Client each time. The public API doesn't let us read the internal
        // Arc, so this test is more about "can we call it 1000× cheaply?"
        // If build_client fires per call, running this under a profiler
        // shows CPU. The functional check is just non-panic.
        for _ in 0..1000 {
            let _c = shared_client();
        }
    }
}
