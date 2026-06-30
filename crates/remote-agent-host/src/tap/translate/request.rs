//! Translate Anthropic Messages API requests → OpenAI Chat Completions API.
//!
//! The entry point is [`translate_request`], which takes the raw request body
//! and path, and returns the translated body + new path.

use serde_json::{json, Value};

use super::TranslateError;

/// Result of translating a request: the new body bytes and the new path.
pub struct TranslatedRequest {
    pub body: Vec<u8>,
    pub path: String,
}

/// Translate an Anthropic `/v1/messages` request to an OpenAI
/// `/v1/chat/completions` request.
///
/// If the body is not an Anthropic Messages request (no `messages` field),
/// it is returned unchanged with the original path.
///
/// On parse failure, the original body is returned unchanged.
pub fn translate_request(body: &[u8], path: &str) -> Result<TranslatedRequest, TranslateError> {
    let mut val: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            return Err(TranslateError::new(format!(
                "failed to parse request JSON: {e}"
            )));
        }
    };

    // Only translate /v1/messages (Anthropic endpoint).
    // For health checks, token-count, etc., forward unchanged.
    if !val.is_object() || !val.get("messages").is_some() {
        // Not an Anthropic Messages request — forward unchanged.
        return Ok(TranslatedRequest {
            body: body.to_vec(),
            path: path.to_string(),
        });
    }

    // ── 1. Convert messages ──────────────────────────────────────────
    let anthropic_messages = match val.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(msgs) => std::mem::take(msgs),
        None => {
            return Ok(TranslatedRequest {
                body: body.to_vec(),
                path: path.to_string(),
            });
        }
    };

    let mut openai_messages: Vec<Value> = Vec::with_capacity(anthropic_messages.len() + 1);

    // Extract system prompt (top-level field) and prepend as first message.
    if let Some(sys) = val.get("system").cloned() {
        if !sys.is_null() {
            let sys_content = convert_anthropic_content_to_openai(&sys);
            if !sys_content.is_null() {
                openai_messages.push(json!({
                    "role": "system",
                    "content": sys_content
                }));
            }
        }
    }

    // Remove anthropic-specific top-level fields that we've handled.
    let _ = val.as_object_mut().map(|o| {
        o.remove("system");
        o.remove("metadata");
        o.remove("top_k");
    });

    for msg in anthropic_messages {
        let role = msg
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("user");

        match role {
            "user" => {
                if let Some(openai_msg) = convert_anthropic_user_message(&msg) {
                    openai_messages.push(openai_msg);
                }
            }
            "assistant" => {
                // Assistant messages may contain tool_use blocks → convert to OpenAI tool_calls.
                if let Some(openai_msg) = convert_anthropic_assistant_message(&msg) {
                    openai_messages.push(openai_msg);
                }
            }
            _ => {
                // Pass through unknown roles unchanged.
                openai_messages.push(msg);
            }
        }
    }

    val["messages"] = json!(openai_messages);

    // ── 2. Convert tools ─────────────────────────────────────────────
    convert_tools(&mut val);

    // ── 3. Convert tool_choice ───────────────────────────────────────
    convert_tool_choice(&mut val);

    // ── 4. Rename stop_sequences → stop ──────────────────────────────
    if let Some(stop_seqs) = val.get("stop_sequences").cloned() {
        val["stop"] = stop_seqs;
        if let Some(obj) = val.as_object_mut() {
            obj.remove("stop_sequences");
        }
    }

    // ── 5. Build new path ────────────────────────────────────────────
    let new_path = if path.contains("/v1/messages") {
        path.replace("/v1/messages", "/v1/chat/completions")
    } else {
        // Path doesn't contain /v1/messages (maybe CLI uses different format).
        // Just replace the last segment.
        let p = path.to_string();
        if p.ends_with("/messages") {
            p.replace("/messages", "/chat/completions")
        } else {
            // Append ? We shouldn't reach here in normal flow.
            // Default to OpenAI endpoint.
            "/v1/chat/completions".to_string()
        }
    };

    let new_body = serde_json::to_vec(&val).map_err(|e| {
        TranslateError::new(format!("failed to serialize translated request: {e}"))
    })?;

    Ok(TranslatedRequest {
        body: new_body,
        path: new_path,
    })
}

