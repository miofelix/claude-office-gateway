use crate::gateway::GatewayError;
use reqwest::{RequestBuilder, Response};
use std::{
    error::Error,
    io,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(250);
pub const REVISION: &str = "2026-10-28.1";

#[derive(Clone)]
pub struct Trace {
    pub id: String,
    started: Instant,
    attempt: Arc<AtomicU8>,
}
impl Trace {
    pub fn new() -> Self {
        Self {
            id: format!("og_{}", uuid::Uuid::new_v4().simple()),
            started: Instant::now(),
            attempt: Arc::new(AtomicU8::new(0)),
        }
    }
    fn record(&self, failure: &Failure, stage: &str, attempt: u8, retry: bool) {
        // All values are generated identifiers, booleans, numbers or enum labels.
        // Never log Error's Display/Debug: even its source can contain secrets.
        log::warn!(
            "Provider transport failed: request_id={}, stage={stage}, attempt={attempt}, elapsed_ms={}, category={}, connect={}, timeout={}, io_kind={:?}, os_code={:?}, retry={retry}",
            self.id, self.started.elapsed().as_millis(), failure.category,
            failure.connect, failure.timeout, failure.io_kind, failure.os_code,
        );
    }
    pub fn body_error(&self, error: reqwest::Error, stage: &str) -> GatewayError {
        let failure = Failure::classify(&error);
        self.record(&failure, stage, self.attempt.load(Ordering::Relaxed), false);
        failure.gateway_error().with_request_id(&self.id)
    }
    pub async fn send(
        &self,
        mut request: RequestBuilder,
        total_timeout: Option<Duration>,
    ) -> Result<Response, GatewayError> {
        let deadline = total_timeout.map(|duration| Instant::now() + duration);
        for attempt in 1..=2 {
            self.attempt.store(attempt, Ordering::Relaxed);
            if let Some(deadline) = deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(Failure::timeout_error().with_request_id(&self.id));
                }
                // The timer follows the response body too, so JSON deadlines
                // include both connection attempts and the complete response.
                request = request.timeout(remaining);
            }
            let retry_request = if attempt == 1 {
                request.try_clone()
            } else {
                None
            };
            match request.send().await {
                Ok(response) => {
                    if attempt > 1 {
                        log::info!("Provider connection recovered: request_id={}, attempt={attempt}, elapsed_ms={}", self.id, self.started.elapsed().as_millis());
                    }
                    return Ok(response);
                }
                Err(error) => {
                    let failure = Failure::classify(&error);
                    let retry = attempt == 1
                        && retry_request.is_some()
                        && failure.retryable_connect()
                        && deadline.is_none_or(|end| {
                            end.saturating_duration_since(Instant::now()) > CONNECT_RETRY_DELAY
                        });
                    self.record(&failure, "response_headers", attempt, retry);
                    if !retry {
                        return Err(failure.gateway_error().with_request_id(&self.id));
                    }
                    tokio::time::sleep(CONNECT_RETRY_DELAY).await;
                    request = retry_request.unwrap();
                }
            }
        }
        unreachable!("Each attempt returns or installs the next request")
    }
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub category: &'static str,
    pub connect: bool,
    pub timeout: bool,
    io_kind: Option<io::ErrorKind>,
    os_code: Option<i32>,
}
impl Failure {
    pub fn classify(error: &reqwest::Error) -> Self {
        // Skip reqwest's outer Display, which embeds the request URL. Hostnames
        // containing words such as "proxy" must not change the classification.
        let mut source: Option<&(dyn Error + 'static)> = error.source();
        let mut io_kind = None;
        let mut os_code = None;
        let (mut tls, mut proxy, mut dns) = (false, false, false);
        for _ in 0..16 {
            let Some(current) = source else {
                break;
            };
            if let Some(io) = current.downcast_ref::<io::Error>() {
                io_kind = Some(io.kind());
                os_code = io.raw_os_error().or(os_code);
            }
            // Rustls is typed; resolver/proxy wrappers do not expose a public
            // error type. Inspect their text only to select fixed labels.
            tls |= current.is::<rustls::Error>();
            let text = current.to_string().to_ascii_lowercase();
            tls |= text.contains("certificate") || text.contains("tls handshake");
            proxy |= text.contains("proxy") || text.contains("tunnel");
            dns |= text.contains("dns error")
                || text.contains("failed to lookup")
                || text.contains("name or service not known")
                || text.contains("name resolution");
            source = current.source();
        }
        let timeout = error.is_timeout();
        let connect = error.is_connect();
        let category = if timeout {
            "timeout"
        } else if tls {
            "tls"
        } else if proxy {
            "proxy"
        } else if dns {
            "dns"
        } else {
            match io_kind {
                Some(io::ErrorKind::ConnectionRefused) => "connection_refused",
                Some(io::ErrorKind::ConnectionReset) => "connection_reset",
                Some(io::ErrorKind::ConnectionAborted) => "connection_aborted",
                Some(io::ErrorKind::UnexpectedEof) => "unexpected_eof",
                Some(io::ErrorKind::BrokenPipe) => "broken_pipe",
                Some(io::ErrorKind::NetworkUnreachable) => "network_unreachable",
                Some(io::ErrorKind::HostUnreachable) => "host_unreachable",
                _ if error.is_body() || error.is_decode() => "response_body",
                _ if connect => "connect",
                _ => "request",
            }
        };
        Self {
            category,
            connect,
            timeout,
            io_kind,
            os_code,
        }
    }
    fn retryable_connect(&self) -> bool {
        // Hyper's is_connect marks errors from connection setup, before sending
        // the provider HTTP request. Never replay an ambiguous POST or a stream.
        self.connect
            && !self.timeout
            && !matches!(self.category, "tls" | "proxy")
            && (self.category == "dns"
                || matches!(
                    self.io_kind,
                    Some(
                        io::ErrorKind::ConnectionRefused
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::NotConnected
                            | io::ErrorKind::UnexpectedEof
                            | io::ErrorKind::BrokenPipe
                            | io::ErrorKind::NetworkUnreachable
                            | io::ErrorKind::HostUnreachable
                            | io::ErrorKind::Interrupted
                            | io::ErrorKind::WouldBlock
                    )
                ))
    }
    fn timeout_error() -> GatewayError {
        GatewayError::new(
            504,
            "api_error",
            "The provider timed out. Check the network or increase the provider timeout.",
        )
    }
    fn gateway_error(&self) -> GatewayError {
        if self.timeout {
            return Self::timeout_error();
        }
        GatewayError::upstream(match self.category {
            "dns" => "Could not resolve the provider hostname. Check the API URL, DNS, and network.",
            "tls" => "The provider TLS connection failed. Check the API URL and certificate chain.",
            "proxy" => "The proxy connection to the provider failed. Check the proxy configuration and network.",
            "connection_refused" => "The provider connection was refused. Check the API URL and network.",
            _ => "Could not reach the provider or its response was interrupted. Check the API URL and network.",
        })
    }
}
