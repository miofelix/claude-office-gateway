use crate::{
    certificate,
    config::{GatewayConfig, UpstreamProtocol},
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
fn migrates_swift_config_without_losing_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path,json!({"baseURL":"https://provider.example/v1","upstreamModel":"saved-model","upstreamProtocol":"anthropic","publicModel":"office-saved","displayName":"Saved","authHeader":"x-api-key","maxTokensField":"max_tokens","timeoutSeconds":45,"allowedOrigins":["https://pivot.claude.ai"]}).to_string()).unwrap();
    let saved = GatewayConfig::load(&path).unwrap();
    assert_eq!(saved.upstream_model, "saved-model");
    assert_eq!(saved.public_model, "office-saved");
    assert_eq!(saved.timeout_seconds, 45);
    saved.save(&path).unwrap();
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
