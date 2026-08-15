
use serde_json::{json, Value};

use super::{stop_reason, tool_use_block};

#[derive(Debug, Clone, PartialEq)]
pub struct AnthropicEvent {
    pub name: &'static str,
    pub data: Value,
}

impl AnthropicEvent {
    fn new(name: &'static str, data: Value) -> Self {
        Self { name, data }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Open {
    Nothing,
    Text,
    Tool(usize),
}

#[derive(Debug)]
pub struct StreamTranslator {
    model: String,
    started: bool,
    open: Open,
    next_index: usize,
    current_tool: Option<usize>,
    finish: Option<String>,
    output_tokens: u64,
}

impl StreamTranslator {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            started: false,
            open: Open::Nothing,
            next_index: 0,
            current_tool: None,
            finish: None,
            output_tokens: 0,
        }
    }

    pub fn chunk(&mut self, chunk: &Value) -> Vec<AnthropicEvent> {
        let mut events = Vec::new();

        if !self.started {
            self.started = true;
            events.push(AnthropicEvent::new(
                "message_start",
                json!({
                    "type": "message_start",
                    "message": {
                        "id": chunk.get("id").cloned().unwrap_or_else(|| json!("msg_kuro")),
                        "type": "message",
                        "role": "assistant",
                        "model": self.model,
                        "content": [],
                        "stop_reason": Value::Null,
                        "stop_sequence": Value::Null,
                        "usage": { "input_tokens": prompt_tokens(chunk), "output_tokens": 0 },
                    },
                }),
            ));
        }

        if let Some(tokens) = chunk.pointer("/usage/completion_tokens").and_then(Value::as_u64) {
            self.output_tokens = tokens;
        }
        if let Some(reason) = chunk
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
        {
            self.finish = Some(reason.to_string());
        }

        let delta = chunk.pointer("/choices/0/delta").cloned().unwrap_or(Value::Null);

        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            if !text.is_empty() {
                if self.open != Open::Text {
                    events.extend(self.close_open());
                    let index = self.take_index();
                    self.open = Open::Text;
                    events.push(AnthropicEvent::new(
                        "content_block_start",
                        json!({
                            "type": "content_block_start",
                            "index": index,
                            "content_block": { "type": "text", "text": "" },
                        }),
                    ));
                }
                events.push(AnthropicEvent::new(
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": self.current_index(),
                        "delta": { "type": "text_delta", "text": text },
                    }),
                ));
            }
        }

        for call in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            events.extend(self.tool_delta(call));
        }

        events
    }

    fn tool_delta(&mut self, call: &Value) -> Vec<AnthropicEvent> {
        let mut events = Vec::new();

        let position = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let starting = self.current_tool != Some(position);

        if starting {
            events.extend(self.close_open());
            let index = self.take_index();
            self.current_tool = Some(position);
            self.open = Open::Tool(index);

            let mut block = tool_use_block(call);
            block["input"] = json!({});
            events.push(AnthropicEvent::new(
                "content_block_start",
                json!({ "type": "content_block_start", "index": index, "content_block": block }),
            ));
        }

        if let Some(fragment) = call.pointer("/function/arguments").and_then(Value::as_str) {
            if !fragment.is_empty() {
                events.push(AnthropicEvent::new(
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": self.current_index(),
                        "delta": { "type": "input_json_delta", "partial_json": fragment },
                    }),
                ));
            }
        }

        events
    }

    pub fn finish(&mut self) -> Vec<AnthropicEvent> {
        let mut events = Vec::new();

        if !self.started {
            self.started = true;
            events.push(AnthropicEvent::new(
                "message_start",
                json!({
                    "type": "message_start",
                    "message": {
                        "id": "msg_kuro",
                        "type": "message",
                        "role": "assistant",
                        "model": self.model,
                        "content": [],
                        "stop_reason": Value::Null,
                        "stop_sequence": Value::Null,
                        "usage": { "input_tokens": 0, "output_tokens": 0 },
                    },
                }),
            ));
        }

        events.extend(self.close_open());

        let reason = stop_reason(self.finish.as_deref().unwrap_or("stop"));
        events.push(AnthropicEvent::new(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": { "stop_reason": reason, "stop_sequence": Value::Null },
                "usage": { "output_tokens": self.output_tokens },
            }),
        ));
        events.push(AnthropicEvent::new(
            "message_stop",
            json!({ "type": "message_stop" }),
        ));

        events
    }

    pub fn error(&self, message: &str) -> AnthropicEvent {
        AnthropicEvent::new(
            "error",
            json!({
                "type": "error",
                "error": { "type": "api_error", "message": message },
            }),
        )
    }

    fn close_open(&mut self) -> Vec<AnthropicEvent> {
        if self.open == Open::Nothing {
            return Vec::new();
        }
        let index = self.current_index();
        self.open = Open::Nothing;
        vec![AnthropicEvent::new(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": index }),
        )]
    }

    fn take_index(&mut self) -> usize {
        let index = self.next_index;
        self.next_index += 1;
        index
    }

    fn current_index(&self) -> usize {
        match self.open {
            Open::Tool(index) => index,
            _ => self.next_index.saturating_sub(1),
        }
    }
}

