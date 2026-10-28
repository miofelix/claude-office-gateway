use crate::config::{GatewayConfig, UpstreamProtocol};
use crate::gateway::GatewayError;
use serde_json::{json, Value};

pub fn required_function(config: &GatewayConfig, incoming: &Value) -> Option<String> {
    if config.upstream_protocol != UpstreamProtocol::OpenaiResponses
        || config.upstream_model != "kimi-k3.1-pro"
        || url::Url::parse(&config.base_url)
            .ok()
            .is_none_or(|url| url.host_str() != Some("api.moonshot.cn"))
    {
        return None;
    }
    match incoming["tool_choice"]["type"].as_str() {
        Some("tool") => incoming["tool_choice"]["name"].as_str().map(str::to_owned),
        Some("any") => Some(String::new()),
        _ => None,
    }
}

pub fn ensure_required_function(reply: &Value, required: &str) -> Result<(), GatewayError> {
    if reply["content"].as_array().is_some_and(|blocks| {
        blocks.iter().any(|block| {
            block["type"] == "tool_use" && (required.is_empty() || block["name"] == required)
        })
    }) {
        Ok(())
    } else {
        Err(GatewayError::upstream("The provider finished without the requested tool call. Retry the action after checking the provider model."))
    }
}

pub fn prepare_request(
    config: &GatewayConfig,
    incoming: &Value,
    outgoing: &mut Value,
    count: bool,
) {
    if let Some(required) = required_function(config, incoming) {
        let instruction = if !required.is_empty() {
            let name = required.as_str();
            if let Some(tools) = outgoing.get_mut("tools").and_then(Value::as_array_mut) {
                tools.retain(|tool| tool["name"] == name);
            }
            format!("For this turn, call the function {name} exactly once. Do not provide a final answer before calling that function.")
        } else {
            "For this turn, call one of the provided functions. Do not provide a final answer before calling a function.".into()
        };
        // Kimi k3.1-pro rejects named and required tool choices while thinking
        // is enabled. Auto plus an explicit instruction is best effort; the
        // response must still be checked for the actual function call.
        outgoing["tool_choice"] = json!("auto");
        outgoing["parallel_tool_calls"] = json!(false);
        if let Some(input) = outgoing.get_mut("input").and_then(Value::as_array_mut) {
            input.insert(
                0,
                json!({"role":"system","content":[{"type":"input_text",
                "text":instruction}]}),
            );
        }
        return;
    }
    if url::Url::parse(&config.base_url)
        .ok()
        .is_none_or(|url| url.host_str() != Some("api.deepseek.com"))
        || !uses_tools(incoming)
    {
        return;
    }
    // DeepSeek thinking rejects forced tool_choice and requires thinking blocks
    // on later tool turns. Office may omit those blocks, so keep the entire
    // tool conversation in non-thinking mode, including tool-result requests.
    match config.upstream_protocol {
        UpstreamProtocol::Anthropic => {
            // Anthropic now labels function tools as "custom". DeepSeek's
            // native endpoint only accepts these tools when type is omitted.
            if let Some(tools) = outgoing.get_mut("tools").and_then(Value::as_array_mut) {
                for tool in tools {
                    if tool["type"] == "custom" {
                        tool.as_object_mut().unwrap().remove("type");
                    }
                }
            }
            if count {
                return;
            }
            outgoing["thinking"] = json!({"type":"disabled"});
            if let Some(output) = outgoing
                .get_mut("output_config")
                .and_then(Value::as_object_mut)
            {
                output.remove("effort");
                if output.is_empty() {
                    outgoing.as_object_mut().unwrap().remove("output_config");
                }
            }
        }
        UpstreamProtocol::OpenaiChat => {
            outgoing["thinking"] = json!({"type":"disabled"});
            outgoing.as_object_mut().unwrap().remove("reasoning_effort");
        }
        UpstreamProtocol::OpenaiResponses => (),
    }
}

fn uses_tools(body: &Value) -> bool {
    body["tools"]
        .as_array()
        .is_some_and(|tools| !tools.is_empty())
        || body["messages"].as_array().is_some_and(|messages| {
            messages.iter().any(|message| {
                message["content"].as_array().is_some_and(|blocks| {
                    blocks.iter().any(|block| {
                        matches!(block["type"].as_str(), Some("tool_use" | "tool_result"))
                    })
                })
            })
        })
}
