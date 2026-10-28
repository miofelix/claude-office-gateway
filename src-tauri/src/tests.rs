use crate::{
    certificate,
    config::{GatewayConfig, ProviderStore, UpstreamProtocol},
    gateway::GatewayRouter,
    protocol,
    runtime::{Phase, Runtime},
    stream,
};
use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Json, Router,
};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Notify;
use tower::ServiceExt;

fn config() -> GatewayConfig {
    GatewayConfig {
        base_url: "https://api.example.com".into(),
        upstream_model: "real-model".into(),
        ..Default::default()
    }
}
fn request() -> Value {
    json!({"model":"claude-office-custom","max_tokens":128,"messages":[{"role":"user","content":"hello"}]})
}
fn history() -> Value {
    json!({"model":"claude-office-custom","max_tokens":128,"messages":[{"role":"user","content":"inspect"},{"role":"assistant","content":[{"type":"tool_use","id":"a","name":"read","input":{"page":1}},{"type":"tool_use","id":"b","name":"read","input":{"page":2}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"a","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"aW1hZ2U="}}]},{"type":"tool_result","tool_use_id":"b","is_error":true,"content":"missing"}]}]})
}
#[test]
fn preserves_parallel_tool_result_order_and_screenshots() {
    let out = protocol::to_chat(&history(), "real-model", "max_tokens").unwrap();
    assert_eq!(
        out["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["role"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["user", "assistant", "tool", "tool", "user"]
    );
    assert_eq!(out["messages"][3]["content"], "[Tool error]\nmissing");
    assert_eq!(
        out["messages"][4]["content"][1]["image_url"]["url"],
        "data:image/png;base64,aW1hZ2U="
    );
    let response = protocol::to_responses(&history(), "real-model").unwrap();
    assert_eq!(response["store"], false);
    assert!(response["input"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"]
            .as_array()
            .is_some_and(|a| a.iter().any(|p| p["type"] == "input_image"))));
}

#[test]
fn regression_responses_preserves_exact_image_bytes_and_order() {
    let image = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
    let request = json!({
        "model":"claude-office-custom","max_tokens":128,
        "messages":[{"role":"user","content":[
            {"type":"text","text":"Count the red squares"},
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":image}},
        ]}]
    });
    let outgoing = protocol::to_responses(&request, "real-model").unwrap();
    assert_eq!(outgoing["input"][0]["content"][0]["type"], "input_text");
    assert_eq!(
        outgoing["input"][0]["content"][0]["text"],
        "Count the red squares"
    );
    assert_eq!(outgoing["input"][0]["content"][1]["type"], "input_image");
    assert_eq!(
        outgoing["input"][0]["content"][1]["image_url"],
        format!("data:image/png;base64,{image}")
    );
}
#[test]
fn rejects_unsupported_content_and_invalid_history() {
    for patch in [
        json!({"max_tokens":true}),
        json!({"stream":1}),
        json!({"output_config":{"format":{"type":"json_schema"}}}),
        json!({"container":"id"}),
        json!({"messages":[{"role":"user","content":[{"type":"document"}]}]}),
        json!({"tool_choice":{"type":"tool","name":"missing"}}),
    ] {
        let mut req = request();
        req.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(
            protocol::to_chat(&req, "real-model", "max_tokens").is_err(),
            "{req}"
        );
    }
    let mut req = history();
    req["messages"][2]["content"][1]["tool_use_id"] = json!("missing");
    assert!(protocol::to_chat(&req, "real-model", "max_tokens").is_err());
    let mut req = history();
    req["messages"][1]["content"][1]["id"] = json!("a");
    assert!(protocol::to_chat(&req, "real-model", "max_tokens").is_err());
}
#[test]
fn converts_reasoning_tools_and_named_choices() {
    let mut req = request();
    req["thinking"] = json!({"type":"enabled","budget_tokens":8192});
    req["tools"] = json!([{"name":"read","input_schema":{"type":"object"}}]);
    req["tool_choice"] = json!({"type":"tool","name":"read","disable_parallel_tool_use":true});
    req["stream"] = json!(true);
    let chat = protocol::to_chat(&req, "real", "max_completion_tokens").unwrap();
    assert_eq!(chat["reasoning_effort"], "medium");
    assert_eq!(chat["parallel_tool_calls"], false);
    assert_eq!(chat["stream_options"]["include_usage"], true);
    let response = protocol::to_responses(&req, "real").unwrap();
    assert_eq!(response["tool_choice"]["name"], "read");
    assert_eq!(response["reasoning"]["effort"], "medium");
    assert_eq!(response["tools"][0]["strict"], false);
}

#[test]
fn regression_kimi_named_tool_choice_limits_available_tools_and_preserves_other_hosts() {
    let mut cfg = config();
    cfg.base_url = "https://api.moonshot.cn/v1".into();
    cfg.upstream_protocol = UpstreamProtocol::OpenaiResponses;
    cfg.upstream_model = "kimi-k3.1-pro".into();
    let mut incoming = request();
    incoming["thinking"] = json!({"type":"adaptive"});
    incoming["tools"] = json!([
        {"name":"read_slide","input_schema":{"type":"object"}},
        {"name":"delete_slide","input_schema":{"type":"object"}},
    ]);
    incoming["tool_choice"] = json!({"type":"tool","name":"read_slide"});
    let mut outgoing = protocol::to_responses(&incoming, &cfg.upstream_model).unwrap();
    assert_eq!(outgoing["tool_choice"]["name"], "read_slide");
    crate::providers::prepare_request(&cfg, &incoming, &mut outgoing, false);
    assert_eq!(outgoing["tool_choice"], "auto");
    assert_eq!(outgoing["tools"].as_array().unwrap().len(), 1);
    assert_eq!(outgoing["tools"][0]["name"], "read_slide");
    assert!(outgoing["input"][0]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("read_slide"));
    let mut other = protocol::to_responses(&incoming, &cfg.upstream_model).unwrap();
    cfg.base_url = "https://sub2api.izhe.top".into();
    crate::providers::prepare_request(&cfg, &incoming, &mut other, false);
    assert_eq!(other["tool_choice"]["name"], "read_slide");
    assert_eq!(other["tools"].as_array().unwrap().len(), 2);
    cfg.base_url = "https://api.moonshot.cn/v1".into();
    incoming["tool_choice"] = json!({"type":"any"});
    let mut any = protocol::to_responses(&incoming, &cfg.upstream_model).unwrap();
    assert_eq!(any["tool_choice"], "required");
    crate::providers::prepare_request(&cfg, &incoming, &mut any, false);
    assert_eq!(any["tool_choice"], "auto");
    assert_eq!(any["tools"].as_array().unwrap().len(), 2);
    assert!(any["input"][0]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("provided functions"));
}

#[test]
fn regression_kimi_auto_fallback_never_reports_text_only_as_forced_tool_success() {
    let text_only = json!({"content":[{"type":"text","text":"I cannot call it"}]});
    assert!(crate::providers::ensure_required_function(&text_only, "read_slide").is_err());
    assert!(crate::providers::ensure_required_function(&text_only, "").is_err());
    let other = json!({"content":[{"type":"tool_use","name":"delete_slide","input":{}}]});
    assert!(crate::providers::ensure_required_function(&other, "read_slide").is_err());
    assert!(crate::providers::ensure_required_function(&other, "").is_ok());
    let requested = json!({"content":[{"type":"tool_use","name":"read_slide","input":{}}]});
    assert!(crate::providers::ensure_required_function(&requested, "read_slide").is_ok());
    let terminal = json!({"type":"response.completed","response":{"status":"completed",
        "output":[{"type":"message","content":[{"type":"output_text","text":"I cannot call it"}]}]}});
    let mut adapter =
        stream::Adapter::new(UpstreamProtocol::OpenaiResponses, "claude-office-custom");
    adapter.require_tool("read_slide".into());
    assert!(adapter.feed("message", &terminal.to_string()).is_err());
    assert!(
        !adapter.done,
        "A missing tool call must not emit message_stop"
    );
}
#[test]
fn validates_terminal_outputs_and_tool_arguments() {
    for arguments in ["[]", "{", "null", "42"] {
        assert!(protocol::tool_block(&json!("a"), &json!("read"), &json!(arguments)).is_err());
    }
    assert!(protocol::from_chat(
        &json!({"choices":[{"finish_reason":"content_filter","message":{}}]}),
        "alias"
    )
    .is_err());
    let response=protocol::from_responses(&json!({"status":"completed","output":[{"type":"function_call","call_id":"a","name":"read","arguments":"{}"}]}),"alias").unwrap();
    assert_eq!(response["stop_reason"], "tool_use");
}
#[test]
fn sse_decoder_handles_split_utf8_crlf_and_multiline_records() {
    let mut decoder = stream::Decoder::new();
    let encoded = "\u{feff}event: text\r\ndata: {\"text\":\"你好\"}\r\n\r\n".as_bytes();
    let mut records = Vec::new();
    for byte in encoded {
        records.extend(decoder.push(&[*byte]).unwrap());
    }
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].0, "text");
    assert!(records[0].1.contains("你好"));
    assert_eq!(
        decoder.push(b"data: first\ndata: second\n\n").unwrap()[0].1,
        "first\nsecond"
    );
}
#[test]
fn stream_requires_terminal_event() {
    let mut adapter = stream::Adapter::new(UpstreamProtocol::OpenaiChat, "alias");
    assert!(adapter
        .feed("message", r#"{"choices":[{"delta":{"content":"hello"}}]}"#)
        .is_ok());
    assert!(adapter.eof().is_err());
}

#[test]
fn regression_stream_errors_preserve_actionable_category_without_provider_details() {
    let cases = [
        (
            json!({"error":{"type":"rate_limit_error","message":"token rate limit exceeded api-key-secret"}}),
            "rate_limit_error",
            "rate-limiting",
        ),
        (
            json!({"error":{"type":"invalid_request_error","message":"maximum context length exceeded; private slide title"}}),
            "invalid_request_error",
            "context window",
        ),
        (
            json!({"type":"response.failed","response":{"error":{"code":"invalid_tool_arguments","message":"function arguments are not valid JSON; private slide title"}}}),
            "api_error",
            "tool arguments",
        ),
    ];
    for (payload, expected_kind, expected_message) in cases {
        let mut adapter = stream::Adapter::new(UpstreamProtocol::OpenaiResponses, "alias");
        let error = adapter.feed("error", &payload.to_string()).unwrap_err();
        assert_eq!(error.kind, expected_kind);
        assert!(
            error.message.contains(expected_message),
            "{}",
            error.message
        );
        assert!(
            !error.message.contains("private slide") && !error.message.contains("api-key-secret")
        );
        assert!(!adapter.done);
    }
}

#[test]
fn regression_sse_mixed_line_endings_preserve_each_event_at_every_chunk_boundary() {
    let input = b"event: first\r\ndata: one\r\n\r\nevent: second\ndata: two\n\n";
    let expected = vec![
        ("first".to_string(), "one".to_string()),
        ("second".to_string(), "two".to_string()),
    ];
    for split in 0..=input.len() {
        let mut decoder = stream::Decoder::new();
        let mut actual = decoder.push(&input[..split]).unwrap();
        actual.extend(decoder.push(&input[split..]).unwrap());
        actual.extend(decoder.finish().unwrap());
        assert_eq!(actual, expected, "TCP split at byte {split}");
    }
}

async fn timeout_test_response(
    upstream: Router,
    streaming: bool,
    timeout_seconds: u64,
) -> (Response, axum_server::Handle) {
    let (pem, key, der) = certificate::generate_identity().unwrap();
    let (upstream_port, upstream_handle) = serve(upstream, &pem, &key).await;
    let mut cfg = config();
    cfg.base_url = format!("https://localhost:{upstream_port}");
    cfg.upstream_protocol = UpstreamProtocol::OpenaiResponses;
    cfg.timeout_seconds = timeout_seconds;
    let client = GatewayRouter::client_builder(&cfg)
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
        .build()
        .unwrap();
    let app = GatewayRouter::with_client(
        cfg,
        "gateway-secret".into(),
        "provider-secret".into(),
        client,
    )
    .unwrap()
    .app();
    let mut body = request();
    body["stream"] = json!(streaming);
    let response = tokio::time::timeout(
        Duration::from_secs(timeout_seconds + 2),
        app.oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("host", "localhost:15724")
                .header("x-api-key", "gateway-secret")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        ),
    )
    .await
    .expect("Gateway must time out even before the provider sends headers")
    .unwrap();
    (response, upstream_handle)
}

fn active_stream_upstream(ticks: usize, delay: Duration) -> Router {
    Router::new().fallback(any(move || async move {
        let source = async_stream::stream! {
            yield Ok::<Bytes, std::io::Error>(Bytes::from_static(
                b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n\n",
            ));
            for _ in 0..ticks {
                tokio::time::sleep(delay).await;
                yield Ok(Bytes::from_static(b": keep-alive\n\n"));
            }
            yield Ok(Bytes::from_static(
                b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"Hello\"}]}]}}\n\n",
            ));
        };
        let mut response = Response::new(Body::from_stream(source));
        response.headers_mut().insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
        response
    }))
}

#[tokio::test]
async fn regression_active_stream_must_not_hit_total_provider_timeout() {
    let (response, upstream_handle) = timeout_test_response(
        active_stream_upstream(8, Duration::from_millis(175)),
        true,
        1,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        to_bytes(response.into_body(), 1024 * 1024),
    )
    .await
    .unwrap()
    .unwrap();
    upstream_handle.shutdown();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        !text.contains("The provider timed out"),
        "An active stream was cut off by the provider timeout: {text}"
    );
    assert!(
        text.contains("message_stop"),
        "Stream must complete successfully: {text}"
    );
}

#[tokio::test]
async fn regression_provider_timeout_still_bounds_response_headers() {
    let upstream = Router::new().fallback(any(|| async {
        tokio::time::sleep(Duration::from_secs(5)).await;
        Json(json!({"status":"completed","output":[]}))
    }));
    let (response, upstream_handle) = timeout_test_response(upstream, true, 1).await;
    upstream_handle.shutdown();
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["error"]["type"], "api_error");
    assert!(payload["error"]["message"]
        .as_str()
        .unwrap()
        .contains("timed out"));
}

#[tokio::test]
async fn regression_provider_timeout_still_bounds_stalled_stream() {
    let upstream = Router::new().fallback(any(|| async {
        let source = futures_util::stream::once(async {
            Ok::<Bytes, std::io::Error>(Bytes::from_static(b": connected\n\n"))
        })
        .chain(futures_util::stream::pending());
        let mut response = Response::new(Body::from_stream(source));
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
        response
    }));
    // The upstream must still time out after Office has received a heartbeat.
    let (response, upstream_handle) = timeout_test_response(upstream, true, 11).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = tokio::time::timeout(
        Duration::from_secs(13),
        to_bytes(response.into_body(), 1024 * 1024),
    )
    .await
    .expect("A stalled provider must not be hidden by downstream heartbeats")
    .unwrap();
    upstream_handle.shutdown();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(text.contains("event: ping"), "{text}");
    assert_eq!(text.matches("event: error").count(), 1, "{text}");
    assert!(text.contains("The provider timed out"), "{text}");
    assert!(
        !text.contains("message_stop"),
        "A timeout must not look like successful completion"
    );
}

#[tokio::test]
async fn regression_provider_timeout_still_bounds_total_json_duration() {
    let upstream = Router::new().fallback(any(|| async {
        let source = async_stream::stream! {
            for _ in 0..8 {
                yield Ok::<Bytes, std::io::Error>(Bytes::from_static(b" "));
                tokio::time::sleep(Duration::from_millis(175)).await;
            }
            yield Ok(Bytes::from_static(b"{\"status\":\"completed\",\"output\":[]}"));
        };
        let mut response = Response::new(Body::from_stream(source));
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        response
    }));
    let (response, upstream_handle) = timeout_test_response(upstream, false, 1).await;
    upstream_handle.shutdown();
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(payload["error"]["message"]
        .as_str()
        .unwrap()
        .contains("timed out"));
}

#[tokio::test]
async fn regression_gateway_pings_while_upstream_only_sends_comments() {
    let (response, upstream_handle) = timeout_test_response(
        active_stream_upstream(44, Duration::from_millis(250)),
        true,
        1,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut source = response.into_body().into_data_stream();
    tokio::time::timeout(Duration::from_secs(12), async {
        while let Some(chunk) = source.next().await {
            let chunk = chunk.unwrap();
            let text = String::from_utf8_lossy(&chunk);
            assert!(!text.contains("event: error"), "{text}");
            if text.contains("event: ping") {
                assert!(text.contains("\"type\":\"ping\""));
                return;
            }
        }
        panic!("Upstream comments must not leave Office without a keepalive event");
    })
    .await
    .expect("Office must receive a ping during a long reasoning interval");
    drop(source);
    upstream_handle.shutdown();
}

#[tokio::test]
async fn regression_stream_concurrency_limit_holds_until_response_body_is_dropped() {
    let (pem, key, der) = certificate::generate_identity().unwrap();
    let mock = Mock {
        kind: UpstreamProtocol::OpenaiResponses,
        count: Arc::new(AtomicUsize::new(0)),
        release: Arc::new(Notify::new()),
    };
    let (upstream_port, upstream_handle) = serve(
        Router::new()
            .fallback(any(upstream))
            .with_state(mock.clone()),
        &pem,
        &key,
    )
    .await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut cfg = config();
    cfg.base_url = format!("https://localhost:{upstream_port}/gateway");
    cfg.upstream_protocol = UpstreamProtocol::OpenaiResponses;
    let app = GatewayRouter::with_client(
        cfg,
        "gateway-secret".into(),
        "provider-secret".into(),
        client,
    )
    .unwrap()
    .app();
    let make_request = || {
        axum::http::Request::builder()
            .method("POST")
            .uri("/v1/messages")
            .header("host", "localhost:15724")
            .header("x-api-key", "gateway-secret")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "model":"claude-office-custom", "max_tokens":128, "stream":true,
                    "messages":[{"role":"user","content":"hello"}],
                })
                .to_string(),
            ))
            .unwrap()
    };
    let mut responses = Vec::new();
    for _ in 0..32 {
        let response = app.clone().oneshot(make_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        responses.push(response);
    }
    let overflow = app.clone().oneshot(make_request()).await.unwrap();
    assert_eq!(
        overflow.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "32 unfinished SSE bodies must occupy all 32 request slots"
    );
    assert_eq!(mock.count.load(Ordering::SeqCst), 32);
    drop(overflow);
    drop(responses.pop());
    let replacement = app.clone().oneshot(make_request()).await.unwrap();
    assert_eq!(
        replacement.status(),
        StatusCode::OK,
        "Disconnecting a stream must release its slot immediately"
    );
    drop(replacement);
    drop(responses);
    upstream_handle.shutdown();
}

struct TestResolver {
    attempts: Arc<AtomicUsize>,
    fail_for: usize,
    delay: Duration,
}
impl reqwest::dns::Resolve for TestResolver {
    fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
        let fail = attempt < self.fail_for;
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            if fail {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "simulated transient resolver failure",
                )
                .into());
            }
            let address: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
            Ok(Box::new(std::iter::once(address)) as reqwest::dns::Addrs)
        })
    }
}

