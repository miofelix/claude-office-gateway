use crate::config::{GatewayConfig, UpstreamProtocol};
use axum::{
    body::Bytes,
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};
use reqwest::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("{message}")]
    Http {
        status: u16,
        kind: String,
        message: String,
    },
}

impl GatewayError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self::Http {
            status: 400,
            kind: "invalid_request_error".into(),
            message: message.into(),
        }
    }
    fn upstream(message: impl Into<String>) -> Self {
        Self::Http {
            status: 502,
            kind: "api_error".into(),
            message: message.into(),
        }
    }
    fn status(&self) -> StatusCode {
        match self {
            Self::Http { status, .. } => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = match self {
            Self::Http { kind, message, .. } => {
                json!({"type": "error", "error": {"type": kind, "message": message}})
            }
        };
        (status, Json(body)).into_response()
    }
}

#[derive(Clone)]
pub struct GatewayRouter {
    config: GatewayConfig,
    gateway_token: String,
    upstream_key: String,
    client: Client,
}

impl GatewayRouter {
    pub fn new(
        config: GatewayConfig,
        gateway_token: String,
        upstream_key: String,
    ) -> anyhow::Result<Self> {
        config.validate()?;
        crate::credentials::validate(&gateway_token)?;
        crate::credentials::validate(&upstream_key)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()?;
        Ok(Self {
            config,
            gateway_token,
            upstream_key,
            client,
        })
    }

    pub fn app(self) -> Router {
        let state = Arc::new(self);
        Router::new()
            .route("/health", get(health))
            .route("/v1/models", get(models))
            .route("/v1/messages", post(messages))
            .route("/v1/messages/count_tokens", post(count_tokens))
            .fallback(any(fallback))
            .with_state(state)
    }

    pub async fn verify_upstream(&self) -> Result<(), GatewayError> {
        for text in ["x", "."] {
            let body = json!({
                "model": self.config.public_model,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": text}]
            });
            let response = self.forward_value(&body, &HeaderMap::new(), false).await?;
            if !response.status().is_success() {
                return Err(GatewayError::upstream(format!(
                    "The provider check failed (HTTP {}).",
                    response.status()
                )));
            }
        }
        Ok(())
    }

    async fn forward_value(
        &self,
        body: &Value,
        incoming: &HeaderMap,
        count_tokens: bool,
    ) -> Result<reqwest::Response, GatewayError> {
        let endpoint = if count_tokens {
            self.config.endpoint("messages/count_tokens")
        } else {
            self.config.endpoint("messages")
        };
        let mut request = self
            .client
            .post(endpoint)
            .header(header::CONTENT_TYPE, "application/json");
        if self.config.auth_header == "x-api-key" {
            request = request.header("x-api-key", &self.upstream_key);
        } else {
            request = request.bearer_auth(&self.upstream_key);
        }
        if matches!(self.config.upstream_protocol, UpstreamProtocol::Anthropic) {
            request = request.header(
                "anthropic-version",
                incoming
                    .get("anthropic-version")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("2023-06-01"),
            );
            if let Some(beta) = incoming.get("anthropic-beta") {
                request = request.header("anthropic-beta", beta);
            }
        }
        let outgoing = if count_tokens
            || matches!(self.config.upstream_protocol, UpstreamProtocol::Anthropic)
        {
            body.clone()
        } else {
            self.to_upstream(body)?
        };
        request.json(&outgoing).send().await.map_err(|error| {
            GatewayError::upstream(format!("Could not reach the provider: {error}"))
        })
    }

    fn to_upstream(&self, body: &Value) -> Result<Value, GatewayError> {
        match self.config.upstream_protocol {
            UpstreamProtocol::OpenaiChat => to_chat(
                body,
                &self.config.upstream_model,
                &self.config.max_tokens_field,
            ),
            UpstreamProtocol::OpenaiResponses => to_responses(body, &self.config.upstream_model),
            UpstreamProtocol::Anthropic => Ok(body.clone()),
        }
    }
}

async fn health(State(router): State<Arc<GatewayRouter>>, headers: HeaderMap) -> Response {
    let body =
        json!({"status": "ok", "runtime": "native-rust", "model": router.config.public_model});
    with_cors(
        (StatusCode::OK, Json(body)).into_response(),
        &router.config,
        &headers,
    )
}

