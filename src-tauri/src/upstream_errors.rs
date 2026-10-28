use crate::gateway::GatewayError;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Classify untrusted provider error text without forwarding or logging it.
pub fn streaming(payload: &Value) -> GatewayError {
    let detail = payload
        .pointer("/response/error")
        .filter(|v| !v.is_null())
        .or_else(|| payload.get("error").filter(|v| !v.is_null()))
        .unwrap_or(payload);
    let code = detail["code"].as_str().unwrap_or("").to_ascii_lowercase();
    let kind = detail["type"].as_str().unwrap_or("").to_ascii_lowercase();
    let message = detail
        .as_str()
        .or_else(|| detail["message"].as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let all = format!("{kind} {code} {message}");
    let token_limit = all.contains("tokens per minute")
        || all.contains("token rate")
        || all
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|word| word == "tpm");
    let (category, status, kind, message) = if token_limit {
        ("rate_limit_tokens",429,"rate_limit_error","Your provider is rate-limiting tokens (token rate limit). Wait before retrying, or start a shorter Office conversation.")
    } else if all.contains("rate_limit")
        || all.contains("rate limit")
        || all.contains("too many requests")
    {
        (
            "rate_limit",
            429,
            "rate_limit_error",
            "Your provider is rate-limiting the stream. Wait before retrying this action.",
        )
    } else if all.contains("context length")
        || all.contains("context_length")
        || all.contains("context window")
        || all.contains("input too long")
    {
        ("context_length",400,"invalid_request_error","The conversation exceeds the provider context window. Start a new Office conversation or reduce the slide content.")
    } else if all.contains("tool_choice")
        && (all.contains("thinking")
            || all.contains("not supported")
            || all.contains("unsupported"))
    {
        (
            "tool_choice",
            400,
            "invalid_request_error",
            "The provider does not support this tool choice in its current thinking mode.",
        )
    } else if all.contains("invalid_tool_arguments")
        || all.contains("tool arguments")
        || all.contains("function arguments")
        || all.contains("json decode")
        || all.contains("json parse")
    {
        ("tool_arguments",502,"api_error","The provider failed to produce valid tool arguments. Retry this action in a new Office conversation.")
    } else if all.contains("overloaded") || all.contains("overload") || all.contains("capacity") {
        (
            "overloaded",
            529,
            "overloaded_error",
            "The provider is overloaded. Wait before retrying this action.",
        )
    } else if all.contains("insufficient_quota")
        || all.contains("insufficient balance")
        || all.contains("credit balance")
    {
        (
            "quota",
            402,
            "api_error",
            "The provider quota or balance is insufficient. Check the provider account.",
        )
    } else if all.contains("authentication") || all.contains("invalid_api_key") {
        (
            "authentication",
            401,
            "authentication_error",
            "The provider rejected authentication while streaming. Check the provider credentials.",
        )
    } else if all.contains("invalid_request") {
        ("invalid_request",400,"invalid_request_error","The provider rejected the streamed request. Check the provider model and supported features.")
    } else {
        ("unknown",502,"api_error","The provider returned a streaming error. Retry in a new Office conversation; use the gateway request ID to diagnose a repeated failure.")
    };
    let mut error = GatewayError::new(status, kind, message).with_provider_category(category);
    error.provider_fingerprint =
        Some(format!("{:x}", Sha256::digest(detail.to_string().as_bytes())).into_boxed_str());
    error
}

pub fn http(status: u16, payload: Option<&Value>) -> GatewayError {
    let classified = payload.map(streaming);
    let mut error = if classified
        .as_ref()
        .is_some_and(|e| e.provider_category != Some("unknown"))
    {
        classified.unwrap()
    } else {
        let (category, kind, detail) = match status {
            401 => (
                "authentication",
                "authentication_error",
                "Your provider rejected the API key. Check its authentication method.",
            ),
            403 => (
                "permission",
                "permission_error",
                "Your provider denied access to this model or protocol.",
            ),
            429 => (
                "rate_limit",
                "rate_limit_error",
                "Your provider is rate-limiting requests. Wait before retrying this action.",
            ),
            503 | 529 => (
                "overloaded",
                "overloaded_error",
                "The provider is overloaded. Wait before retrying this action.",
            ),
            _ => (
                "unknown",
                "api_error",
                "Your provider rejected the request.",
            ),
        };
        let mut error = GatewayError::new(status, kind, detail).with_provider_category(category);
        error.provider_fingerprint = classified.and_then(|e| e.provider_fingerprint);
        error
    };
    error.status = status;
    error.message = format!("{} (HTTP {status})", error.message);
    error
}
