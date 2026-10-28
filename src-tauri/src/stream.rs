use crate::{
    config::UpstreamProtocol, gateway::GatewayError, protocol, providers, upstream_errors,
};
use bytes::Bytes;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const RECORD_LIMIT: usize = 4 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 32 * 1024 * 1024;

pub fn event(name: &str, payload: &Value) -> Bytes {
    Bytes::from(format!("event: {name}\ndata: {payload}\n\n"))
}

#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
    first: bool,
}
impl Decoder {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            first: true,
        }
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<(String, String)>, GatewayError> {
        self.buffer.extend_from_slice(bytes);
        let mut records = Vec::new();
        loop {
            let lf_boundary = self
                .buffer
                .windows(2)
                .position(|p| p == b"\n\n")
                .map(|p| (p, 2));
            let crlf_boundary = self
                .buffer
                .windows(4)
                .position(|p| p == b"\r\n\r\n")
                .map(|p| (p, 4));
            let boundary = match (lf_boundary, crlf_boundary) {
                (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
                (Some(boundary), None) | (None, Some(boundary)) => Some(boundary),
                (None, None) => None,
            };
            let Some((index, delimiter)) = boundary else {
                break;
            };
            if index > RECORD_LIMIT {
                return Err(GatewayError::upstream("SSE record exceeds 4 MiB"));
            }
            let record = String::from_utf8(self.buffer.drain(..index + delimiter).collect())
                .map_err(|_| GatewayError::upstream("Invalid upstream SSE encoding"))?;
            let record = if self.first {
                self.first = false;
                record.trim_start_matches('\u{feff}').to_string()
            } else {
                record
            };
            let mut name = "message".to_string();
            let mut data = Vec::new();
            for line in record.lines() {
                if line.starts_with(':') {
                    continue;
                }
                let (field, value) = line.split_once(':').unwrap_or((line, ""));
                let value = value.strip_prefix(' ').unwrap_or(value);
                match field {
                    "event" => name = value.to_string(),
                    "data" => data.push(value),
                    _ => (),
                }
            }
            if !data.is_empty() {
                records.push((name, data.join("\n")));
            }
        }
        if self.buffer.len() > RECORD_LIMIT {
            return Err(GatewayError::upstream("SSE record exceeds 4 MiB"));
        }
        Ok(records)
    }
    pub fn finish(&mut self) -> Result<Vec<(String, String)>, GatewayError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        self.push(b"\n\n")
    }
}

