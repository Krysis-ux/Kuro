
use serde_json::{json, Map, Value};

pub mod stream;

pub use stream::{AnthropicEvent, StreamTranslator};

pub fn to_openai_request(anthropic: &Value, model: &str) -> Value {
    let mut messages: Vec<Value> = Vec::new();

    if let Some(system) = anthropic.get("system") {
        let text = flatten_text(system);
        if !text.is_empty() {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }

    for message in anthropic
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        translate_message(message, &mut messages);
    }

    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": anthropic.get("stream").and_then(Value::as_bool).unwrap_or(false),
    });

    for key in ["max_tokens", "temperature", "top_p", "stop_sequences"] {
        if let Some(value) = anthropic.get(key) {
            let name = if key == "stop_sequences" { "stop" } else { key };
            body[name] = value.clone();
        }
    }

    if let Some(tools) = anthropic.get("tools").and_then(Value::as_array) {
        let translated: Vec<Value> = tools.iter().map(to_openai_tool).collect();
        if !translated.is_empty() {
            body["tools"] = json!(translated);
            body["tool_choice"] = tool_choice(anthropic);
        }
    }

    body
}

fn to_openai_tool(tool: &Value) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.get("name").cloned().unwrap_or(Value::Null),
            "description": tool.get("description").cloned().unwrap_or_else(|| json!("")),
            "parameters": tool
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
        },
    })
}

fn tool_choice(anthropic: &Value) -> Value {
    match anthropic.pointer("/tool_choice/type").and_then(Value::as_str) {
        Some("any") => json!("required"),
        Some("none") => json!("none"),
        Some("tool") => json!({
            "type": "function",
            "function": {
                "name": anthropic
                    .pointer("/tool_choice/name")
                    .cloned()
                    .unwrap_or(Value::Null),
            },
        }),
        _ => json!("auto"),
    }
}

fn translate_message(message: &Value, out: &mut Vec<Value>) {
    let role = message.get("role").and_then(Value::as_str).unwrap_or("user");

    let Some(blocks) = message.get("content").and_then(Value::as_array) else {
        out.push(json!({
            "role": role,
            "content": message.get("content").cloned().unwrap_or_else(|| json!("")),
        }));
        return;
    };

    let mut text = String::new();
    let mut tool_calls: Vec<Value> = Vec::new();

    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(part) = block.get("text").and_then(Value::as_str) {
                    text.push_str(part);
                }
            }
            Some("tool_use") => tool_calls.push(json!({
                "id": block.get("id").cloned().unwrap_or(Value::Null),
                "type": "function",
                "function": {
                    "name": block.get("name").cloned().unwrap_or(Value::Null),
                    "arguments": block
                        .get("input")
                        .map(|input| input.to_string())
                        .unwrap_or_else(|| "{}".to_string()),
                },
            })),
            Some("tool_result") => {
                out.push(json!({
                    "role": "tool",
                    "tool_call_id": block.get("tool_use_id").cloned().unwrap_or(Value::Null),
                    "content": flatten_text(
                        block.get("content").unwrap_or(&Value::Null)
                    ),
                }));
            }
            _ => {}
        }
    }

    if !tool_calls.is_empty() {
        let mut assistant = json!({ "role": "assistant", "tool_calls": tool_calls });
        assistant["content"] = if text.is_empty() { Value::Null } else { json!(text) };
        out.push(assistant);
    } else if !text.is_empty() {
        out.push(json!({ "role": role, "content": text }));
    }
}

pub fn flatten_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                Value::String(text) => Some(text.clone()),
                _ => block.get("text").and_then(Value::as_str).map(str::to_string),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn to_anthropic_response(openai: &Value, model: &str) -> Value {
    let message = openai.pointer("/choices/0/message").cloned().unwrap_or_else(|| json!({}));
    let mut content: Vec<Value> = Vec::new();

    if let Some(text) = message.get("content").and_then(Value::as_str) {
        if !text.is_empty() {
            content.push(json!({ "type": "text", "text": text }));
        }
    }

    for call in message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        content.push(tool_use_block(call));
    }

    let finish = openai
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("stop");

    json!({
        "id": openai.get("id").cloned().unwrap_or_else(|| json!("msg_kuro")),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason(finish),
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": usage(openai, "prompt_tokens"),
            "output_tokens": usage(openai, "completion_tokens"),
        },
    })
}

