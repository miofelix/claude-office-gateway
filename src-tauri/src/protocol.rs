use crate::gateway::GatewayError;
use serde_json::{json, Value};
use std::collections::HashSet;

type Result<T> = std::result::Result<T, GatewayError>;
fn invalid(text: &str) -> GatewayError {
    GatewayError::bad_request(text)
}
fn nonempty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::String(s) => !s.is_empty(),
        Value::Bool(b) => *b,
        _ => true,
    }
}
fn parts(value: &Value) -> Result<Vec<Value>> {
    match value {
        Value::String(s) => Ok(vec![json!({"type":"text","text":s})]),
        Value::Array(a) if a.iter().all(Value::is_object) => Ok(a.clone()),
        _ => Err(invalid(
            "Content must be text or an array of content blocks",
        )),
    }
}
fn text(block: &Value) -> Result<&str> {
    block["text"]
        .as_str()
        .ok_or_else(|| invalid("Text block requires a string"))
}
fn image(block: &Value) -> Result<Value> {
    let source = &block["source"];
    let url = match source["type"].as_str() {
        Some("base64") => {
            let mime = source["media_type"]
                .as_str()
                .filter(|s| s.starts_with("image/"))
                .ok_or_else(|| invalid("Image requires media_type"))?;
            let data = source["data"]
                .as_str()
                .ok_or_else(|| invalid("Image requires base64 data"))?;
            format!("data:{mime};base64,{data}")
        }
        Some("url") => {
            let url = source["url"]
                .as_str()
                .ok_or_else(|| invalid("Image requires HTTPS URL"))?;
            if url::Url::parse(url)
                .map(|u| u.scheme() != "https")
                .unwrap_or(true)
            {
                return Err(invalid("Use base64 image data or an HTTPS image URL"));
            }
            url.to_string()
        }
        _ => return Err(invalid("Use base64 image data or an HTTPS image URL")),
    };
    Ok(json!({"type":"image_url","image_url":{"url":url}}))
}

pub fn validate(body: &Value) -> Result<()> {
    let messages = body["messages"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| invalid("messages must be nonempty"))?;
    if body["max_tokens"].as_u64().is_none_or(|v| v == 0) {
        return Err(invalid("max_tokens must be a positive integer"));
    }
    if body.get("stream").is_some_and(|v| !v.is_boolean()) {
        return Err(invalid("stream must be a boolean"));
    }
    for message in messages {
        if !matches!(message["role"].as_str(), Some("user" | "assistant")) {
            return Err(invalid("Every message must have role user or assistant"));
        }
        parts(&message["content"])?;
    }
    Ok(())
}