#[tokio::test]
async fn regression_transient_connection_failure_recovers_without_replaying_post() {
    let (pem, key, der) = certificate::generate_identity().unwrap();
    let received = Arc::new(AtomicUsize::new(0));
    let counter = received.clone();
    let upstream = Router::new().fallback(any(move |request: Request| {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            let body: Value =
                serde_json::from_slice(&to_bytes(request.into_body(), 1024 * 1024).await.unwrap())
                    .unwrap();
            assert_eq!(body["messages"][0]["content"], "hello");
            Json(json!({"type":"message","content":[{"type":"text","text":"recovered"}]}))
        }
    }));
    let (port, handle) = serve(upstream, &pem, &key).await;
    let mut cfg = config();
    cfg.upstream_protocol = UpstreamProtocol::Anthropic;
    cfg.base_url = format!("https://localhost:{port}");
    cfg.timeout_seconds = 3;
    let attempts = Arc::new(AtomicUsize::new(0));
    let client = GatewayRouter::client_builder(&cfg)
        .no_proxy()
        .dns_resolver(Arc::new(TestResolver {
            attempts: attempts.clone(),
            fail_for: 1,
            delay: Duration::ZERO,
        }))
        .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
        .build()
        .unwrap();
    let app = GatewayRouter::with_client(
        cfg,
        "gateway-secret".into(),
        "provider-secret".into(),
        client,
    )
    .unwrap()
    .app();
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("host", "localhost:15724")
                .header("x-api-key", "gateway-secret")
                .header("content-type", "application/json")
                .body(Body::from(request().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    handle.shutdown();
    assert_eq!(
        status,
        StatusCode::OK,
        "A transient pre-send connection failure must recover: {}",
        String::from_utf8_lossy(&bytes)
    );
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        2,
        "Exactly one bounded connection retry"
    );
    assert_eq!(
        received.load(Ordering::SeqCst),
        1,
        "POST must reach the provider exactly once"
    );
}