pub fn tool_use_block(call: &Value) -> Value {
    let arguments = call
        .pointer("/function/arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}");

    json!({
        "type": "tool_use",
        "id": call.get("id").cloned().unwrap_or_else(|| json!("toolu_kuro")),
        "name": call.pointer("/function/name").cloned().unwrap_or(Value::Null),
        "input": serde_json::from_str::<Value>(arguments)
            .unwrap_or_else(|_| json!({})),
    })
}

pub fn stop_reason(finish: &str) -> &'static str {
    match finish {
        "tool_calls" | "function_call" => "tool_use",
        "length" => "max_tokens",
        "content_filter" => "stop_sequence",
        _ => "end_turn",
    }
}

fn usage(openai: &Value, key: &str) -> u64 {
    openai
        .pointer(&format!("/usage/{key}"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

pub fn error_body(kind: &str, message: &str) -> Value {
    json!({
        "type": "error",
        "error": { "type": kind, "message": message },
    })
}

pub fn wants_stream(anthropic: &Value) -> bool {
    anthropic.get("stream").and_then(Value::as_bool).unwrap_or(false)
}

pub fn requested_model(anthropic: &Value) -> Option<&str> {
    anthropic.get("model").and_then(Value::as_str)
}

pub fn is_object(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_prompt_moves_into_the_message_list() {
        let request = json!({
            "model": "test-model",
            "system": "You are terse.",
            "messages": [{ "role": "user", "content": "hi" }],
        });

        let openai = to_openai_request(&request, "local-model");

        assert_eq!(openai["messages"][0]["role"], "system");
        assert_eq!(openai["messages"][0]["content"], "You are terse.");
        assert_eq!(openai["messages"][1]["role"], "user");
        assert_eq!(openai["model"], "local-model");
    }

    #[test]
    fn a_system_prompt_split_into_blocks_is_joined() {
        let request = json!({
            "system": [
                { "type": "text", "text": "First part." },
                { "type": "text", "text": "Second part." },
            ],
            "messages": [],
        });

        let openai = to_openai_request(&request, "m");

        assert_eq!(openai["messages"][0]["content"], "First part.\nSecond part.");
    }

    #[test]
    fn a_tool_definition_is_rewritten_into_a_function() {
        let request = json!({
            "messages": [],
            "tools": [{
                "name": "read_file",
                "description": "Read a file.",
                "input_schema": { "type": "object", "properties": { "path": { "type": "string" } } },
            }],
        });

        let openai = to_openai_request(&request, "m");
        let tool = &openai["tools"][0];

        assert_eq!(tool["type"], "function");
        assert_eq!(tool["function"]["name"], "read_file");
        assert_eq!(tool["function"]["parameters"]["properties"]["path"]["type"], "string");
    }

    #[test]
    fn an_assistant_tool_call_becomes_tool_calls_with_string_arguments() {
        let request = json!({
            "messages": [{
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "read_file",
                    "input": { "path": "src/main.rs" },
                }],
            }],
        });

        let openai = to_openai_request(&request, "m");
        let call = &openai["messages"][0]["tool_calls"][0];

        assert_eq!(call["id"], "toolu_1");
        assert_eq!(call["function"]["name"], "read_file");
        let arguments = call["function"]["arguments"].as_str().expect("a string");
        assert_eq!(
            serde_json::from_str::<Value>(arguments).expect("valid json")["path"],
            "src/main.rs"
        );
    }

    #[test]
    fn tool_results_become_their_own_messages_in_order() {
        let request = json!({
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "tool_result", "tool_use_id": "toolu_1", "content": "first" },
                    { "type": "tool_result", "tool_use_id": "toolu_2", "content": "second" },
                    { "type": "text", "text": "now what?" },
                ],
            }],
        });

        let openai = to_openai_request(&request, "m");
        let messages = openai["messages"].as_array().expect("messages");

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "tool");
        assert_eq!(messages[0]["tool_call_id"], "toolu_1");
        assert_eq!(messages[0]["content"], "first");
        assert_eq!(messages[1]["tool_call_id"], "toolu_2");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"], "now what?");
    }

    #[test]
    fn a_tool_result_carrying_blocks_is_flattened_to_text() {
        let request = json!({
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "t1",
                    "content": [{ "type": "text", "text": "the output" }],
                }],
            }],
        });

        let openai = to_openai_request(&request, "m");
        assert_eq!(openai["messages"][0]["content"], "the output");
    }

    #[test]
    fn a_plain_string_message_survives_untouched() {
        let request = json!({ "messages": [{ "role": "user", "content": "hello" }] });
        let openai = to_openai_request(&request, "m");

        assert_eq!(openai["messages"][0]["content"], "hello");
    }

    #[test]
    fn a_response_with_text_becomes_a_text_block() {
        let openai = json!({
            "id": "chatcmpl-1",
            "choices": [{ "message": { "content": "Hello." }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 3 },
        });

        let anthropic = to_anthropic_response(&openai, "kuro-model");

        assert_eq!(anthropic["type"], "message");
        assert_eq!(anthropic["content"][0]["type"], "text");
        assert_eq!(anthropic["content"][0]["text"], "Hello.");
        assert_eq!(anthropic["stop_reason"], "end_turn");
        assert_eq!(anthropic["usage"]["input_tokens"], 10);
        assert_eq!(anthropic["usage"]["output_tokens"], 3);
    }

    #[test]
    fn a_response_with_a_tool_call_becomes_a_tool_use_block_with_parsed_input() {
        let openai = json!({
            "choices": [{
                "message": {
                    "content": Value::Null,
                    "tool_calls": [{
                        "id": "call_1",
                        "function": { "name": "read_file", "arguments": "{\"path\":\"a.rs\"}" },
                    }],
                },
                "finish_reason": "tool_calls",
            }],
        });

        let anthropic = to_anthropic_response(&openai, "m");

        assert_eq!(anthropic["content"][0]["type"], "tool_use");
        assert_eq!(anthropic["content"][0]["name"], "read_file");
        assert_eq!(anthropic["content"][0]["input"]["path"], "a.rs");
        assert_eq!(anthropic["stop_reason"], "tool_use");
    }

    #[test]
    fn malformed_tool_arguments_become_an_empty_object_rather_than_a_string() {
        let call = json!({
            "id": "c1",
            "function": { "name": "x", "arguments": "{not json" },
        });

        assert_eq!(tool_use_block(&call)["input"], json!({}));
    }

    #[test]
    fn finish_reasons_are_translated_rather_than_passed_through() {
        assert_eq!(stop_reason("tool_calls"), "tool_use");
        assert_eq!(stop_reason("length"), "max_tokens");
        assert_eq!(stop_reason("stop"), "end_turn");
        assert_eq!(stop_reason("something_new"), "end_turn");
    }

    #[test]
    fn tool_choice_is_translated_in_both_of_its_shapes() {
        let any = json!({ "messages": [], "tools": [{ "name": "a" }], "tool_choice": { "type": "any" } });
        assert_eq!(to_openai_request(&any, "m")["tool_choice"], "required");

        let named = json!({
            "messages": [],
            "tools": [{ "name": "a" }],
            "tool_choice": { "type": "tool", "name": "read_file" },
        });
        let choice = &to_openai_request(&named, "m")["tool_choice"];
        assert_eq!(choice["function"]["name"], "read_file");
    }

    #[test]
    fn sampling_options_are_copied_only_when_they_were_given() {
        let bare = json!({ "messages": [], "max_tokens": 100 });
        let openai = to_openai_request(&bare, "m");

        assert_eq!(openai["max_tokens"], 100);
        assert!(openai.get("temperature").is_none());
    }
}