fn history(body: &Value) -> Result<Vec<Value>> {
    let mut output = Vec::new();
    if nonempty(&body["system"]) {
        let system = parts(&body["system"])?;
        if system.iter().any(|p| p["type"] != "text") {
            return Err(invalid("system must contain text blocks only"));
        }
        let texts = system.iter().map(text).collect::<Result<Vec<_>>>()?;
        output.push(json!({"role":"system","content":texts.join("\n")}));
    }
    let mut pending = HashSet::<String>::new();
    let mut seen = HashSet::<String>::new();
    for message in body["messages"].as_array().unwrap() {
        let blocks = parts(&message["content"])?;
        if message["role"] == "assistant" {
            if !pending.is_empty() {
                return Err(invalid("Missing tool_result before another assistant turn"));
            }
            let mut texts = Vec::new();
            let mut calls = Vec::new();
            for block in &blocks {
                match block["type"].as_str() {
                    Some("text") => texts.push(text(block)?.to_string()),
                    Some("tool_use") => {
                        let id = block["id"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .ok_or_else(|| invalid("tool_use requires a unique id"))?;
                        let name = block["name"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .ok_or_else(|| invalid("tool_use requires a name"))?;
                        let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                        if !input.is_object() || !seen.insert(id.to_string()) {
                            return Err(invalid(
                                "tool_use requires a unique id, name and object input",
                            ));
                        }
                        pending.insert(id.to_string());
                        calls.push(json!({"id":id,"type":"function","function":{"name":name,"arguments":input.to_string()}}));
                    }
                    Some("thinking" | "redacted_thinking") => (),
                    _ => return Err(invalid("Unsupported assistant content block")),
                }
            }
            let mut entry = json!({"role":"assistant","content":if texts.is_empty() {Value::Null} else {json!(texts.join("\n"))}});
            if !calls.is_empty() {
                entry["tool_calls"] = json!(calls);
            }
            output.push(entry);
        } else {
            let mut ordinary = Vec::new();
            let mut promoted = Vec::new();
            for block in &blocks {
                match block["type"].as_str() {
                    Some("tool_result") => {
                        let id = block["tool_use_id"]
                            .as_str()
                            .ok_or_else(|| invalid("tool_result requires tool_use_id"))?;
                        if !pending.remove(id) {
                            return Err(invalid("tool_result must match an outstanding tool_use"));
                        }
                        let content = block.get("content").cloned().unwrap_or_else(|| json!(""));
                        let mut texts = Vec::new();
                        let mut images = Vec::new();
                        for part in parts(&content)? {
                            match part["type"].as_str() {
                                Some("text") => texts.push(text(&part)?.to_string()),
                                Some("image") => images.push(image(&part)?),
                                _ => {
                                    return Err(invalid(
                                        "Only text and images are supported in tool_result",
                                    ))
                                }
                            }
                        }
                        if block.get("is_error").is_some_and(|v| !v.is_boolean()) {
                            return Err(invalid("tool_result.is_error must be a boolean"));
                        }
                        let prefix = if block["is_error"] == true {
                            "[Tool error]\n"
                        } else {
                            ""
                        };
                        output.push(json!({"role":"tool","tool_call_id":id,"content":format!("{prefix}{}",texts.join("\n"))}));
                        if !images.is_empty() {
                            promoted.push(json!({"type":"text","text":format!("Images from tool result {id}: ")}));
                            promoted.extend(images);
                        }
                    }
                    Some("text") => ordinary.push(json!({"type":"text","text":text(block)?})),
                    Some("image") => ordinary.push(image(block)?),
                    _ => return Err(invalid("Unsupported user content block")),
                }
            }
            if !pending.is_empty() {
                return Err(invalid(
                    "Return all outstanding tool calls in one user turn",
                ));
            }
            ordinary.extend(promoted);
            if !ordinary.is_empty() {
                output.push(json!({"role":"user","content":ordinary}));
            }
        }
    }
    if !pending.is_empty() {
        return Err(invalid("Unanswered tool_use at end of history"));
    }
    Ok(output)
}

fn effort(body: &Value) -> Result<Option<&str>> {
    for field in ["output_config", "thinking"] {
        if body.get(field).is_some_and(|v| !v.is_object()) {
            return Err(invalid("output_config and thinking must be objects"));
        }
    }
    if nonempty(&body["output_config"]["format"]) {
        return Err(invalid(
            "Structured output_config.format is not implemented",
        ));
    }
    if let Some(effort) = body["output_config"].get("effort") {
        return match effort.as_str() {
            Some("low") => Ok(Some("low")),
            Some("medium") => Ok(Some("medium")),
            Some("high" | "max") => Ok(Some("high")),
            _ => Err(invalid("Unsupported output_config.effort")),
        };
    }
    match body["thinking"]["type"].as_str() {
        Some("adaptive") => Ok(Some("high")),
        Some("enabled") => {
            let budget = body["thinking"]
                .get("budget_tokens")
                .cloned()
                .unwrap_or(json!(1024))
                .as_u64()
                .ok_or_else(|| invalid("thinking.budget_tokens must be a nonnegative integer"))?;
            Ok(Some(if budget < 4096 {
                "low"
            } else if budget < 16384 {
                "medium"
            } else {
                "high"
            }))
        }
        _ => Ok(None),
    }
}

pub fn to_chat(body: &Value, model: &str, token_field: &str) -> Result<Value> {
    validate(body)?;
    for field in [
        "container",
        "mcp_servers",
        "context_management",
        "safeguards",
    ] {
        if nonempty(&body[field]) {
            return Err(invalid(&format!(
                "{field} requires a native Anthropic backend"
            )));
        }
    }
    let mut request = json!({"model":model,"messages":history(body)?,token_field:body["max_tokens"],"stream":body["stream"].as_bool().unwrap_or(false)});
    for (source, target) in [
        ("temperature", "temperature"),
        ("top_p", "top_p"),
        ("stop_sequences", "stop"),
    ] {
        if let Some(value) = body.get(source) {
            request[target] = value.clone();
        }
    }
    let mut names = HashSet::new();
    if let Some(tools) = body.get("tools") {
        let tools = tools
            .as_array()
            .ok_or_else(|| invalid("tools must be an array"))?;
        let mut converted = Vec::new();
        for tool in tools {
            let name = tool["name"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 64)
                .ok_or_else(|| invalid("Tools require a name and input_schema"))?;
            if (tool.get("type").is_some() && tool["type"] != "custom")
                || !tool["input_schema"].is_object()
                || !names.insert(name.to_string())
            {
                return Err(invalid("OpenAI adapters require uniquely named client function tools with input_schema"));
            }
            converted.push(json!({"type":"function","function":{"name":name,"description":tool["description"].as_str().unwrap_or(""),"parameters":tool["input_schema"]}}));
        }
        if !converted.is_empty() {
            request["tools"] = json!(converted);
        }
    }
    if let Some(choice) = body.get("tool_choice") {
        request["tool_choice"] = match choice["type"].as_str() {
            Some("tool") => {
                let name = choice["name"]
                    .as_str()
                    .filter(|name| names.contains(*name))
                    .ok_or_else(|| invalid("Named tool_choice must refer to a declared tool"))?;
                json!({"type":"function","function":{"name":name}})
            }
            Some("auto") => json!("auto"),
            Some("any") => json!("required"),
            Some("none") => json!("none"),
            _ => return Err(invalid("Unsupported tool_choice")),
        };
        if choice["disable_parallel_tool_use"] == true {
            request["parallel_tool_calls"] = json!(false);
        }
    }
    if let Some(effort) = effort(body)? {
        request["reasoning_effort"] = json!(effort);
    }
    if request["stream"] == true {
        request["stream_options"] = json!({"include_usage":true});
    }
    Ok(request)
}

pub fn to_responses(body: &Value, model: &str) -> Result<Value> {
    let chat = to_chat(body, model, "max_completion_tokens")?;
    let mut inputs = Vec::new();
    for message in chat["messages"].as_array().unwrap() {
        let role = message["role"].as_str().unwrap();
        if role == "tool" {
            inputs.push(json!({"type":"function_call_output","call_id":message["tool_call_id"],"output":message["content"]}));
            continue;
        }
        if nonempty(&message["content"]) {
            let mut content = Vec::new();
            for part in parts(&message["content"])? {
                content.push(if part["type"]=="image_url" {json!({"type":"input_image","image_url":part["image_url"]["url"],"detail":"auto"})} else {json!({"type":if role=="assistant" {"output_text"} else {"input_text"},"text":text(&part)?})});
            }
            inputs.push(json!({"role":role,"content":content}));
        }
        for call in message["tool_calls"].as_array().into_iter().flatten() {
            inputs.push(json!({"type":"function_call","call_id":call["id"],"name":call["function"]["name"],"arguments":call["function"]["arguments"]}));
        }
    }
    if nonempty(&chat["stop"]) {
        return Err(invalid("Responses adapter does not support stop_sequences"));
    }
    let mut request = json!({"model":model,"input":inputs,"max_output_tokens":body["max_tokens"],"stream":chat["stream"],"store":false});
    for field in ["temperature", "top_p", "parallel_tool_calls"] {
        if let Some(value) = chat.get(field) {
            request[field] = value.clone();
        }
    }
    if let Some(tools) = chat["tools"].as_array() {
        request["tools"] = json!(tools
            .iter()
            .map(|tool| {
                let mut function = tool["function"].clone();
                function["type"] = json!("function");
                function["strict"] = json!(false);
                function
            })
            .collect::<Vec<_>>());
    }
    if let Some(choice) = chat.get("tool_choice") {
        request["tool_choice"] = if choice.is_object() {
            json!({"type":"function","name":choice["function"]["name"]})
        } else {
            choice.clone()
        };
    }
    if let Some(effort) = chat.get("reasoning_effort") {
        request["reasoning"] = json!({"effort":effort});
    }
    Ok(request)
}

pub fn tool_block(id: &Value, name: &Value, arguments: &Value) -> Result<Value> {
    let id = id
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| GatewayError::upstream("Upstream tool call is missing id"))?;
    let name = name
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| GatewayError::upstream("Upstream tool call is missing name"))?;
    let input = if let Some(s) = arguments.as_str() {
        serde_json::from_str(s)
            .map_err(|_| GatewayError::upstream("Invalid upstream tool arguments"))?
    } else {
        arguments.clone()
    };
    if !input.is_object() {
        return Err(GatewayError::upstream(
            "Upstream tool arguments must be an object",
        ));
    }
    Ok(json!({"type":"tool_use","id":id,"name":name,"input":input}))
}
pub fn usage(raw: &Value, chat: bool) -> Value {
    json!({"input_tokens":raw[if chat {"prompt_tokens"} else {"input_tokens"}].as_u64().unwrap_or(0),"output_tokens":raw[if chat {"completion_tokens"} else {"output_tokens"}].as_u64().unwrap_or(0)})
}
pub fn message(model: &str, content: Vec<Value>, reason: &str, usage: Value) -> Value {
    json!({"id":format!("msg_{}",uuid::Uuid::new_v4().simple()),"type":"message","role":"assistant","model":model,"content":content,"stop_reason":reason,"stop_sequence":null,"usage":usage})
}
pub fn from_chat(response: &Value, model: &str) -> Result<Value> {
    let choice = response["choices"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| GatewayError::upstream("Invalid upstream choices"))?;
    let finish = choice["finish_reason"]
        .as_str()
        .filter(|s| matches!(*s, "stop" | "length" | "tool_calls" | "function_call"))
        .ok_or_else(|| GatewayError::upstream("Upstream completion did not finish successfully"))?;
    if !choice["message"].is_object() {
        return Err(GatewayError::upstream("Invalid upstream message"));
    }
    let mut content = Vec::new();
    for field in ["content", "refusal"] {
        if let Some(t) = choice["message"][field].as_str().filter(|s| !s.is_empty()) {
            content.push(json!({"type":"text","text":t}));
        }
    }
    if let Some(calls) = choice["message"].get("tool_calls").filter(|v| !v.is_null()) {
        for call in calls
            .as_array()
            .ok_or_else(|| GatewayError::upstream("Invalid upstream tool_calls"))?
        {
            content.push(tool_block(
                &call["id"],
                &call["function"]["name"],
                &call["function"]["arguments"],
            )?);
        }
    }
    let reason = if finish == "length" {
        "max_tokens"
    } else if content.iter().any(|p| p["type"] == "tool_use") {
        "tool_use"
    } else {
        "end_turn"
    };
    Ok(message(
        model,
        content,
        reason,
        usage(&response["usage"], true),
    ))
}
pub fn from_responses(response: &Value, model: &str) -> Result<Value> {
    let status = response["status"].as_str().unwrap_or("");
    if status != "completed"
        && !(status == "incomplete"
            && response["incomplete_details"]["reason"] == "max_output_tokens")
    {
        return Err(GatewayError::upstream(
            "Upstream Responses request did not complete successfully",
        ));
    }
    let output = response["output"]
        .as_array()
        .ok_or_else(|| GatewayError::upstream("Invalid Responses output"))?;
    let mut content = Vec::new();
    for item in output {
        match item["type"].as_str() {
            Some("function_call") => content.push(tool_block(
                &item["call_id"],
                &item["name"],
                &item["arguments"],
            )?),
            Some("message") => {
                for part in item["content"].as_array().into_iter().flatten() {
                    match part["type"].as_str() {
                        Some("output_text") => {
                            if let Some(t) = part["text"].as_str() {
                                content.push(json!({"type":"text","text":t}));
                            }
                        }
                        Some("refusal") => {
                            if let Some(t) = part["refusal"].as_str() {
                                content.push(json!({"type":"text","text":t}));
                            }
                        }
                        _ => return Err(GatewayError::upstream("Unsupported Responses content")),
                    }
                }
            }
            Some("reasoning") => (),
            _ => {
                return Err(GatewayError::upstream(
                    "Unsupported upstream hosted tool output",
                ))
            }
        }
    }
    let reason = if status == "incomplete" {
        "max_tokens"
    } else if content.iter().any(|p| p["type"] == "tool_use") {
        "tool_use"
    } else {
        "end_turn"
    };
    Ok(message(
        model,
        content,
        reason,
        usage(&response["usage"], false),
    ))
}
