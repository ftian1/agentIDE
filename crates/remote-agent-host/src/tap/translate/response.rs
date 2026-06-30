//! Translate OpenAI Chat Completions API responses → Anthropic Messages API.
//!
//! Handles both success responses and error responses.  For streaming SSE
//! responses, see [`super::stream`].

use serde_json::{json, Value};

use super::TranslateError;

/// Translate a non-streaming OpenAI chat completion response body to
/// Anthropic Messages format.  Also handles OpenAI error responses.
///
/// Returns the translated body bytes.  On parse failure the original body
/// is passed through unchanged (logged elsewhere).
pub fn translate_nonstreaming_response(body: &[u8]) -> Result<Vec<u8>, TranslateError> {
    let val: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            return Err(TranslateError::new(format!(
                "failed to parse response JSON: {e}"
            )));
        }
    };

    // ── OpenAI error response ────────────────────────────────────────
    if val.get("error").is_some() {
        return translate_openai_error(&val);
    }

    // ── OpenAI success response ──────────────────────────────────────
    if val.get("choices").is_some() {
        return translate_openai_success(&val);
    }

    // Unknown format — pass through.
    Ok(body.to_vec())
}

/// Translate an OpenAI error response to Anthropic error format.
fn translate_openai_error(val: &Value) -> Result<Vec<u8>, TranslateError> {
    let oai_err = &val["error"];
    let message = oai_err
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Unknown error");
    let oai_type = oai_err
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("api_error");

    let anthropic_type = match oai_type {
        "invalid_request_error" => "invalid_request_error",
        "authentication_error" => "authentication_error",
        "rate_limit_error" => "rate_limit_error",
        "server_error" => "api_error",
        _ => "api_error",
    };

    let result = json!({
        "type": "error",
        "error": {
            "type": anthropic_type,
            "message": message
        }
    });

    serde_json::to_vec(&result)
        .map_err(|e| TranslateError::new(format!("failed to serialize error response: {e}")))
}

/// Translate an OpenAI success response to Anthropic Messages format.
fn translate_openai_success(val: &Value) -> Result<Vec<u8>, TranslateError> {
    let id = val
        .get("id")
        .and_then(|i| i.as_str())
        .unwrap_or("msg_unknown");
    let model = val
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown");

    let choice = val
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|choices| choices.first());

    // ── Build content array ──────────────────────────────────────────
    let mut content: Vec<Value> = Vec::new();

    if let Some(choice) = choice {
        let msg = choice.get("message");

        // Text content.
        if let Some(text) = msg.and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
            if !text.is_empty() {
                content.push(json!({"type": "text", "text": text}));
            }
        }

        // Tool calls → tool_use content blocks.
        if let Some(tool_calls) = msg.and_then(|m| m.get("tool_calls")).and_then(|tc| tc.as_array())
        {
            for tc in tool_calls {
                let tc_id = tc
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("");
                let func = tc.get("function");
                let name = func
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let arguments_str = func
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}");
                let input: Value =
                    serde_json::from_str(arguments_str).unwrap_or_else(|_| json!({}));

                content.push(json!({
                    "type": "tool_use",
                    "id": tc_id,
                    "name": name,
                    "input": input
                }));
            }
        }
    }

    // ── Stop reason ──────────────────────────────────────────────────
    let finish_reason = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str());
    let stop_reason = map_finish_reason(finish_reason);

    // ── Usage ────────────────────────────────────────────────────────
    let usage = val.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);

    let result = json!({
        "id": format!("msg_{id}"),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": model,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens
        }
    });

    serde_json::to_vec(&result)
        .map_err(|e| TranslateError::new(format!("failed to serialize response: {e}")))
}

/// Map OpenAI finish_reason to Anthropic stop_reason.
fn map_finish_reason(reason: Option<&str>) -> &'static str {
    match reason {
        Some("stop") => "end_turn",
        Some("length") => "max_tokens",
        Some("content_filter") => "content_filtered",
        Some("tool_calls") => "tool_use",
        Some("function_call") => "tool_use",
        _ => "end_turn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_success_response() {
        let body = json!({
            "id": "chatcmpl-abc123",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_nonstreaming_response(&body_bytes).unwrap();
        let translated: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(translated["id"], "msg_chatcmpl-abc123");
        assert_eq!(translated["type"], "message");
        assert_eq!(translated["role"], "assistant");
        assert_eq!(translated["model"], "gpt-4o");
        assert_eq!(translated["stop_reason"], "end_turn");
        assert_eq!(translated["stop_sequence"], Value::Null);

        let content = translated["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Hello! How can I help?");

        assert_eq!(translated["usage"]["input_tokens"], 10);
        assert_eq!(translated["usage"]["output_tokens"], 5);
    }

    #[test]
    fn test_finish_reason_mapping() {
        let cases = [
            ("stop", "end_turn"),
            ("length", "max_tokens"),
            ("content_filter", "content_filtered"),
            ("tool_calls", "tool_use"),
            ("function_call", "tool_use"),
        ];
        for (openai, anthropic) in &cases {
            let body = json!({
                "id": "test",
                "model": "test",
                "choices": [{"message": {"role": "assistant", "content": ""}, "finish_reason": openai}]
            });
            let body_bytes = serde_json::to_vec(&body).unwrap();
            let result = translate_nonstreaming_response(&body_bytes).unwrap();
            let translated: Value = serde_json::from_slice(&result).unwrap();
            assert_eq!(
                translated["stop_reason"], *anthropic,
                "finish_reason {openai} → {anthropic}"
            );
        }
    }

    #[test]
    fn test_tool_calls_response() {
        let body = json!({
            "id": "chatcmpl-xyz",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"Beijing\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_nonstreaming_response(&body_bytes).unwrap();
        let translated: Value = serde_json::from_slice(&result).unwrap();

        let content = translated["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["id"], "call_123");
        assert_eq!(content[0]["name"], "get_weather");
        assert_eq!(content[0]["input"]["location"], "Beijing");
        assert_eq!(translated["stop_reason"], "tool_use");
    }

    #[test]
    fn test_error_response() {
        let body = json!({
            "error": {
                "message": "Incorrect API key provided: sk-...",
                "type": "authentication_error",
                "param": null,
                "code": "invalid_api_key"
            }
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_nonstreaming_response(&body_bytes).unwrap();
        let translated: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(translated["type"], "error");
        assert_eq!(translated["error"]["type"], "authentication_error");
        assert!(translated["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Incorrect API key"));
    }

    #[test]
    fn test_empty_content() {
        let body = json!({
            "id": "test",
            "model": "test",
            "choices": [{
                "message": {"role": "assistant", "content": ""},
                "finish_reason": "stop"
            }]
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let result = translate_nonstreaming_response(&body_bytes).unwrap();
        let translated: Value = serde_json::from_slice(&result).unwrap();
        // Empty text content should result in empty content array.
        assert_eq!(translated["content"].as_array().unwrap().len(), 0);
    }
}
