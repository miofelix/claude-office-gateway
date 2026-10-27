use crate::{
    config::{GatewayConfig, UpstreamProtocol},
    protocol, stream,
};
use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
    Json, Router,
};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};

type Result<T> = std::result::Result<T, GatewayError>;
const RESPONSE_LIMIT: usize = 32 * 1024 * 1024;
#[derive(Debug, Error, Clone)]
#[error("{message}")]
pub struct GatewayError {
    pub status: u16,
    pub kind: String,
    pub message: String,
}
impl GatewayError {
    pub fn new(status: u16, kind: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            kind: kind.into(),
            message: message.into(),
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(400, "invalid_request_error", message)
    }
    pub fn upstream(message: impl Into<String>) -> Self {
        Self::new(502, "api_error", message)
    }
    pub fn payload(&self) -> Value {
        json!({"type":"error","error":{"type":self.kind,"message":self.message}})
    }
}
impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(self.payload()),
        )
            .into_response()
    }
}
fn network_error(error: reqwest::Error) -> GatewayError {
    if error.is_timeout() {
        GatewayError::new(
            504,
            "api_error",
            "The provider timed out. Check the network or increase the provider timeout.",
        )
    } else {
        GatewayError::upstream("Could not reach the provider or its response was interrupted. Check the API URL and network.")
    }
}