fn transport_request(streaming: bool) -> axum::http::Request<Body> {
    let mut body = request();
    body["stream"] = json!(streaming);
    axum::http::Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("host", "localhost:15724")
        .header("x-api-key", "gateway-secret")
        .header("origin", "https://pivot.claude.ai")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn transport_app(upstream: Router) -> (Router, axum_server::Handle) {
    let (pem, key, der) = certificate::generate_identity().unwrap();
    let (port, handle) = serve(upstream, &pem, &key).await;
    let mut cfg = config();
    cfg.upstream_protocol = UpstreamProtocol::Anthropic;
    cfg.base_url = format!("https://localhost:{port}");
    cfg.timeout_seconds = 2;
    let client = GatewayRouter::client_builder(&cfg)
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
        .build()
        .unwrap();
    let app = GatewayRouter::with_client(
        cfg,
        "gateway-secret".into(),
        "provider-secret".into(),
        client,
    )
    .unwrap()
    .app();
    (app, handle)
}

#[tokio::test]
async fn regression_provider_rejection_is_not_replayed_and_preserves_retry_headers() {
    for status in [
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        let received = Arc::new(AtomicUsize::new(0));
        let counter = received.clone();
        let upstream = Router::new().fallback(any(move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                (
                    status,
                    [("retry-after", "7"), ("x-request-id", "upstream-test-id")],
                    "provider-secret body must not leak",
                )
            }
        }));
        let (app, handle) = transport_app(upstream).await;
        let response = app.oneshot(transport_request(false)).await.unwrap();
        handle.shutdown();
        assert_eq!(response.status(), status);
        assert_eq!(
            received.load(Ordering::SeqCst),
            1,
            "Never replay a rejected provider request"
        );
        assert_eq!(
            response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
            Some("7"),
            "Office must receive the provider retry delay"
        );
        assert_eq!(response.headers()["x-request-id"], "upstream-test-id");
        let request_id = response.headers()["x-office-gateway-request-id"]
            .to_str()
            .unwrap()
            .to_string();
        assert!(response.headers()["access-control-expose-headers"]
            .to_str()
            .unwrap()
            .contains("x-office-gateway-request-id"));
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let payload: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["request_id"], request_id);
        assert!(!String::from_utf8_lossy(&bytes).contains("provider-secret"));
    }
}