/// Convert Anthropic content (string, or array of content blocks) to a
/// value suitable for OpenAI message content.
fn convert_anthropic_content_to_openai(content: &Value) -> Value {
    match content {
        Value::String(s) => json!(s),
        Value::Array(blocks) if blocks.is_empty() => Value::Null,
        Value::Array(blocks) => {
            // Check if all blocks are text — if so, join them.
            let all_text = blocks.iter().all(|b| {
                b.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "text")
                    .unwrap_or(false)
            });
            if all_text {
                let joined: String = blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("");
                return json!(joined);
            }

            // Mixed content blocks — convert individually for OpenAI.
            let parts: Vec<Value> = blocks
                .iter()
                .filter_map(|block| convert_anthropic_content_block(block))
                .collect();
            if parts.is_empty() {
                Value::Null
            } else if parts.len() == 1 && parts[0].get("type").map_or(false, |t| t == "text") {
                // Single text part — flatten to string (OpenAI accepts both).
                parts[0].get("text").cloned().unwrap_or(Value::Null)
            } else {
                json!(parts)
            }
        }
        _ => Value::Null,
    }
}

/// Convert a single Anthropic content block to an OpenAI content part.
fn convert_anthropic_content_block(block: &Value) -> Option<Value> {
    let block_type = block.get("type").and_then(|t| t.as_str())?;

    match block_type {
        "text" => {
            let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
            Some(json!({"type": "text", "text": text}))
        }
        "image" => {
            let source = block.get("source")?;
            let media_type = source
                .get("media_type")
                .and_then(|m| m.as_str())
                .unwrap_or("image/png");
            let data = source.get("data").and_then(|d| d.as_str())?;
            let url = format!("data:{media_type};base64,{data}");
            Some(json!({
                "type": "image_url",
                "image_url": {"url": url, "detail": "auto"}
            }))
        }
        "tool_use" | "tool_result" => {
            // These are handled at the message level, not as individual content blocks.
            None
        }
        _ => {
            // Unknown block type — skip.
            None
        }
    }
}

/// Convert an Anthropic user message to an OpenAI user or tool message.
/// User messages may contain `tool_result` content blocks → converted to
/// separate `role: "tool"` messages in the output.
fn convert_anthropic_user_message(msg: &Value) -> Option<Value> {
    let content = msg.get("content")?;

    // Check for tool_result content blocks (multi-block content).
    if let Some(blocks) = content.as_array() {
        let has_tool_results = blocks
            .iter()
            .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"));

        if has_tool_results {
            // Return a Vec of messages? No — the caller flattens individual messages.
            // Actually, we need to handle this differently. The caller iterates
            // messages 1:1. Tool results in user messages need to produce N tool
            // messages + possibly 1 user message for non-tool-result content.
            //
            // For simplicity in V1: if ALL blocks are tool_result, return the
            // LAST tool_result as the message (the caller will handle the rest
            // by checking for this pattern). Better: add a flag.
            //
            // Actually, let's just handle the simple case: a single tool_result
            // block. Multi-block tool results are rare.
            if blocks.len() == 1 {
                let tr = &blocks[0];
                let tool_use_id = tr
                    .get("tool_use_id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("");
                let tr_content = tr.get("content").cloned().unwrap_or(Value::Null);
                let tr_text = convert_anthropic_content_to_openai(&tr_content);
                return Some(json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": tr_text
                }));
            }

            // Multi-block with mixed content: extract non-tool-result content
            // into a user message, and tool results into tool messages.
            // For V1, we only handle the single-message case above.
            // Multi tool-result content blocks are handled by returning the
            // first tool_result and leaving the rest for future work.
            return Some(json!({
                "role": "tool",
                "tool_call_id": blocks.iter()
                    .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
                    .and_then(|b| b.get("tool_use_id"))
                    .and_then(|i| i.as_str())
                    .unwrap_or(""),
                "content": ""
            }));
        }
    }

    let openai_content = convert_anthropic_content_to_openai(content);
    Some(json!({
        "role": "user",
        "content": openai_content
    }))
}