async fn models(
    State(router): State<Arc<GatewayRouter>>,
    headers: HeaderMap,
) -> Result<Response, GatewayError> {
    authorize(&router, &headers).await?;
    let body = json!({
        "data": [{
            "id": router.config.public_model,
            "type": "model",
            "display_name": router.config.display_name,
            "created_at": "2026-10-26T00:00:00Z"
        }],
        "has_more": false,
        "first_id": router.config.public_model,
        "last_id": router.config.public_model
    });
    Ok(with_cors(
        (StatusCode::OK, Json(body)).into_response(),
        &router.config,
        &headers,
    ))
}

async fn messages(
    State(router): State<Arc<GatewayRouter>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, GatewayError> {
    authorize(&router, &headers).await?;
    let input: Value = serde_json::from_slice(&body)
        .map_err(|_| GatewayError::bad_request("Request body must be valid JSON."))?;
    validate_anthropic(&input)?;
    let wants_stream = input
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let response = router.forward_value(&input, &headers, false).await?;
    let status = response.status();
    let payload = response
        .bytes()
        .await
        .map_err(|error| GatewayError::upstream(error.to_string()))?;
    if !status.is_success() {
        let mut output = Response::new(axum::body::Body::from(payload));
        *output.status_mut() = status;
        return Ok(with_cors(output, &router.config, &headers));
    }
    if matches!(router.config.upstream_protocol, UpstreamProtocol::Anthropic) {
        let mut output = Response::new(axum::body::Body::from(payload));
        output.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static(if wants_stream {
                "text/event-stream"
            } else {
                "application/json"
            }),
        );
        return Ok(with_cors(output, &router.config, &headers));
    }
    let upstream: Value = serde_json::from_slice(&payload)
        .map_err(|_| GatewayError::upstream("The provider returned invalid JSON."))?;
    let converted = match router.config.upstream_protocol {
        UpstreamProtocol::OpenaiChat => from_chat(&upstream, &router.config.public_model)?,
        UpstreamProtocol::OpenaiResponses => {
            from_responses(&upstream, &router.config.public_model)?
        }
        UpstreamProtocol::Anthropic => unreachable!(),
    };
    let output = if wants_stream {
        synthetic_stream(&converted)
    } else {
        (StatusCode::OK, Json(converted)).into_response()
    };
    Ok(with_cors(output, &router.config, &headers))
}

async fn count_tokens(
    State(router): State<Arc<GatewayRouter>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, GatewayError> {
    authorize(&router, &headers).await?;
    if !matches!(router.config.upstream_protocol, UpstreamProtocol::Anthropic) {
        return Err(GatewayError::Http {
            status: 501,
            kind: "api_error".into(),
            message: "Exact token counting requires an Anthropic backend.".into(),
        });
    }
    let input: Value = serde_json::from_slice(&body)
        .map_err(|_| GatewayError::bad_request("Request body must be valid JSON."))?;
    let upstream = router.forward_value(&input, &headers, true).await?;
    let status = upstream.status();
    let payload = upstream
        .bytes()
        .await
        .map_err(|error| GatewayError::upstream(error.to_string()))?;
    let mut output = Response::new(axum::body::Body::from(payload));
    *output.status_mut() = status;
    Ok(with_cors(output, &router.config, &headers))
}

async fn fallback(
    State(router): State<Arc<GatewayRouter>>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response, GatewayError> {
    if request.method() == Method::OPTIONS {
        return Ok(with_cors(
            (StatusCode::NO_CONTENT, "").into_response(),
            &router.config,
            &headers,
        ));
    }
    Err(GatewayError::Http {
        status: 404,
        kind: "not_found_error".into(),
        message: "Unknown route or model".into(),
    })
}

async fn authorize(router: &GatewayRouter, headers: &HeaderMap) -> Result<(), GatewayError> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let expected = format!("localhost:{}", router.config.port);
    let expected_ip = format!("127.0.0.1:{}", router.config.port);
    if host != expected && host != expected_ip {
        return Err(GatewayError::Http {
            status: 403,
            kind: "permission_error".into(),
            message: "Host is not allowed".into(),
        });
    }
    if let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        if !router
            .config
            .allowed_origins
            .iter()
            .any(|allowed| allowed == origin)
        {
            return Err(GatewayError::Http {
                status: 403,
                kind: "permission_error".into(),
                message: "Origin is not allowed".into(),
            });
        }
    }
    let api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let bearer = authorization
        .strip_prefix("Bearer ")
        .or_else(|| authorization.strip_prefix("bearer "))
        .unwrap_or("")
        .trim();
    if !api_key.is_empty() && !bearer.is_empty() && !constant_equal(api_key, bearer) {
        return Err(GatewayError::Http {
            status: 401,
            kind: "authentication_error".into(),
            message: "Conflicting authentication headers".into(),
        });
    }
    if !constant_equal(
        if api_key.is_empty() { bearer } else { api_key },
        &router.gateway_token,
    ) {
        return Err(GatewayError::Http {
            status: 401,
            kind: "authentication_error".into(),
            message: "Invalid gateway API key".into(),
        });
    }
    Ok(())
}