#[tokio::test]
async fn regression_http_token_limit_is_actionable_and_identifiable_in_office_error_text() {
    let upstream = Router::new().fallback(any(|| async {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "11")],
            Json(json!({"error":{"type":"rate_limit_error","message":
                "You have exceeded the TPM (Tokens Per Minute) rate limit. provider-secret private-slide-title"}})),
        )
    }));
    let (app, handle) = transport_app(upstream).await;
    let response = app.oneshot(transport_request(true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "11");
    let request_id = response.headers()["x-office-gateway-request-id"]
        .to_str()
        .unwrap()
        .to_string();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    handle.shutdown();
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    let message = payload["error"]["message"].as_str().unwrap();
    assert!(message.contains("token rate limit"), "{message}");
    assert!(
        message.contains(&request_id),
        "Office copies only error.message; the request ID must survive"
    );
    assert!(!String::from_utf8_lossy(&bytes).contains("provider-secret"));
    assert!(!String::from_utf8_lossy(&bytes).contains("private-slide-title"));
}

#[tokio::test]
async fn regression_stream_rejection_keeps_category_and_request_id_without_success_or_replay() {
    for event_name in ["error", "response.failed"] {
        let payload = if event_name == "error" {
            json!({"error":{"type":"overloaded_error","message":"overloaded provider-secret private-slide-title"}})
        } else {
            json!({"type":"response.failed","response":{"error":{"code":"context_length_exceeded","message":"private-slide-title"}}})
        };
        let expected_kind = if event_name == "error" {
            "overloaded_error"
        } else {
            "invalid_request_error"
        };
        let received = Arc::new(AtomicUsize::new(0));
        let counter = received.clone();
        let upstream = Router::new().fallback(any(move || {
            let payload = payload.clone();
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                let body = format!("event: {event_name}\ndata: {payload}\n\n");
                ([("content-type", "text/event-stream")], body)
            }
        }));
        let (response, handle) = timeout_test_response(upstream, true, 2).await;
        let request_id = response.headers()["x-office-gateway-request-id"]
            .to_str()
            .unwrap()
            .to_string();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        handle.shutdown();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        let errors: Vec<Value> = text
            .lines()
            .filter_map(|l| l.strip_prefix("data: "))
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .filter(|v| v["type"] == "error")
            .collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0]["error"]["type"], expected_kind);
        assert!(errors[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains(&request_id));
        assert!(!text.contains("message_stop"));
        assert!(!text.contains("provider-secret") && !text.contains("private-slide-title"));
        assert_eq!(received.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn regression_http_diagnostics_bound_error_bodies_and_preserve_original_rejection() {
    for stalled in [false, true] {
        let upstream = Router::new().fallback(any(move || async move {
            let source = async_stream::stream! {
                if stalled { tokio::time::sleep(Duration::from_secs(4)).await; }
                yield Ok::<Bytes, std::io::Error>(Bytes::from(vec![b'x'; 32 * 1024]));
            };
            let mut response = Response::new(Body::from_stream(source));
            *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
            response
                .headers_mut()
                .insert("retry-after", "9".parse().unwrap());
            response
        }));
        let (app, handle) = transport_app(upstream).await;
        let response =
            tokio::time::timeout(Duration::from_secs(3), app.oneshot(transport_request(true)))
                .await
                .expect("Diagnostic reads cannot stall Office rejection handling")
                .unwrap();
        handle.shutdown();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()["retry-after"], "9");
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let payload: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["error"]["type"], "rate_limit_error");
    }
}