#[derive(Clone)]
pub struct GatewayRouter {
    config: GatewayConfig,
    token: String,
    key: String,
    client: Client,
    cache: Arc<Mutex<HashMap<String, (Instant, Value)>>>,
    concurrent: Arc<Semaphore>,
}
impl GatewayRouter {
    pub fn new(config: GatewayConfig, token: String, key: String) -> anyhow::Result<Self> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()?;
        Self::with_client(config, token, key, client)
    }
    pub fn with_client(
        config: GatewayConfig,
        token: String,
        key: String,
        client: Client,
    ) -> anyhow::Result<Self> {
        config.validate()?;
        crate::credentials::validate(&token)?;
        crate::credentials::validate(&key)?;
        Ok(Self {
            config,
            token,
            key,
            client,
            cache: Arc::new(Mutex::new(HashMap::new())),
            concurrent: Arc::new(Semaphore::new(32)),
        })
    }
    pub fn app(self) -> Router {
        Router::new()
            .fallback(any(handle))
            .with_state(Arc::new(self))
    }
    pub async fn verify_upstream(&self) -> Result<()> {
        for text in ["x", "."] {
            let body = json!({"model":self.config.public_model,"max_tokens":1,"messages":[{"role":"user","content":text}]});
            self.forward(body, &HeaderMap::new(), false, true).await?;
        }
        Ok(())
    }
    async fn forward(
        &self,
        body: Value,
        headers: &HeaderMap,
        count: bool,
        bypass_cache: bool,
    ) -> Result<Response> {
        if body["model"] != self.config.public_model {
            return Err(GatewayError::new(
                404,
                "not_found_error",
                "Unknown model; select an id from /v1/models",
            ));
        }
        if count && self.config.upstream_protocol != UpstreamProtocol::Anthropic {
            return Err(GatewayError::new(
                501,
                "api_error",
                "Exact token counting requires an Anthropic backend",
            ));
        }
        if !count {
            protocol::validate(&body)?;
        }
        let cache_key = if !count && self.config.probe_cache_seconds > 0 {
            probe_key(&body, headers)
        } else {
            None
        };
        if !bypass_cache {
            if let Some(key) = &cache_key {
                if let Some((created, reply)) = self.cache.lock().await.get(key) {
                    if created.elapsed() < Duration::from_secs(self.config.probe_cache_seconds) {
                        let mut response = Json(reply.clone()).into_response();
                        response.headers_mut().insert(
                            "x-office-gateway-probe-cache",
                            HeaderValue::from_static("hit"),
                        );
                        return Ok(response);
                    }
                }
            }
        }
        let (resource, mut outgoing) = match self.config.upstream_protocol {
            UpstreamProtocol::Anthropic => (
                if count {
                    "messages/count_tokens"
                } else {
                    "messages"
                },
                body.clone(),
            ),
            UpstreamProtocol::OpenaiChat => (
                "chat/completions",
                protocol::to_chat(
                    &body,
                    &self.config.upstream_model,
                    &self.config.max_tokens_field,
                )?,
            ),
            UpstreamProtocol::OpenaiResponses => (
                "responses",
                protocol::to_responses(&body, &self.config.upstream_model)?,
            ),
        };
        outgoing["model"] = json!(self.config.upstream_model);
        let streaming = !count && body["stream"] == true;
        let mut request = self
            .client
            .post(self.config.endpoint(resource))
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::ACCEPT,
                if streaming {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            );
        request = if self.config.auth_header == "x-api-key" {
            request.header("x-api-key", &self.key)
        } else {
            request.bearer_auth(&self.key)
        };
        if self.config.upstream_protocol == UpstreamProtocol::Anthropic {
            request = request.header(
                "anthropic-version",
                headers
                    .get("anthropic-version")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("2023-06-01"),
            );
            if let Some(beta) = headers.get("anthropic-beta") {
                request = request.header("anthropic-beta", beta);
            }
        }
        let upstream = request
            .json(&outgoing)
            .send()
            .await
            .map_err(network_error)?;
        let status = upstream.status().as_u16();
        if upstream.status().is_redirection() {
            return Err(GatewayError::upstream(
                "Upstream redirect refused; check API Base URL",
            ));
        }
        if !upstream.status().is_success() {
            let kind = match status {
                401 => "authentication_error",
                403 => "permission_error",
                429 => "rate_limit_error",
                _ => "api_error",
            };
            let detail = match status {
                401 => "Your provider rejected the API key. Check its authentication method.",
                403 => "Your provider denied access to this model or protocol.",
                429 => "Your provider is rate-limiting requests. Wait and recheck.",
                _ => "Your provider rejected the request.",
            };
            return Err(GatewayError::new(
                status,
                kind,
                format!("{detail} (HTTP {status})"),
            ));
        }
        let mut response_headers = HeaderMap::new();
        for name in ["request-id", "x-request-id", "retry-after"] {
            if let Some(value) = upstream.headers().get(name) {
                response_headers.insert(name, value.clone());
            }
        }
        if streaming {
            if !upstream
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|s| s.to_ascii_lowercase().contains("text/event-stream"))
            {
                return Err(GatewayError::upstream(
                    "Upstream did not return an SSE stream",
                ));
            }
            let kind = self.config.upstream_protocol.clone();
            let model = self.config.public_model.clone();
            let output = async_stream::try_stream! {
                let mut decoder=stream::Decoder::new();
                let mut adapter=stream::Adapter::new(kind,&model);
                for chunk in adapter.start() { yield chunk; }
                let mut source=upstream.bytes_stream();
                while let Some(chunk)=source.next().await {
                    let chunk=chunk.map_err(network_error)?;
                    for (name,data) in decoder.push(&chunk)? {
                        for output in adapter.feed(&name,&data)? { yield output; }
                    }
                    if adapter.done { break; }
                }
                if !adapter.done {
                    for (name,data) in decoder.finish()? {
                        for output in adapter.feed(&name,&data)? { yield output; }
                    }
                }
                adapter.eof()?;
            };
            let output = output.map(|result: Result<Bytes>| {
                Ok::<Bytes, std::io::Error>(match result {
                    Ok(bytes) => bytes,
                    Err(error) => stream::event("error", &error.payload()),
                })
            });
            let mut response = Response::new(Body::from_stream(output));
            response.headers_mut().extend(response_headers);
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream; charset=utf-8"),
            );
            response
                .headers_mut()
                .insert("x-accel-buffering", HeaderValue::from_static("no"));
            return Ok(response);
        }
        let mut source = upstream.bytes_stream();
        let mut payload = Vec::new();
        while let Some(chunk) = source.next().await {
            let chunk = chunk.map_err(network_error)?;
            if payload.len() + chunk.len() > RESPONSE_LIMIT {
                return Err(GatewayError::upstream("Upstream response exceeds 32 MiB"));
            }
            payload.extend_from_slice(&chunk);
        }
        let mut reply: Value = serde_json::from_slice(&payload)
            .map_err(|_| GatewayError::upstream("The provider returned invalid JSON"))?;
        if !count {
            reply = match self.config.upstream_protocol {
                UpstreamProtocol::Anthropic => {
                    if reply["type"] != "message" || !reply["content"].is_array() {
                        return Err(GatewayError::upstream("Invalid upstream Anthropic message"));
                    }
                    reply["model"] = json!(self.config.public_model);
                    reply
                }
                UpstreamProtocol::OpenaiChat => {
                    protocol::from_chat(&reply, &self.config.public_model)?
                }
                UpstreamProtocol::OpenaiResponses => {
                    protocol::from_responses(&reply, &self.config.public_model)?
                }
            };
        }
        if let Some(key) = cache_key {
            let mut cache = self.cache.lock().await;
            if cache.len() >= 4 {
                cache.clear();
            }
            cache.insert(key, (Instant::now(), reply.clone()));
        }
        let mut response = Json(reply).into_response();
        response.headers_mut().extend(response_headers);
        Ok(response)
    }
}