fn constant_equal(left: &str, right: &str) -> bool {
    let left = Sha256::digest(left.as_bytes());
    let right = Sha256::digest(right.as_bytes());
    left.iter()
        .zip(right.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn with_cors(mut response: Response, config: &GatewayConfig, headers: &HeaderMap) -> Response {
    if let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        if config
            .allowed_origins
            .iter()
            .any(|allowed| allowed == origin)
        {
            if let Ok(value) = HeaderValue::from_str(origin) {
                response
                    .headers_mut()
                    .insert("access-control-allow-origin", value);
            }
            response
                .headers_mut()
                .insert("vary", HeaderValue::from_static("Origin"));
            response.headers_mut().insert("access-control-allow-headers", HeaderValue::from_static("accept, authorization, content-type, x-api-key, anthropic-version, anthropic-beta"));
            response.headers_mut().insert(
                "access-control-allow-methods",
                HeaderValue::from_static("GET, POST, OPTIONS"),
            );
        }
    }
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn validate_anthropic(body: &Value) -> Result<(), GatewayError> {
    if !body
        .get("messages")
        .and_then(Value::as_array)
        .is_some_and(|value| !value.is_empty())
    {
        return Err(GatewayError::bad_request("messages must be nonempty"));
    }
    if !body
        .get("max_tokens")
        .and_then(Value::as_u64)
        .is_some_and(|value| value > 0)
    {
        return Err(GatewayError::bad_request(
            "max_tokens must be a positive integer",
        ));
    }
    Ok(())
}

fn text_content(value: &Value) -> Result<String, GatewayError> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => Ok(parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")),
        _ => Err(GatewayError::bad_request(
            "Content must be text or an array of content blocks",
        )),
    }
}

fn tool_result_content(block: &Value) -> Result<Value, GatewayError> {
    let content = block
        .get("content")
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()));
    let text = text_content(&content)?;
    let prefix = if block
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "[Tool error]\n"
    } else {
        ""
    };
    Ok(Value::String(format!("{prefix}{text}")))
}

fn to_chat(body: &Value, model: &str, token_field: &str) -> Result<Value, GatewayError> {
    validate_anthropic(body)?;
    let mut messages = Vec::new();
    if let Some(system) = body.get("system") {
        messages.push(json!({"role": "system", "content": text_content(system)?}));
    }
    for message in body["messages"].as_array().into_iter().flatten() {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| GatewayError::bad_request("Every message must have a role"))?;
        let blocks = message
            .get("content")
            .cloned()
            .unwrap_or(Value::String(String::new()));
        let parts = match blocks {
            Value::String(text) => vec![json!({"type": "text", "text": text})],
            Value::Array(parts) => parts,
            _ => {
                return Err(GatewayError::bad_request(
                    "Content must be text or an array of content blocks",
                ))
            }
        };
        if role == "assistant" {
            let mut texts = Vec::new();
            let mut calls = Vec::new();
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => texts.push(part.get("text").and_then(Value::as_str).unwrap_or_default().to_string()),
                    Some("tool_use") => calls.push(json!({"id": part["id"], "type": "function", "function": {"name": part["name"], "arguments": serde_json::to_string(part.get("input").unwrap_or(&json!({}))).unwrap_or_else(|_| "{}".into())}})),
                    Some("thinking") | Some("redacted_thinking") => {}
                    _ => return Err(GatewayError::bad_request("Unsupported assistant content block")),
                }
            }
            let mut item = json!({"role": "assistant", "content": if texts.is_empty() { Value::Null } else { Value::String(texts.join("\n")) }});
            if !calls.is_empty() {
                item["tool_calls"] = Value::Array(calls);
            }
            messages.push(item);
        } else {
            let mut text_parts = Vec::new();
            let mut images = Vec::new();
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => text_parts.push(json!({"type": "text", "text": part.get("text").and_then(Value::as_str).unwrap_or_default()})),
                    Some("image") => {
                        let source = &part["source"];
                        if source["type"] == "base64" {
                            let media = source["media_type"].as_str().unwrap_or("image/png");
                            let data = source["data"].as_str().unwrap_or_default();
                            images.push(json!({"type": "image_url", "image_url": {"url": format!("data:{media};base64,{data}")}}));
                        } else if source["type"] == "url" {
                            images.push(json!({"type": "image_url", "image_url": {"url": source["url"]}}));
                        }
                    }
                    Some("tool_result") => messages.push(json!({"role": "tool", "tool_call_id": part["tool_use_id"], "content": tool_result_content(&part)?})),
                    _ => return Err(GatewayError::bad_request("Unsupported user content block")),
                }
            }
            if !text_parts.is_empty() || !images.is_empty() {
                let mut content = text_parts;
                content.extend(images);
                messages.push(json!({"role": "user", "content": content}));
            }
        }
    }
    let mut output = json!({"model": model, "messages": messages, token_field: body["max_tokens"], "stream": false});
    for field in ["temperature", "top_p"] {
        if let Some(value) = body.get(field) {
            output[field] = value.clone();
        }
    }
    if let Some(stop) = body.get("stop_sequences") {
        output["stop"] = stop.clone();
    }
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        output["tools"] = Value::Array(tools.iter().map(|tool| json!({"type": "function", "function": {"name": tool["name"], "description": tool.get("description").and_then(Value::as_str).unwrap_or_default(), "parameters": tool["input_schema"]}})).collect());
    }
    if let Some(choice) = body.get("tool_choice") {
        if choice["type"] == "tool" {
            output["tool_choice"] =
                json!({"type": "function", "function": {"name": choice["name"]}});
        } else if let Some(kind) = choice["type"].as_str() {
            output["tool_choice"] = Value::String(if kind == "any" {
                "required".into()
            } else {
                kind.into()
            });
        }
    }
    Ok(output)
}