#[tokio::test]
async fn regression_committed_post_and_partial_tool_stream_are_never_replayed() {
    for streaming in [false, true] {
        let received = Arc::new(AtomicUsize::new(0));
        let counter = received.clone();
        let upstream = Router::new().fallback(any(move |request: Request| {
            let counter = counter.clone();
            async move {
                let _ = to_bytes(request.into_body(),1024*1024).await.unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
                let source = async_stream::stream! {
                    if streaming {
                        yield Ok::<Bytes,std::io::Error>(Bytes::from_static(b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"real-model\"}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"single-call\",\"name\":\"read_slide\",\"input\":{}}}\n\n"));
                        tokio::time::sleep(Duration::from_millis(40)).await;
                    }
                    yield Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset,"simulated drop after POST"));
                };
                let mut response = Response::new(Body::from_stream(source));
                response.headers_mut().insert(header::CONTENT_TYPE,
                    if streaming {"text/event-stream"} else {"application/json"}.parse().unwrap());
                response
            }
        }));
        let (app, handle) = transport_app(upstream).await;
        let response = app.oneshot(transport_request(streaming)).await.unwrap();
        let request_id = response.headers()["x-office-gateway-request-id"]
            .to_str()
            .unwrap()
            .to_string();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        handle.shutdown();
        assert_eq!(
            received.load(Ordering::SeqCst),
            1,
            "POST already consumed; retry would risk duplicate billing or tools"
        );
        assert!(
            text.contains(&request_id),
            "Error must identify the same gateway request"
        );
        if streaming {
            assert_eq!(text.matches("single-call").count(), 1);
            assert_eq!(text.matches("event: error").count(), 1);
            assert!(!text.contains("message_stop"));
        } else {
            let payload: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(payload["error"]["type"], "api_error");
        }
    }
}

#[tokio::test]
async fn regression_tls_failures_are_classified_without_retry_or_secret_leak() {
    let (pem, key, _) = certificate::generate_identity().unwrap();
    let received = Arc::new(AtomicUsize::new(0));
    let counter = received.clone();
    let upstream = Router::new().fallback(any(move || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            "must not be reached"
        }
    }));
    let (port, handle) = serve(upstream, &pem, &key).await;
    let mut cfg = config();
    cfg.base_url = format!("https://localhost:{port}");
    let attempts = Arc::new(AtomicUsize::new(0));
    let client = GatewayRouter::client_builder(&cfg)
        .no_proxy()
        .dns_resolver(Arc::new(TestResolver {
            attempts: attempts.clone(),
            fail_for: 0,
            delay: Duration::ZERO,
        }))
        .build()
        .unwrap();
    let error = client
        .get(format!("https://localhost:{port}/?api_key=secret-query"))
        .send()
        .await
        .unwrap_err();
    let failure = crate::transport::Failure::classify(&error);
    assert_eq!(failure.category, "tls");
    assert!(!format!("{failure:?}").contains("secret-query"));
    attempts.store(0, Ordering::SeqCst);
    let app = GatewayRouter::with_client(
        cfg,
        "gateway-secret".into(),
        "provider-secret".into(),
        client,
    )
    .unwrap()
    .app();
    let response = app.oneshot(transport_request(false)).await.unwrap();
    handle.shutdown();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "Certificate failure must not retry"
    );
    assert_eq!(
        received.load(Ordering::SeqCst),
        0,
        "TLS failure must not reach HTTP service"
    );
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("TLS"), "{text}");
    assert!(!text.contains("provider-secret") && !text.contains("secret-query"));
}

#[tokio::test]
async fn regression_connection_retry_is_bounded_and_keeps_the_original_deadline() {
    for delay in [Duration::ZERO, Duration::from_millis(500)] {
        let attempts = Arc::new(AtomicUsize::new(0));
        let mut cfg = config();
        cfg.upstream_protocol = UpstreamProtocol::Anthropic;
        cfg.timeout_seconds = 1;
        let client = GatewayRouter::client_builder(&cfg)
            .no_proxy()
            .dns_resolver(Arc::new(TestResolver {
                attempts: attempts.clone(),
                fail_for: usize::MAX,
                delay,
            }))
            .build()
            .unwrap();
        let app = GatewayRouter::with_client(
            cfg,
            "gateway-secret".into(),
            "provider-secret".into(),
            client,
        )
        .unwrap()
        .app();
        let started = std::time::Instant::now();
        let response = tokio::time::timeout(
            Duration::from_secs(2),
            app.oneshot(transport_request(false)),
        )
        .await
        .expect("Retry must not reset the total request deadline")
        .unwrap();
        let elapsed = started.elapsed();
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "At most two connection attempts"
        );
        if delay.is_zero() {
            assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        } else {
            assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
            assert!(
                elapsed < Duration::from_millis(1250),
                "The second 500ms resolver attempt exceeded the original deadline: {elapsed:?}"
            );
        }
    }
}

#[tokio::test]
async fn regression_proxy_rejection_is_classified_and_not_retried() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = attempts.clone();
    let proxy = Router::new().fallback(any(move || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            StatusCode::PROXY_AUTHENTICATION_REQUIRED
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let proxy_task = tokio::spawn(async move {
        axum::serve(listener, proxy.into_make_service())
            .await
            .unwrap();
    });
    let cfg = config();
    let client = GatewayRouter::client_builder(&cfg)
        .no_proxy()
        .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).unwrap())
        .build()
        .unwrap();
    let app = GatewayRouter::with_client(
        cfg,
        "gateway-secret".into(),
        "provider-secret".into(),
        client,
    )
    .unwrap()
    .app();
    let response = app.oneshot(transport_request(false)).await.unwrap();
    proxy_task.abort();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "Proxy authentication cannot be repaired by replay"
    );
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("proxy"), "{text}");
    assert!(!text.contains("provider-secret") && !text.contains("127.0.0.1"));
}