struct AnthropicStream {
    model: String,
    id: String,
    text_open: bool,
    text_seen: bool,
}
impl AnthropicStream {
    fn new(model: &str) -> Self {
        Self {
            model: model.into(),
            id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            text_open: false,
            text_seen: false,
        }
    }
    fn start(&self) -> Bytes {
        event(
            "message_start",
            &json!({"type":"message_start","message":{"id":self.id,"type":"message","role":"assistant","model":self.model,"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}}),
        )
    }
    fn text(&mut self, text: &str) -> Vec<Bytes> {
        if text.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if !self.text_open {
            self.text_open = true;
            self.text_seen = true;
            out.push(event("content_block_start",&json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})));
        }
        out.push(event("content_block_delta",&json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}})));
        out
    }
    fn finish(&mut self, tools: Vec<Value>, reason: &str, usage: Value) -> Vec<Bytes> {
        let mut out = Vec::new();
        if self.text_open {
            out.push(event(
                "content_block_stop",
                &json!({"type":"content_block_stop","index":0}),
            ));
            self.text_open = false;
        }
        for (offset, tool) in tools.into_iter().enumerate() {
            let index = offset + usize::from(self.text_seen);
            let mut start = tool.clone();
            start["input"] = json!({});
            out.push(event(
                "content_block_start",
                &json!({"type":"content_block_start","index":index,"content_block":start}),
            ));
            out.push(event("content_block_delta",&json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":tool["input"].to_string()}})));
            out.push(event(
                "content_block_stop",
                &json!({"type":"content_block_stop","index":index}),
            ));
        }
        out.push(event("message_delta",&json!({"type":"message_delta","delta":{"stop_reason":reason,"stop_sequence":null},"usage":usage})));
        out.push(event("message_stop", &json!({"type":"message_stop"})));
        out
    }
}

#[derive(Default)]
struct Call {
    id: String,
    name: String,
    arguments: String,
}

pub struct Adapter {
    kind: UpstreamProtocol,
    output: AnthropicStream,
    calls: BTreeMap<u64, Call>,
    finish: Option<String>,
    usage: Value,
    delivered: String,
    pub done: bool,
    required_tool: Option<String>,
}
impl Adapter {
    pub fn new(kind: UpstreamProtocol, model: &str) -> Self {
        Self {
            kind,
            output: AnthropicStream::new(model),
            calls: BTreeMap::new(),
            finish: None,
            usage: json!({}),
            delivered: String::new(),
            done: false,
            required_tool: None,
        }
    }
    pub fn require_tool(&mut self, name: String) {
        self.required_tool = Some(name);
    }
    pub fn start(&self) -> Vec<Bytes> {
        if self.kind == UpstreamProtocol::Anthropic {
            Vec::new()
        } else {
            vec![self.output.start()]
        }
    }
    pub fn feed(&mut self, name: &str, data: &str) -> Result<Vec<Bytes>, GatewayError> {
        if self.done {
            return Ok(Vec::new());
        }
        if data == "[DONE]" && self.kind == UpstreamProtocol::OpenaiChat {
            let finish = self
                .finish
                .clone()
                .filter(|s| {
                    matches!(
                        s.as_str(),
                        "stop" | "length" | "tool_calls" | "function_call"
                    )
                })
                .ok_or_else(|| {
                    GatewayError::upstream(
                        "Upstream stream ended without a successful terminal event",
                    )
                })?;
            let tools = self
                .calls
                .values()
                .map(|c| protocol::tool_block(&json!(c.id), &json!(c.name), &json!(c.arguments)))
                .collect::<Result<Vec<_>, _>>()?;
            let reason = if finish == "length" {
                "max_tokens"
            } else if tools.is_empty() {
                "end_turn"
            } else {
                "tool_use"
            };
            self.done = true;
            return Ok(self
                .output
                .finish(tools, reason, protocol::usage(&self.usage, true)));
        }
        let mut payload: Value = serde_json::from_str(data)
            .map_err(|_| GatewayError::upstream("Invalid upstream SSE JSON"))?;
        if payload.get("error").is_some_and(|v| !v.is_null()) || name == "error" {
            return Err(upstream_errors::streaming(&payload));
        }
        match self.kind {
            UpstreamProtocol::Anthropic => {
                if name == "message_start" {
                    payload["message"]["model"] = json!(self.output.model);
                }
                if name == "message_stop" {
                    self.done = true;
                }
                Ok(vec![event(name, &payload)])
            }
            UpstreamProtocol::OpenaiChat => {
                if payload["usage"].is_object() {
                    self.usage = payload["usage"].clone();
                }
                let choice = &payload["choices"][0];
                let delta = &choice["delta"];
                let mut out = Vec::new();
                for field in ["content", "refusal"] {
                    if let Some(text) = delta[field].as_str() {
                        self.delivered.push_str(text);
                        if self.delivered.len() > OUTPUT_LIMIT {
                            return Err(GatewayError::upstream("Stream text exceeds 32 MiB"));
                        }
                        out.extend(self.output.text(text));
                    }
                }
                for call in delta["tool_calls"].as_array().into_iter().flatten() {
                    let index = call["index"]
                        .as_u64()
                        .filter(|i| *i < 1024)
                        .ok_or_else(|| GatewayError::upstream("Invalid upstream tool index"))?;
                    let current = self.calls.entry(index).or_default();
                    if let Some(id) = call["id"].as_str().filter(|s| !s.is_empty()) {
                        if !current.id.is_empty() && current.id != id {
                            return Err(GatewayError::upstream("Upstream changed tool call id"));
                        }
                        current.id = id.into();
                    }
                    if let Some(name) = call["function"]["name"].as_str() {
                        current.name.push_str(name);
                    }
                    if let Some(arguments) = call["function"]["arguments"].as_str() {
                        current.arguments.push_str(arguments);
                    }
                    if current.arguments.len() > RECORD_LIMIT || current.name.len() > 4096 {
                        return Err(GatewayError::upstream(
                            "Upstream tool arguments exceed limit",
                        ));
                    }
                }
                if let Some(reason) = choice["finish_reason"].as_str() {
                    self.finish = Some(reason.into());
                }
                Ok(out)
            }
            UpstreamProtocol::OpenaiResponses => {
                let kind = payload["type"].as_str().unwrap_or(name);
                match kind {
                    "response.output_text.delta" | "response.refusal.delta" => {
                        let delta = payload["delta"].as_str().ok_or_else(|| {
                            GatewayError::upstream("Invalid Responses text delta")
                        })?;
                        self.delivered.push_str(delta);
                        if self.delivered.len() > OUTPUT_LIMIT {
                            return Err(GatewayError::upstream("Stream text exceeds 32 MiB"));
                        }
                        Ok(self.output.text(delta))
                    }
                    "error" | "response.failed" => Err(upstream_errors::streaming(&payload)),
                    "response.completed" | "response.incomplete" => {
                        let converted =
                            protocol::from_responses(&payload["response"], &self.output.model)?;
                        if let Some(required) = self.required_tool.as_deref() {
                            providers::ensure_required_function(&converted, required)?;
                        }
                        let content = converted["content"].as_array().unwrap();
                        let expected = content
                            .iter()
                            .filter(|p| p["type"] == "text")
                            .filter_map(|p| p["text"].as_str())
                            .collect::<String>();
                        let remainder =
                            expected.strip_prefix(&self.delivered).ok_or_else(|| {
                                GatewayError::upstream("Terminal text disagrees with streamed text")
                            })?;
                        let mut out = self.output.text(remainder);
                        let tools = content
                            .iter()
                            .filter(|p| p["type"] == "tool_use")
                            .cloned()
                            .collect();
                        out.extend(self.output.finish(
                            tools,
                            converted["stop_reason"].as_str().unwrap(),
                            converted["usage"].clone(),
                        ));
                        self.done = true;
                        Ok(out)
                    }
                    _ => Ok(Vec::new()),
                }
            }
        }
    }
    pub fn eof(&self) -> Result<(), GatewayError> {
        if self.done {
            Ok(())
        } else {
            Err(GatewayError::upstream(
                "Upstream stream ended without a terminal event",
            ))
        }
    }
}