fn to_responses(body: &Value, model: &str) -> Result<Value, GatewayError> {
    let chat = to_chat(body, model, "max_completion_tokens")?;
    let mut input = Vec::new();
    for message in chat["messages"].as_array().into_iter().flatten() {
        match message["role"].as_str() {
            Some("tool") => input.push(json!({"type": "function_call_output", "call_id": message["tool_call_id"], "output": message["content"]})),
            Some(role) => {
                if !message["content"].is_null() {
                    input.push(json!({"role": role, "content": [{"type": if role == "assistant" { "output_text" } else { "input_text" }, "text": text_content(&message["content"])?}]}));
                }
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    input.push(json!({"type": "function_call", "call_id": call["id"], "name": call["function"]["name"], "arguments": call["function"]["arguments"]}));
                }
            }
            None => {}
        }
    }
    let mut output = json!({"model": model, "input": input, "max_output_tokens": body["max_tokens"], "stream": false, "store": false});
    for field in ["temperature", "top_p"] {
        if let Some(value) = chat.get(field) {
            output[field] = value.clone();
        }
    }
    if let Some(tools) = chat["tools"].as_array() {
        output["tools"] = Value::Array(
            tools
                .iter()
                .map(|tool| {
                    let mut function = tool["function"].clone();
                    function["type"] = json!("function");
                    function["strict"] = json!(false);
                    function
                })
                .collect(),
        );
    }
    Ok(output)
}

fn from_chat(response: &Value, model: &str) -> Result<Value, GatewayError> {
    let choice = response["choices"]
        .as_array()
        .and_then(|values| values.first())
        .ok_or_else(|| GatewayError::upstream("Upstream completion did not finish successfully"))?;
    let finish = choice["finish_reason"].as_str().unwrap_or("stop");
    let message = &choice["message"];
    let mut content = Vec::new();
    if let Some(text) = message["content"].as_str() {
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
    }
    if let Some(text) = message["refusal"].as_str() {
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
    }
    for call in message["tool_calls"].as_array().into_iter().flatten() {
        let arguments = call["function"]["arguments"].as_str().unwrap_or("{}");
        let input: Value = serde_json::from_str(arguments)
            .map_err(|_| GatewayError::upstream("Upstream tool arguments must be an object"))?;
        if !input.is_object() {
            return Err(GatewayError::upstream(
                "Upstream tool arguments must be an object",
            ));
        }
        content.push(json!({"type": "tool_use", "id": call["id"], "name": call["function"]["name"], "input": input}));
    }
    Ok(message_value(
        model,
        content,
        if finish == "length" {
            "max_tokens"
        } else if finish == "tool_calls" {
            "tool_use"
        } else {
            "end_turn"
        },
        usage(&response["usage"], true),
    ))
}