#[test]
fn migrates_swift_config_without_losing_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path,json!({"baseURL":"https://provider.example/v1","upstreamModel":"saved-model","upstreamProtocol":"anthropic","publicModel":"office-saved","displayName":"Saved","authHeader":"x-api-key","maxTokensField":"max_tokens","timeoutSeconds":45,"allowedOrigins":["https://pivot.claude.ai"]}).to_string()).unwrap();
    let saved = ProviderStore::load(&path)
        .unwrap()
        .active()
        .unwrap()
        .config
        .clone();
    assert_eq!(saved.upstream_model, "saved-model");
    assert_eq!(saved.public_model, "office-saved");
    assert_eq!(saved.timeout_seconds, 45);
    ProviderStore::load(&path).unwrap().save(&path).unwrap();
    assert!(path.with_extension("swift-backup.json").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn rejects_bad_hosts_origins_and_auth_and_allows_office_preflight() {
    let app = GatewayRouter::new(config(), "gateway-secret".into(), "provider-secret".into())
        .unwrap()
        .app();
    for (host, origin, key, status) in [
        ("evil.example:15724", None, Some("gateway-secret"), 403),
        ("localhost:15725", None, Some("gateway-secret"), 403),
        ("localhost", None, Some("gateway-secret"), 403),
        (
            "localhost:15724",
            Some("https://evil.example"),
            Some("gateway-secret"),
            403,
        ),
        ("localhost:15724", None, Some("wrong"), 401),
        ("localhost:15724", None, None, 401),
    ] {
        let mut req = Request::builder()
            .uri("/v1/models")
            .header(header::HOST, host);
        if let Some(origin) = origin {
            req = req.header(header::ORIGIN, origin);
        }
        if let Some(key) = key {
            req = req.header("x-api-key", key);
        }
        let response = app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
    }
    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/v1/messages")
        .header(header::HOST, "localhost:15724")
        .header(header::ORIGIN, "https://pivot.claude.ai")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "x-api-key, anthropic-version, x-stainless-runtime-version",
        )
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(preflight).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://pivot.claude.ai"
    );
    let unknown = Request::builder()
        .uri("/v1/models/unknown")
        .header(header::HOST, "localhost:15724")
        .header("x-api-key", "gateway-secret")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(unknown).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
}

#[derive(Clone)]
struct Mock {
    kind: UpstreamProtocol,
    count: Arc<AtomicUsize>,
    release: Arc<Notify>,
}
async fn upstream(State(mock): State<Mock>, request: Request) -> Response {
    mock.count.fetch_add(1, Ordering::SeqCst);
    let expected = match mock.kind {
        UpstreamProtocol::Anthropic => "messages",
        UpstreamProtocol::OpenaiChat => "chat/completions",
        UpstreamProtocol::OpenaiResponses => "responses",
    };
    assert_eq!(request.uri().path(), format!("/gateway/v1/{expected}"));
    assert_eq!(request.headers()["authorization"], "Bearer provider-secret");
    let body: Value =
        serde_json::from_slice(&to_bytes(request.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(body["model"], "real-model");
    if body["stream"] != true {
        let response = match mock.kind {
            UpstreamProtocol::Anthropic => {
                json!({"type":"message","id":"native-id","model":"real-model","content":[{"type":"text","text":"hello"}],"stop_reason":"end_turn","usage":{"input_tokens":2,"output_tokens":1}})
            }
            UpstreamProtocol::OpenaiChat => {
                json!({"choices":[{"finish_reason":"stop","message":{"content":"hello"}}],"usage":{"prompt_tokens":2,"completion_tokens":1}})
            }
            UpstreamProtocol::OpenaiResponses => {
                json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"hello"}]}],"usage":{"input_tokens":2,"output_tokens":1}})
            }
        };
        return Json(response).into_response();
    }
    let first = match mock.kind {
        UpstreamProtocol::Anthropic => format!(
            "{}{}",
            String::from_utf8_lossy(&stream::event(
                "message_start",
                &json!({"type":"message_start","message":{"id":"native","type":"message","role":"assistant","model":"real-model","content":[],"usage":{"input_tokens":2,"output_tokens":0}}})
            )),
            String::from_utf8_lossy(&stream::event(
                "content_block_delta",
                &json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}})
            ))
        ),
        UpstreamProtocol::OpenaiChat => format!(
            "data: {}\n\n",
            json!({"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]})
        ),
        UpstreamProtocol::OpenaiResponses => format!(
            "data: {}\n\n",
            json!({"type":"response.output_text.delta","delta":"Hello"})
        ),
    };
    let last = match mock.kind {
        UpstreamProtocol::Anthropic => {
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".into()
        }
        UpstreamProtocol::OpenaiChat => format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1}})
        ),
        UpstreamProtocol::OpenaiResponses => format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Hello"}]}],"usage":{"input_tokens":2,"output_tokens":1}}})
        ),
    };
    let source = async_stream::stream! {yield Ok::<Bytes,std::io::Error>(Bytes::from(first));mock.release.notified().await;yield Ok(Bytes::from(last));};
    let mut response = Response::new(Body::from_stream(source));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
    response
}
async fn serve(app: Router, pem: &str, key: &str) -> (u16, axum_server::Handle) {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem(
        pem.as_bytes().to_vec(),
        key.as_bytes().to_vec(),
    )
    .await
    .unwrap();
    let handle = axum_server::Handle::new();
    let h = handle.clone();
    tokio::spawn(async move {
        axum_server::from_tcp_rustls(listener, tls)
            .handle(h)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
    handle.listening().await.unwrap();
    (port, handle)
}
#[tokio::test]
async fn https_http2_end_to_end_all_protocols_stream_before_completion_and_cache_only_probes() {
    for kind in [
        UpstreamProtocol::Anthropic,
        UpstreamProtocol::OpenaiChat,
        UpstreamProtocol::OpenaiResponses,
    ] {
        let (pem, key, der) = certificate::generate_identity().unwrap();
        let mock = Mock {
            kind: kind.clone(),
            count: Arc::new(AtomicUsize::new(0)),
            release: Arc::new(Notify::new()),
        };
        let (upstream_port, upstream_handle) = serve(
            Router::new()
                .fallback(any(upstream))
                .with_state(mock.clone()),
            &pem,
            &key,
        )
        .await;
        let client = reqwest::Client::builder()
            .http2_prior_knowledge()
            .no_proxy()
            .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut cfg = config();
        cfg.port = port;
        cfg.base_url = format!("https://localhost:{upstream_port}/gateway");
        cfg.upstream_protocol = kind.clone();
        let router = GatewayRouter::with_client(
            cfg,
            "gateway-secret".into(),
            "provider-secret".into(),
            client.clone(),
        )
        .unwrap();
        router.verify_upstream().await.unwrap();
        assert_eq!(mock.count.load(Ordering::SeqCst), 2);
        let app = router.app();
        // Serve at the configured Host port, so the same auth/origin path runs over real TLS.
        let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem(
            pem.as_bytes().to_vec(),
            key.as_bytes().to_vec(),
        )
        .await
        .unwrap();
        let gateway_handle = axum_server::Handle::new();
        let h = gateway_handle.clone();
        tokio::spawn(async move {
            axum_server::from_tcp_rustls(listener, tls)
                .handle(h)
                .serve(app.into_make_service())
                .await
                .unwrap();
        });
        gateway_handle.listening().await.unwrap();
        let url = format!("https://localhost:{port}");
        let response = client
            .get(format!("{url}/v1/models"))
            .header("x-api-key", "gateway-secret")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        assert_eq!(response.version(), axum::http::Version::HTTP_2);
        let models: Value = response.json().await.unwrap();
        assert_eq!(models["data"][0]["id"], "claude-office-custom");
        let probe = json!({"model":"claude-office-custom","max_tokens":1,"messages":[{"role":"user","content":"x"}]});
        let response = client
            .post(format!("{url}/v1/messages"))
            .header("x-api-key", "gateway-secret")
            .json(&probe)
            .send()
            .await
            .unwrap();
        assert_eq!(response.headers()["x-office-gateway-probe-cache"], "hit");
        assert_eq!(mock.count.load(Ordering::SeqCst), 2);
        let mut streaming = request();
        streaming["stream"] = json!(true);
        let response = client
            .post(format!("{url}/v1/messages"))
            .header("x-api-key", "gateway-secret")
            .json(&streaming)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let mut bytes = response.bytes_stream();
        let mut collected = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !String::from_utf8_lossy(&collected).contains("Hello") {
                collected.extend_from_slice(&bytes.next().await.unwrap().unwrap());
            }
        })
        .await
        .expect("Text must stream before the upstream terminal event is released");
        assert!(!String::from_utf8_lossy(&collected).contains("message_stop"));
        mock.release.notify_one();
        while let Some(chunk) = bytes.next().await {
            collected.extend_from_slice(&chunk.unwrap());
        }
        let text = String::from_utf8(collected).unwrap();
        assert!(text.contains("message_stop"));
        assert!(text.contains("claude-office-custom"));
        assert!(!text.contains("real-model"));
        gateway_handle.shutdown();
        upstream_handle.shutdown();
    }
}
#[tokio::test]
async fn runtime_reports_port_conflict_and_cannot_recheck_when_stopped() {
    let (pem, key, _) = certificate::generate_identity().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert, pem).unwrap();
    std::fs::write(&key_path, key).unwrap();
    let occupied = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let mut cfg = config();
    cfg.port = occupied.local_addr().unwrap().port();
    let router = GatewayRouter::new(cfg.clone(), "token".into(), "key".into()).unwrap();
    let runtime = Runtime::default();
    assert!(runtime.recheck().await.is_err());
    runtime
        .connect(cfg, router, &cert, &key_path)
        .await
        .unwrap();
    assert_eq!(runtime.snapshot().phase, Phase::Failed);
    assert!(!runtime.snapshot().running);
    assert!(runtime.snapshot().error.unwrap().contains("port is in use"));
    runtime.stop().await;
    assert_eq!(runtime.snapshot().phase, Phase::Stopped);
}

