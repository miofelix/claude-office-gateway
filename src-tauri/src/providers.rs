use crate::config::{GatewayConfig, UpstreamProtocol};
use serde_json::{json, Value};

pub fn prepare_request(
    config: &GatewayConfig,
    incoming: &Value,
    outgoing: &mut Value,
    count: bool,
) {
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