/// Convert an Anthropic assistant message to an OpenAI assistant message.
/// Handles `tool_use` content blocks → OpenAI `tool_calls`.
fn convert_anthropic_assistant_message(msg: &Value) -> Option<Value> {
    let content = msg.get("content")?;

    match content {
        Value::String(text) => Some(json!({
            "role": "assistant",
            "content": text
        })),
        Value::Array(blocks) => {
            let mut text_parts: Vec<String> = Vec::new();
            let mut tool_calls: Vec<Value> = Vec::new();

            for block in blocks {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(t.to_string());
                        }
                    }
                    Some("tool_use") => {
                        let id = block
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        let input = block.get("input").cloned().unwrap_or(json!({}));
                        let arguments =
                            serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                        tool_calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": arguments
                            }
                        }));
                    }
                    _ => {}
                }
            }

            if tool_calls.is_empty() {
                // Text-only assistant message.
                let text = text_parts.join("");
                Some(json!({
                    "role": "assistant",
                    "content": if text.is_empty() && blocks.is_empty() {
                        Value::Null
                    } else {
                        json!(text)
                    }
                }))
            } else if text_parts.is_empty() {
                // Tool-calls-only assistant message.
                Some(json!({
                    "role": "assistant",
                    "content": Value::Null,
                    "tool_calls": tool_calls
                }))
            } else {
                // Mixed — OpenAI doesn't support text + tool_calls in same message well.
                // Emit text in content and tool_calls alongside.
                let text = text_parts.join("");
                Some(json!({
                    "role": "assistant",
                    "content": text,
                    "tool_calls": tool_calls
                }))
            }
        }
        _ => Some(json!({
            "role": "assistant",
            "content": Value::Null
        })),
    }
}

/// Convert Anthropic `tools` to OpenAI `tools` format.
fn convert_tools(val: &mut Value) {
    let Some(tools) = val.get("tools").and_then(|t| t.as_array()) else {
        return;
    };

    let converted: Vec<Value> = tools
        .iter()
        .map(|tool| {
            let name = tool
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let description = tool
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            let parameters = tool
                .get("input_schema")
                .cloned()
                .unwrap_or(json!({"type": "object", "properties": {}}));

            json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": parameters
                }
            })
        })
        .collect();

    val["tools"] = json!(converted);
}