fn from_responses(response: &Value, model: &str) -> Result<Value, GatewayError> {
    let status = response["status"].as_str().unwrap_or_default();
    if status != "completed"
        && !(status == "incomplete"
            && response["incomplete_details"]["reason"] == "max_output_tokens")
    {
        return Err(GatewayError::upstream(
            "Upstream Responses request did not complete successfully",
        ));
    }
    let mut content = Vec::new();
    for item in response["output"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("function_call") => {
                let input: Value = serde_json::from_str(item["arguments"].as_str().unwrap_or("{}"))
                    .map_err(|_| {
                        GatewayError::upstream("Upstream tool arguments must be an object")
                    })?;
                content.push(json!({"type": "tool_use", "id": item["call_id"], "name": item["name"], "input": input}));
            }
            Some("message") => {
                for part in item["content"].as_array().into_iter().flatten() {
                    if let Some(text) = part["text"].as_str() {
                        content.push(json!({"type": "text", "text": text}));
                    }
                }
            }
            Some("reasoning") | None => {}
            _ => {}
        }
    }
    Ok(message_value(
        model,
        content,
        if status == "incomplete" {
            "max_tokens"
        } else {
            "end_turn"
        },
        usage(&response["usage"], false),
    ))
}

fn usage(value: &Value, chat: bool) -> Value {
    json!({
        "input_tokens": value[if chat { "prompt_tokens" } else { "input_tokens" }].as_u64().unwrap_or(0),
        "output_tokens": value[if chat { "completion_tokens" } else { "output_tokens" }].as_u64().unwrap_or(0)
    })
}

fn message_value(model: &str, content: Vec<Value>, reason: &str, usage: Value) -> Value {
    json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": reason,
        "stop_sequence": null,
        "usage": usage
    })
}

fn synthetic_stream(message: &Value) -> Response {
    let mut events = String::new();
    events.push_str(&format!(
        "event: message_start\ndata: {}\n\n",
        json!({"type": "message_start", "message": message})
    ));
    for (index, block) in message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if block["type"] == "text" {
            events.push_str(&format!("event: content_block_start\ndata: {}\n\n", json!({"type": "content_block_start", "index": index, "content_block": {"type": "text", "text": ""}})));
            events.push_str(&format!("event: content_block_delta\ndata: {}\n\n", json!({"type": "content_block_delta", "index": index, "delta": {"type": "text_delta", "text": block["text"]}})));
            events.push_str(&format!(
                "event: content_block_stop\ndata: {}\n\n",
                json!({"type": "content_block_stop", "index": index})
            ));
        } else {
            events.push_str(&format!(
                "event: content_block_start\ndata: {}\n\n",
                json!({"type": "content_block_start", "index": index, "content_block": block})
            ));
            events.push_str(&format!(
                "event: content_block_stop\ndata: {}\n\n",
                json!({"type": "content_block_stop", "index": index})
            ));
        }
    }
    events.push_str(&format!("event: message_delta\ndata: {}\n\n", json!({"type": "message_delta", "delta": {"stop_reason": message["stop_reason"], "stop_sequence": null}, "usage": message["usage"]})));
    events.push_str("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
    let mut response = Response::new(axum::body::Body::from(events));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-cache"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Value {
        json!({
            "model": "claude-office-custom",
            "max_tokens": 128,
            "system": [{"type": "text", "text": "Be concise."}],
            "messages": [{"role": "user", "content": "Hello"}],
            "tools": [{"name": "read_slide", "description": "Read a slide", "input_schema": {"type": "object"}}]
        })
    }

    #[test]
    fn converts_anthropic_messages_to_chat_completions() {
        let output = to_chat(&request(), "gpt-test", "max_tokens").unwrap();
        assert_eq!(output["model"], "gpt-test");
        assert_eq!(output["messages"][0]["role"], "system");
        assert_eq!(output["tools"][0]["function"]["name"], "read_slide");
        assert_eq!(output["stream"], false);
    }

    #[test]
    fn converts_chat_completion_to_anthropic_message() {
        let response = json!({
            "choices": [{"finish_reason": "stop", "message": {"content": "Hello from upstream"}}],
            "usage": {"prompt_tokens": 4, "completion_tokens": 3}
        });
        let output = from_chat(&response, "claude-office-custom").unwrap();
        assert_eq!(output["content"][0]["text"], "Hello from upstream");
        assert_eq!(output["usage"]["input_tokens"], 4);
    }
}