async fn handle(State(router): State<Arc<GatewayRouter>>, request: Request) -> Response {
    let headers = request.headers().clone();
    let result=async {
        validate_local(&router.config,request.uri(),&headers)?;
        if request.method()==Method::OPTIONS {return preflight(&headers);}
        if request.method()==Method::GET && request.uri().path()=="/health" {return Ok(Json(json!({"status":"ok","version":env!("CARGO_PKG_VERSION"),"runtime":"native-rust"})).into_response());}
        authorize(&router.token,&headers)?;
        let _permit=router.concurrent.clone().try_acquire_owned().map_err(|_|GatewayError::new(429,"rate_limit_error","Too many concurrent gateway requests"))?;
        match (request.method().as_str(),request.uri().path()) {
            ("GET","/v1/models")=>{let model=model_value(&router.config);Ok(Json(json!({"data":[model],"has_more":false,"first_id":router.config.public_model,"last_id":router.config.public_model})).into_response())},
            ("GET",path) if path==format!("/v1/models/{}",router.config.public_model)=>Ok(Json(model_value(&router.config)).into_response()),
            ("POST","/v1/messages"|"/v1/messages/count_tokens")=>{
                let count=request.uri().path().ends_with("/count_tokens");
                let body=tokio::time::timeout(Duration::from_secs(30),to_bytes(request.into_body(),router.config.max_request_bytes)).await.map_err(|_|GatewayError::new(408,"invalid_request_error","Request body timed out"))?.map_err(|_|GatewayError::new(413,"request_too_large","Request body exceeds configured limit"))?;
                let value=serde_json::from_slice(&body).map_err(|_|GatewayError::bad_request("Request body must be a JSON object"))?;
                router.forward(value,&headers,count,false).await
            }
            _=>Err(GatewayError::new(404,"not_found_error","Unknown route or model")),
        }
    }.await;
    let mut response = match result {
        Ok(response) => response,
        Err(error) => error.into_response(),
    };
    if let Some(origin) = headers.get(header::ORIGIN).filter(|v| {
        v.to_str()
            .ok()
            .is_some_and(|s| router.config.allowed_origins.iter().any(|o| o == s))
    }) {
        response
            .headers_mut()
            .insert("access-control-allow-origin", origin.clone());
        response
            .headers_mut()
            .insert("vary", HeaderValue::from_static("Origin"));
        response.headers_mut().insert(
            "access-control-expose-headers",
            HeaderValue::from_static(
                "request-id, x-request-id, retry-after, x-office-gateway-probe-cache",
            ),
        );
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}
fn model_value(config: &GatewayConfig) -> Value {
    json!({"id":config.public_model,"type":"model","display_name":config.display_name,"created_at":"2026-10-26T00:00:00Z"})
}
fn validate_local(config: &GatewayConfig, uri: &Uri, headers: &HeaderMap) -> Result<()> {
    // HTTP/2 carries :authority in the URI; Hyper does not synthesize Host.
    // Check both when supplied so an absolute URI cannot bypass Host validation.
    let authority = uri
        .authority()
        .map(|value| value.as_str().to_ascii_lowercase());
    let host = headers
        .get(header::HOST)
        .map(|value| value.to_str().unwrap_or("").to_ascii_lowercase());
    let allowed = [
        format!("localhost:{}", config.port),
        format!("127.0.0.1:{}", config.port),
        format!("[::1]:{}", config.port),
    ];
    if headers.get_all(header::HOST).iter().count() > 1
        || (authority.is_none() && host.is_none())
        || authority
            .iter()
            .chain(host.iter())
            .any(|value| !allowed.contains(value))
        || matches!((&authority, &host), (Some(a), Some(h)) if a != h)
    {
        return Err(GatewayError::new(
            403,
            "permission_error",
            "Host is not allowed",
        ));
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        if !origin
            .to_str()
            .ok()
            .is_some_and(|s| config.allowed_origins.iter().any(|o| o == s))
        {
            return Err(GatewayError::new(
                403,
                "permission_error",
                "Origin is not allowed",
            ));
        }
    }
    Ok(())
}
fn authorize(token: &str, headers: &HeaderMap) -> Result<()> {
    if headers.get_all("x-api-key").iter().count() > 1
        || headers.get_all(header::AUTHORIZATION).iter().count() > 1
    {
        return Err(GatewayError::bad_request("Duplicate authentication header"));
    }
    let key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let bearer = auth
        .split_once(' ')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, value)| value.trim())
        .unwrap_or("");
    if !key.is_empty() && !bearer.is_empty() && !constant_equal(key, bearer) {
        return Err(GatewayError::new(
            401,
            "authentication_error",
            "Conflicting authentication headers",
        ));
    }
    if !constant_equal(if key.is_empty() { bearer } else { key }, token) {
        return Err(GatewayError::new(
            401,
            "authentication_error",
            "Invalid gateway API key",
        ));
    }
    Ok(())
}
fn constant_equal(a: &str, b: &str) -> bool {
    Sha256::digest(a.as_bytes())
        .iter()
        .zip(Sha256::digest(b.as_bytes()).iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}
fn allowed_headers() -> Vec<String> {
    let mut names = [
        "accept",
        "authorization",
        "content-type",
        "cache-control",
        "x-api-key",
        "user-agent",
        "anthropic-version",
        "anthropic-beta",
        "anthropic-dangerous-direct-browser-access",
        "x-client-request-id",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();
    names.extend(
        [
            "lang",
            "package-version",
            "os",
            "arch",
            "retry-count",
            "runtime",
            "runtime-version",
            "timeout",
            "async",
            "helper-method",
            "poll-helper",
            "custom-poll-interval",
        ]
        .iter()
        .map(|s| format!("x-stainless-{s}")),
    );
    names
}
fn preflight(headers: &HeaderMap) -> Result<Response> {
    if !headers
        .get("access-control-request-method")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| matches!(v, "GET" | "POST" | "OPTIONS"))
    {
        return Err(GatewayError::bad_request("CORS method is not allowed"));
    }
    let allowed = allowed_headers();
    let requested = headers
        .get("access-control-request-headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if requested
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .any(|s| !allowed.contains(&s))
    {
        return Err(GatewayError::bad_request("CORS header is not allowed"));
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    response.headers_mut().insert(
        "access-control-allow-headers",
        HeaderValue::from_str(&allowed.join(", ")).unwrap(),
    );
    response
        .headers_mut()
        .insert("access-control-max-age", HeaderValue::from_static("600"));
    Ok(response)
}
fn probe_key(body: &Value, headers: &HeaderMap) -> Option<String> {
    let object = body.as_object()?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "model" | "messages" | "max_tokens" | "stream"))
        || body["max_tokens"] != 1
        || body["stream"] == true
    {
        return None;
    }
    let messages = body["messages"].as_array()?;
    if messages.len() != 1
        || messages[0].as_object()?.len() != 2
        || messages[0]["role"] != "user"
        || !matches!(messages[0]["content"].as_str(), Some("x" | "."))
    {
        return None;
    }
    Some(format!(
        "{}:{}:{}",
        messages[0]["content"],
        headers
            .get("anthropic-version")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("2023-06-01"),
        headers
            .get("anthropic-beta")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    ))
}