fn prompt_tokens(chunk: &Value) -> u64 {
    chunk
        .pointer("/usage/prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(events: &[AnthropicEvent]) -> Vec<&'static str> {
        events.iter().map(|event| event.name).collect()
    }

    fn run(chunks: &[Value]) -> Vec<AnthropicEvent> {
        let mut translator = StreamTranslator::new("kuro-model");
        let mut events: Vec<AnthropicEvent> = Vec::new();
        for chunk in chunks {
            events.extend(translator.chunk(chunk));
        }
        events.extend(translator.finish());
        events
    }

    #[test]
    fn a_text_stream_is_bracketed_the_way_the_client_expects() {
        let events = run(&[
            json!({ "id": "c1", "choices": [{ "delta": { "content": "Hel" } }] }),
            json!({ "choices": [{ "delta": { "content": "lo" } }] }),
            json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] }),
        ]);

        assert_eq!(
            names(&events),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );
        assert_eq!(events[2].data["delta"]["text"], "Hel");
        assert_eq!(events[5].data["delta"]["stop_reason"], "end_turn");
    }

    #[test]
    fn a_tool_call_opens_its_own_block_and_streams_partial_json() {
        let events = run(&[
            json!({ "id": "c1", "choices": [{ "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "function": { "name": "read_file", "arguments": "" },
                }],
            } }] }),
            json!({ "choices": [{ "delta": {
                "tool_calls": [{ "index": 0, "function": { "arguments": "{\"path\":" } }],
            } }] }),
            json!({ "choices": [{ "delta": {
                "tool_calls": [{ "index": 0, "function": { "arguments": "\"a.rs\"}" } }],
            } }] }),
            json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] }),
        ]);

        assert_eq!(
            names(&events),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );

        let start = &events[1].data;
        assert_eq!(start["content_block"]["type"], "tool_use");
        assert_eq!(start["content_block"]["name"], "read_file");
        assert_eq!(start["content_block"]["input"], json!({}));

        assert_eq!(events[2].data["delta"]["type"], "input_json_delta");
        assert_eq!(events[2].data["delta"]["partial_json"], "{\"path\":");
        assert_eq!(events[6].data["type"], "message_stop");
    }

    #[test]
    fn text_followed_by_a_tool_call_closes_the_text_block_first() {
        let events = run(&[
            json!({ "id": "c1", "choices": [{ "delta": { "content": "Let me look." } }] }),
            json!({ "choices": [{ "delta": {
                "tool_calls": [{ "index": 0, "id": "c", "function": { "name": "read_file" } }],
            } }] }),
            json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] }),
        ]);

        assert_eq!(
            names(&events),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "content_block_start",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );
        assert_eq!(events[1].data["index"], 0);
        assert_eq!(events[4].data["index"], 1, "the tool block gets its own index");
    }

    #[test]
    fn two_tool_calls_get_two_blocks() {
        let events = run(&[
            json!({ "id": "c1", "choices": [{ "delta": {
                "tool_calls": [{ "index": 0, "id": "a", "function": { "name": "one" } }],
            } }] }),
            json!({ "choices": [{ "delta": {
                "tool_calls": [{ "index": 1, "id": "b", "function": { "name": "two" } }],
            } }] }),
            json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] }),
        ]);

        let starts: Vec<&Value> = events
            .iter()
            .filter(|event| event.name == "content_block_start")
            .map(|event| &event.data)
            .collect();

        assert_eq!(starts.len(), 2);
        assert_eq!(starts[0]["content_block"]["name"], "one");
        assert_eq!(starts[1]["content_block"]["name"], "two");
        assert_eq!(starts[1]["index"], 1);
    }

    #[test]
    fn a_stream_that_produced_nothing_is_still_a_well_formed_message() {
        let mut translator = StreamTranslator::new("m");
        let events = translator.finish();

        assert_eq!(names(&events), vec!["message_start", "message_delta", "message_stop"]);
    }

    #[test]
    fn usage_from_the_final_chunk_reaches_the_closing_event() {
        let events = run(&[
            json!({ "id": "c1", "choices": [{ "delta": { "content": "hi" } }] }),
            json!({
                "choices": [{ "delta": {}, "finish_reason": "stop" }],
                "usage": { "prompt_tokens": 12, "completion_tokens": 4 },
            }),
        ]);

        let start = events.iter().find(|event| event.name == "message_start").expect("start");
        assert_eq!(start.data["message"]["usage"]["input_tokens"], 0);

        let delta = events.iter().find(|event| event.name == "message_delta").expect("delta");
        assert_eq!(delta.data["usage"]["output_tokens"], 4);
    }

    #[test]
    fn an_error_is_an_event_rather_than_a_dropped_connection() {
        let translator = StreamTranslator::new("m");
        let event = translator.error("the provider refused");

        assert_eq!(event.name, "error");
        assert_eq!(event.data["error"]["message"], "the provider refused");
    }
}