#[tokio::test]
async fn stopping_during_verification_cancels_without_resurrecting_listener() {
    let (pem, key, der) = certificate::generate_identity().unwrap();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let a = entered.clone();
    let b = release.clone();
    let upstream = Router::new().fallback(any(move || {
        let entered = a.clone();
        let release = b.clone();
        async move {
            entered.notify_one();
            release.notified().await;
            Json(json!({"choices":[{"finish_reason":"stop","message":{"content":"ok"}}]}))
        }
    }));
    let (provider_port, provider_handle) = serve(upstream, &pem, &key).await;
    let client = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let slot = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = slot.local_addr().unwrap().port();
    drop(slot);
    let mut cfg = config();
    cfg.port = port;
    cfg.base_url = format!("https://localhost:{provider_port}");
    let router =
        GatewayRouter::with_client(cfg.clone(), "token".into(), "key".into(), client).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, pem).unwrap();
    std::fs::write(&key_path, key).unwrap();
    let runtime = Runtime::default();
    let connecting = runtime.clone();
    let task = tokio::spawn(async move {
        connecting
            .connect(cfg, router, &cert_path, &key_path)
            .await
            .unwrap();
    });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    assert_eq!(runtime.snapshot().phase, Phase::Checking);
    runtime.stop().await;
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(runtime.snapshot().phase, Phase::Stopped);
    assert!(!runtime.snapshot().running);
    let _reused = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    provider_handle.shutdown();
}