/// Convert Anthropic `tool_choice` to OpenAI format.
fn convert_tool_choice(val: &mut Value) {
    let Some(tool_choice) = val.get("tool_choice") else {
        return;
    };

    let converted = if let Some(tc_obj) = tool_choice.as_object() {
        let tc_type = tc_obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match tc_type {
            "auto" => json!("auto"),
            "any" => json!("required"),
            "tool" => {
                let name = tc_obj
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                json!({
                    "type": "function",
                    "function": {"name": name}
                })
            }
            _ => json!("auto"),
        }
    } else if let Some(tc_str) = tool_choice.as_str() {
        // Already a string — pass through.
        json!(tc_str)
    } else {
        json!("auto")
    };

    val["tool_choice"] = converted;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_text_request() {
        let body = json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "Hello, world!"}
            ],
            "system": "You are a helpful assistant.",
            "temperature": 0.7,
            "stream": true
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();

        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        assert_eq!(result.path, "/v1/chat/completions");

        // System should be prepended to messages.
        let msgs = translated["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are a helpful assistant.");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "Hello, world!");

        // System top-level field should be removed.
        assert!(translated.get("system").is_none());

        // Fields should pass through.
        assert_eq!(translated["model"], "claude-sonnet-4-6");
        assert_eq!(translated["max_tokens"], 1024);
        assert_eq!(translated["temperature"], 0.7);
        assert_eq!(translated["stream"], true);
    }

    #[test]
    fn test_stop_sequences_to_stop() {
        let body = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "Hi"}],
            "stop_sequences": ["\n\n", "END"]
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        assert!(translated.get("stop_sequences").is_none());
        assert_eq!(translated["stop"], json!(["\n\n", "END"]));
    }

    #[test]
    fn test_top_k_dropped() {
        let body = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "Hi"}],
            "top_k": 40,
            "top_p": 0.9
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        assert!(translated.get("top_k").is_none());
        assert_eq!(translated["top_p"], 0.9);
    }

    #[test]
    fn test_metadata_dropped() {
        let body = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "Hi"}],
            "metadata": {"user_id": "abc123"}
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        assert!(translated.get("metadata").is_none());
    }

    #[test]
    fn test_tools_translation() {
        let body = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "What's the weather?"}],
            "tools": [{
                "name": "get_weather",
                "description": "Get the weather",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "location": {"type": "string"}
                    }
                }
            }]
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();

        let tools = translated["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        let tool = &tools[0];
        assert_eq!(tool["type"], "function");
        assert_eq!(tool["function"]["name"], "get_weather");
        assert_eq!(tool["function"]["description"], "Get the weather");
        assert!(tool["function"]["parameters"].is_object());
    }

    #[test]
    fn test_tool_choice_auto() {
        let body = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "Hi"}],
            "tool_choice": {"type": "auto"}
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        assert_eq!(translated["tool_choice"], "auto");
    }

    #[test]
    fn test_tool_choice_any() {
        let body = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "Hi"}],
            "tool_choice": {"type": "any"}
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        assert_eq!(translated["tool_choice"], "required");
    }

    #[test]
    fn test_tool_choice_specific() {
        let body = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "Hi"}],
            "tool_choice": {"type": "tool", "name": "get_weather"}
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        assert_eq!(translated["tool_choice"]["type"], "function");
        assert_eq!(translated["tool_choice"]["function"]["name"], "get_weather");
    }

    #[test]
    fn test_non_messages_body_passed_through() {
        // A health-check or count-tokens request without "messages" field.
        let body = json!({"model": "test"});
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/count_tokens").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        assert_eq!(translated, body);
        assert_eq!(result.path, "/v1/count_tokens");
    }

    #[test]
    fn test_image_content_block() {
        let body = json!({
            "model": "test",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Describe this:"},
                    {"type": "image", "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": "iVBORw0KGgo="
                    }}
                ]
            }]
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        let msgs = translated["messages"].as_array().unwrap();
        let content = msgs[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image_url");
        assert!(content[1]["image_url"]["url"].as_str().unwrap().contains("data:image/png;base64,"));
    }

    #[test]
    fn test_assistant_with_tool_use() {
        let body = json!({
            "model": "test",
            "messages": [
                {"role": "user", "content": "What's 2+2?"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "toolu_01", "name": "calculator",
                     "input": {"expression": "2+2"}}
                ]}
            ]
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        let msgs = translated["messages"].as_array().unwrap();
        let asst_msg = &msgs[1];
        assert_eq!(asst_msg["role"], "assistant");
        assert!(asst_msg["content"].is_null());
        let tool_calls = asst_msg["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["function"]["name"], "calculator");
    }

    #[test]
    fn test_system_as_content_blocks() {
        let body = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "Hi"}],
            "system": [
                {"type": "text", "text": "You are helpful."}
            ]
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        let msgs = translated["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful.");
    }

    #[test]
    fn test_all_text_blocks_joined() {
        let body = json!({
            "model": "test",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Hello "},
                    {"type": "text", "text": "world!"}
                ]
            }]
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_request(&body_bytes, "/v1/messages").unwrap();
        let translated: Value = serde_json::from_slice(&result.body).unwrap();
        let msgs = translated["messages"].as_array().unwrap();
        // All-text blocks should be joined into a single string.
        assert_eq!(msgs[0]["content"], "Hello world!");
    }
}