#[tokio::test]
async fn certificate_identity_is_not_port_scoped_and_does_not_trust_an_unrelated_localhost_certificate(
) {
    let (pem, key, der) = certificate::generate_identity().unwrap();
    let app = Router::new().fallback(any(|| async { "local-test" }));
    let (first, a) = serve(app.clone(), &pem, &key).await;
    let (second, b) = serve(app.clone(), &pem, &key).await;
    assert_ne!(first, second);
    let client = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    for port in [first, second] {
        assert_eq!(
            client
                .get(format!("https://localhost:{port}"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "local-test"
        );
    }
    let (other_pem, other_key, _) = certificate::generate_identity().unwrap();
    let (other, c) = serve(app, &other_pem, &other_key).await;
    assert!(client
        .get(format!("https://localhost:{other}"))
        .send()
        .await
        .is_err());
    a.shutdown();
    b.shutdown();
    c.shutdown();
}

#[tokio::test]
async fn http2_authority_allows_office_preflight_and_rejects_foreign_or_conflicting_hosts() {
    let app = GatewayRouter::new(config(), "gateway-secret".into(), "provider-secret".into())
        .unwrap()
        .app();
    for (authority, host, status) in [
        ("localhost:15724", None, StatusCode::OK),
        ("127.0.0.1:15724", None, StatusCode::OK),
        ("[::1]:15724", None, StatusCode::OK),
        ("evil.example:15724", None, StatusCode::FORBIDDEN),
        ("localhost:15725", None, StatusCode::FORBIDDEN),
        (
            "localhost:15724",
            Some("evil.example:15724"),
            StatusCode::FORBIDDEN,
        ),
        (
            "evil.example:15724",
            Some("localhost:15724"),
            StatusCode::FORBIDDEN,
        ),
        (
            "localhost:15724",
            Some("127.0.0.1:15724"),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let mut request = Request::builder()
            .version(axum::http::Version::HTTP_2)
            .uri(format!("https://{authority}/health"));
        if let Some(host) = host {
            request = request.header(header::HOST, host);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            status,
            "authority={authority}, host={host:?}"
        );
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .version(axum::http::Version::HTTP_2)
                .uri("https://localhost:15724/v1/models")
                .method("OPTIONS")
                .header(header::ORIGIN, "https://pivot.claude.ai")
                .header("access-control-request-method", "GET")
                .header(
                    "access-control-request-headers",
                    "x-api-key,anthropic-version",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://pivot.claude.ai"
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn deepseek_tools_and_results_use_non_thinking_mode_without_changing_other_requests() {
    async fn provider(request: Request) -> Response {
        let chat = !request.uri().path().contains("/anthropic/");
        let count = request.uri().path().ends_with("/count_tokens");
        let deepseek = request.uri().host() == Some("api.deepseek.com")
            || request
                .headers()
                .get(header::HOST)
                .is_some_and(|h| h.to_str().unwrap_or("").starts_with("api.deepseek.com:"));
        let body: Value =
            serde_json::from_slice(&to_bytes(request.into_body(), 16384).await.unwrap()).unwrap();
        if deepseek
            && !chat
            && body["tools"]
                .as_array()
                .is_some_and(|tools| tools.iter().any(|tool| tool["type"] == "custom"))
        {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error":{"message":"Unknown tool type custom"}})),
            )
                .into_response();
        }
        let with_tools = body["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty())
            || body["messages"].as_array().unwrap().iter().any(|message| {
                message["role"] == "tool"
                    || message["content"].as_array().is_some_and(|blocks| {
                        blocks.iter().any(|block| block["type"] == "tool_result")
                    })
            });
        let compatible = if count {
            body.get("thinking").is_none() && body.get("output_config").is_none()
        } else if deepseek && with_tools {
            body["thinking"]["type"] == "disabled"
                && body["output_config"].get("effort").is_none()
                && body.get("reasoning_effort").is_none()
        } else if chat {
            body.get("thinking").is_none() && body["reasoning_effort"] == "high"
        } else {
            body["thinking"]["type"] == "adaptive"
                && body["output_config"]["effort"] == "high"
                && body["tools"]
                    .as_array()
                    .is_none_or(|tools| tools.iter().all(|tool| tool["type"] == "custom"))
        };
        if !compatible {
            return (StatusCode::BAD_REQUEST, Json(json!({"error":{"type":"invalid_request_error","message":"Thinking mode does not support this tool_choice"}}))).into_response();
        }
        if count {
            return Json(json!({"input_tokens":3})).into_response();
        }
        if chat {
            return Json(json!({"id":"test","model":"deepseek-flash","choices":[{"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}})).into_response();
        }
        Json(json!({"id":"test","type":"message","role":"assistant","model":"deepseek-flash","content":[{"type":"text","text":"OK"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}})).into_response()
    }
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec![
        "api.deepseek.com".into(),
        "api.example.com".into(),
        "api.deepseek.com.example.com".into(),
    ])
    .unwrap()
    .self_signed(&key)
    .unwrap();
    let (port, handle) = serve(
        Router::new().fallback(any(provider)),
        &cert.pem(),
        &key.serialize_pem(),
    )
    .await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap())
        .resolve("api.deepseek.com", ([127, 0, 0, 1], port).into())
        .resolve("api.example.com", ([127, 0, 0, 1], port).into())
        .resolve(
            "api.deepseek.com.example.com",
            ([127, 0, 0, 1], port).into(),
        )
        .build()
        .unwrap();
    for protocol in [UpstreamProtocol::Anthropic, UpstreamProtocol::OpenaiChat] {
        for (host, choice) in [
            ("api.deepseek.com", "tool"),
            ("api.deepseek.com", "any"),
            ("api.deepseek.com", "auto"),
            ("api.deepseek.com", "plain"),
            ("api.deepseek.com", "result"),
            ("api.deepseek.com", "default"),
            ("api.deepseek.com", "count"),
            ("api.example.com", "tool"),
            ("api.deepseek.com.example.com", "tool"),
        ] {
            if choice == "count" && protocol != UpstreamProtocol::Anthropic {
                continue;
            }
            let mut cfg = config();
            let path = if protocol == UpstreamProtocol::Anthropic {
                "/anthropic"
            } else {
                ""
            };
            cfg.base_url = format!("https://{host}:{port}{path}");
            cfg.upstream_protocol = protocol.clone();
            let app = GatewayRouter::with_client(
                cfg,
                "gateway-secret".into(),
                "provider-secret".into(),
                client.clone(),
            )
            .unwrap()
            .app();
            let mut body = request();
            body["thinking"] = json!({"type":"adaptive"});
            body["output_config"] = json!({"effort":"high"});
            body["tools"] = json!([{"type":"custom","name":"read","input_schema":{"type":"object","properties":{}}}]);
            body["tool_choice"] = json!({"type":choice});
            if choice == "plain" {
                body.as_object_mut().unwrap().remove("tools");
                body.as_object_mut().unwrap().remove("tool_choice");
            }
            if choice == "tool" {
                body["tool_choice"]["name"] = json!("read");
            }
            if choice == "result" {
                body.as_object_mut().unwrap().remove("tools");
                body.as_object_mut().unwrap().remove("tool_choice");
                body["messages"] = json!([
                    {"role":"user","content":"read"},
                    {"role":"assistant","content":[{"type":"tool_use","id":"a","name":"read","input":{}}]},
                    {"role":"user","content":[{"type":"tool_result","tool_use_id":"a","content":"3"}]}
                ]);
            }
            if choice == "default" {
                body.as_object_mut().unwrap().remove("thinking");
                body.as_object_mut().unwrap().remove("output_config");
                body["tool_choice"] = json!({"type":"auto"});
                body["tools"][0].as_object_mut().unwrap().remove("type");
            }
            if choice == "count" {
                body.as_object_mut().unwrap().remove("thinking");
                body.as_object_mut().unwrap().remove("output_config");
                body.as_object_mut().unwrap().remove("tool_choice");
            }
            let uri = if choice == "count" {
                "/v1/messages/count_tokens"
            } else {
                "/v1/messages"
            };
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(uri)
                        .header(header::HOST, "localhost:15724")
                        .header("x-api-key", "gateway-secret")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "provider={host}, protocol={protocol:?}, choice={choice}"
            );
        }
    }
    handle.shutdown();
}
