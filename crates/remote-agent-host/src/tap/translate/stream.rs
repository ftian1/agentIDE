//! Streaming SSE translation: OpenAI chat completion chunks → Anthropic
//! message events.
//!
//! ## Architecture
//!
//! ```text
//! Upstream body (OpenAI SSE)
//!   → SseLineParser (extract "data: ..." lines)
//!     → SseTranslator (state machine, emit Anthropic SSE events)
//!       → output buffer → downstream frames
//! ```
//!
//! [`SseLineParser`] handles partial line boundaries (TCP frame splits).
//! [`SseTranslator`] is a pure state machine; it produces `Vec<String>` of
//! Anthropic SSE event strings per input chunk.
//! [`StreamingTranslateBody`] wraps a hyper `Incoming` body and implements
//! `http_body::Body`, yielding translated frames.

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use hyper::body::{Body, Frame, Incoming};
use tokio::sync::mpsc::UnboundedSender;
use shared_protocol::messages::ProtocolMessage;

use super::super::record::{cap_body, ExchangeBuilder, BODY_CAP};

// ── SseLineParser ────────────────────────────────────────────────────

/// Accumulates raw bytes from an SSE stream and yields complete `data:`
/// line payloads (stripped of the `"data: "` prefix).
///
/// Handles partial lines that span TCP frame boundaries.
#[derive(Debug)]
struct SseLineParser {
    buf: Vec<u8>,
}

impl SseLineParser {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(4096),
        }
    }

    /// Push raw bytes from the upstream stream. Returns zero or more
    /// complete SSE data payloads (the content after `"data: "`, without
    /// the prefix or trailing newline).
    fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(data);
        let mut results: Vec<Vec<u8>> = Vec::new();

        loop {
            // Find next newline.
            let nl = self.buf.iter().position(|&b| b == b'\n');
            let Some(nl_pos) = nl else {
                // No complete line yet; wait for more data.
                break;
            };

            // Extract line up to (but not including) the newline.
            let line = self.buf[..nl_pos].to_vec();
            // Remove the line (including newline) from buffer.
            let consume = nl_pos + 1;
            let remaining = self.buf[consume..].to_vec();
            self.buf = remaining;

            // Trim trailing \r (CRLF).
            let line = if line.ends_with(b"\r") {
                &line[..line.len() - 1]
            } else {
                line.as_slice()
            };

            if line.is_empty() {
                // Empty line = SSE message boundary. Skip.
                continue;
            }

            // Parse "data: <payload>" line.
            if let Some(payload) = line.strip_prefix(b"data: ") {
                results.push(payload.to_vec());
            } else if let Some(payload) = line.strip_prefix(b"data:") {
                // Bare "data:" without space.
                results.push(payload.to_vec());
            }
            // Ignore other SSE fields (event:, id:, retry:).
        }

        results
    }

    /// Drain any remaining partial data (no complete line). Returns it as
    /// raw bytes if non-empty, for use as a final (possibly incomplete) chunk.
    fn drain(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }
}

// ── SseTranslator ────────────────────────────────────────────────────

/// State of the streaming translation state machine.
#[derive(Debug, Clone, PartialEq)]
enum StreamState {
    /// Awaiting first content chunk.
    Initial,
    /// Receiving text delta chunks.
    TextContent {
        has_block: bool,
        content_block_index: usize,
    },
    /// Receiving tool call delta chunks.
    ToolCalls {
        tool_indices: Vec<usize>, // which tool blocks are active
    },
    /// Stream finished.
    Finished,
}

/// Translates OpenAI SSE streaming chunks → Anthropic SSE event strings.
///
/// Call [`push_chunk`] for each OpenAI SSE `data:` payload. It returns
/// Anthropic SSE event strings to write downstream.
///
/// Call [`finish`] when the stream ends (`data: [DONE]`) to get any
/// remaining events.
#[derive(Debug)]
pub struct SseTranslator {
    state: StreamState,
    message_id: String,
    model: String,
    output_text_len: usize,
}

impl SseTranslator {
    pub fn new() -> Self {
        Self {
            state: StreamState::Initial,
            message_id: String::new(),
            model: String::from("unknown"),
            output_text_len: 0,
        }
    }

    /// Process an OpenAI SSE `data:` payload (raw bytes of the JSON after
    /// `"data: "`). Returns Anthropic SSE event strings.
    pub fn push_chunk(&mut self, payload: &[u8]) -> Vec<String> {
        if self.state == StreamState::Finished {
            return vec![];
        }

        // Check for [DONE] sentinel.
        if payload == b"[DONE]" {
            return self.finish_inner();
        }

        let val: serde_json::Value = match serde_json::from_slice(payload) {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let choice = val
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first());

        // Capture id / model from first chunk.
        if self.message_id.is_empty() {
            if let Some(id) = val.get("id").and_then(|i| i.as_str()) {
                self.message_id = format!("msg_{id}");
            } else {
                self.message_id = "msg_unknown".to_string();
            }
        }
        if self.model == "unknown" {
            if let Some(m) = val.get("model").and_then(|m| m.as_str()) {
                self.model = m.to_string();
            }
        }

        let delta = choice.and_then(|c| c.get("delta"));
        let finish_reason = choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(|f| f.as_str());

        match &self.state {
            StreamState::Initial => {
                self.handle_initial(delta, finish_reason)
            }
            StreamState::TextContent { .. } => self.handle_text(delta, finish_reason),
            StreamState::ToolCalls { .. } => {
                self.handle_tool_calls(delta, finish_reason)
            }
            StreamState::Finished => vec![],
        }
    }

    fn handle_initial(
        &mut self,
        delta: Option<&serde_json::Value>,
        finish_reason: Option<&str>,
    ) -> Vec<String> {
        let mut events: Vec<String> = Vec::new();

        // Emit message_start.
        events.push(sse_event(
            "message_start",
            &serde_json::json!({
                "type": "message_start",
                "message": {
                    "id": self.message_id,
                    "type": "message",
                    "role": "assistant",
                    "model": self.model,
                    "content": [],
                    "usage": {"input_tokens": 0, "output_tokens": 0}
                }
            }),
        ));

        if let Some(delta) = delta {
            // Check for tool calls first.
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                // Emit content_block_start for each tool.
                let mut tool_indices: Vec<usize> = Vec::new();
                for tc in tool_calls {
                    let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let func = tc.get("function");
                    let name = func
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    let tc_id = tc
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("");

                    events.push(sse_event(
                        "content_block_start",
                        &serde_json::json!({
                            "type": "content_block_start",
                            "index": idx,
                            "content_block": {
                                "type": "tool_use",
                                "id": tc_id,
                                "name": name,
                                "input": {}
                            }
                        }),
                    ));

                    // If arguments are already present in this chunk.
                    if let Some(args) = func.and_then(|f| f.get("arguments")) {
                        if let Some(args_str) = args.as_str() {
                            if !args_str.is_empty() {
                                events.push(sse_event(
                                    "content_block_delta",
                                    &serde_json::json!({
                                        "type": "content_block_delta",
                                        "index": idx,
                                        "delta": {
                                            "type": "input_json_delta",
                                            "partial_json": args_str
                                        }
                                    }),
                                ));
                            }
                        }
                    }

                    tool_indices.push(idx);
                }
                self.state = StreamState::ToolCalls { tool_indices };
            } else {
                // Text content — emit content_block_start (index 0).
                let idx = 0usize;
                events.push(sse_event(
                    "content_block_start",
                    &serde_json::json!({
                        "type": "content_block_start",
                        "index": idx,
                        "content_block": {
                            "type": "text",
                            "text": ""
                        }
                    }),
                ));

                // If there's content in the first chunk, emit it.
                if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        self.output_text_len += text.len();
                        events.push(sse_event(
                            "content_block_delta",
                            &serde_json::json!({
                                "type": "content_block_delta",
                                "index": idx,
                                "delta": {
                                    "type": "text_delta",
                                    "text": text
                                }
                            }),
                        ));
                    }
                }

                self.state = StreamState::TextContent {
                    has_block: true,
                    content_block_index: idx,
                };
            }
        }

        // Check finish_reason on first chunk (edge case: empty response).
        if let Some(fr) = finish_reason {
            events.extend(self.emit_finish(fr));
        }

        events
    }

    fn handle_text(
        &mut self,
        delta: Option<&serde_json::Value>,
        finish_reason: Option<&str>,
    ) -> Vec<String> {
        let mut events: Vec<String> = Vec::new();

        if let Some(delta) = delta {
            // Check if transition to tool calls.
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                let idx = if let StreamState::TextContent {
                    content_block_index, ..
                } = &self.state
                {
                    *content_block_index
                } else {
                    0
                };

                // End current text block.
                events.push(sse_event(
                    "content_block_stop",
                    &serde_json::json!({
                        "type": "content_block_stop",
                        "index": idx
                    }),
                ));

                // Start tool call blocks.
                let mut tool_indices: Vec<usize> = Vec::new();
                for tc in tool_calls {
                    let tc_idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let func = tc.get("function");
                    let name = func
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    let tc_id = tc
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("");

                    events.push(sse_event(
                        "content_block_start",
                        &serde_json::json!({
                            "type": "content_block_start",
                            "index": tc_idx,
                            "content_block": {
                                "type": "tool_use",
                                "id": tc_id,
                                "name": name,
                                "input": {}
                            }
                        }),
                    ));

                    if let Some(args) = func.and_then(|f| f.get("arguments")) {
                        if let Some(args_str) = args.as_str() {
                            if !args_str.is_empty() {
                                events.push(sse_event(
                                    "content_block_delta",
                                    &serde_json::json!({
                                        "type": "content_block_delta",
                                        "index": tc_idx,
                                        "delta": {
                                            "type": "input_json_delta",
                                            "partial_json": args_str
                                        }
                                    }),
                                ));
                            }
                        }
                    }
                    tool_indices.push(tc_idx);
                }
                self.state = StreamState::ToolCalls { tool_indices };
            } else if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                if !text.is_empty() {
                    self.output_text_len += text.len();
                    let idx = if let StreamState::TextContent {
                        content_block_index, ..
                    } = &self.state
                    {
                        *content_block_index
                    } else {
                        0
                    };
                    events.push(sse_event(
                        "content_block_delta",
                        &serde_json::json!({
                            "type": "content_block_delta",
                            "index": idx,
                            "delta": {
                                "type": "text_delta",
                                "text": text
                            }
                        }),
                    ));
                }
            }
        }

        if let Some(fr) = finish_reason {
            // End current text block.
            let idx = if let StreamState::TextContent {
                content_block_index, ..
            } = &self.state
            {
                *content_block_index
            } else {
                0
            };
            events.push(sse_event(
                "content_block_stop",
                &serde_json::json!({
                    "type": "content_block_stop",
                    "index": idx
                }),
            ));
            events.extend(self.emit_finish(fr));
        }

        events
    }

    fn handle_tool_calls(
        &mut self,
        delta: Option<&serde_json::Value>,
        finish_reason: Option<&str>,
    ) -> Vec<String> {
        let mut events: Vec<String> = Vec::new();

        if let Some(delta) = delta {
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                for tc in tool_calls {
                    let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let func = tc.get("function");

                    // Check if this is a new tool call (has name/id).
                    let has_name = func
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .is_some();
                    let has_id = tc.get("id").is_some();

                    if let StreamState::ToolCalls { tool_indices } = &self.state {
                        if (has_name || has_id) && !tool_indices.contains(&idx) {
                            // New tool call appeared mid-stream — emit block_start.
                            let name = func
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("");
                            let tc_id = tc
                                .get("id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("");
                            events.push(sse_event(
                                "content_block_start",
                                &serde_json::json!({
                                    "type": "content_block_start",
                                    "index": idx,
                                    "content_block": {
                                        "type": "tool_use",
                                        "id": tc_id,
                                        "name": name,
                                        "input": {}
                                    }
                                }),
                            ));
                        }
                    }

                    // Arguments delta.
                    if let Some(args) = func.and_then(|f| f.get("arguments")) {
                        if let Some(args_str) = args.as_str() {
                            if !args_str.is_empty() {
                                events.push(sse_event(
                                    "content_block_delta",
                                    &serde_json::json!({
                                        "type": "content_block_delta",
                                        "index": idx,
                                        "delta": {
                                            "type": "input_json_delta",
                                            "partial_json": args_str
                                        }
                                    }),
                                ));
                            }
                        }
                    }
                }
            }
        }

        if let Some(fr) = finish_reason {
            // End all active tool call blocks.
            if let StreamState::ToolCalls { tool_indices } = &self.state {
                for idx in tool_indices {
                    events.push(sse_event(
                        "content_block_stop",
                        &serde_json::json!({
                            "type": "content_block_stop",
                            "index": idx
                        }),
                    ));
                }
            }
            events.extend(self.emit_finish(fr));
        }

        events
    }

    fn emit_finish(&mut self, finish_reason: &str) -> Vec<String> {
        let stop_reason = map_finish_reason(Some(finish_reason));
        // Estimate output_tokens: ~4 chars per token.
        let output_tokens = (self.output_text_len as f64 / 4.0).ceil() as u64;

        let events = vec![
            sse_event(
                "message_delta",
                &serde_json::json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": stop_reason,
                        "stop_sequence": null
                    },
                    "usage": {
                        "output_tokens": output_tokens
                    }
                }),
            ),
            sse_event(
                "message_stop",
                &serde_json::json!({
                    "type": "message_stop"
                }),
            ),
        ];

        self.state = StreamState::Finished;
        events
    }

    fn finish_inner(&mut self) -> Vec<String> {
        let mut events: Vec<String> = Vec::new();

        match &self.state {
            StreamState::Initial => {
                // Stream ended without any content — emit minimal events.
                if self.message_id.is_empty() {
                    self.message_id = "msg_unknown".to_string();
                }
                events.push(sse_event(
                    "message_start",
                    &serde_json::json!({
                        "type": "message_start",
                        "message": {
                            "id": self.message_id,
                            "type": "message",
                            "role": "assistant",
                            "model": self.model,
                            "content": [],
                            "usage": {"input_tokens": 0, "output_tokens": 0}
                        }
                    }),
                ));
                let output_tokens = (self.output_text_len as f64 / 4.0).ceil() as u64;
                events.push(sse_event(
                    "message_delta",
                    &serde_json::json!({
                        "type": "message_delta",
                        "delta": {
                            "stop_reason": "end_turn",
                            "stop_sequence": null
                        },
                        "usage": {
                            "output_tokens": output_tokens
                        }
                    }),
                ));
                events.push(sse_event(
                    "message_stop",
                    &serde_json::json!({
                        "type": "message_stop"
                    }),
                ));
            }
            StreamState::TextContent {
                content_block_index, ..
            } => {
                // End current text block.
                events.push(sse_event(
                    "content_block_stop",
                    &serde_json::json!({
                        "type": "content_block_stop",
                        "index": content_block_index
                    }),
                ));
                let output_tokens = (self.output_text_len as f64 / 4.0).ceil() as u64;
                events.push(sse_event(
                    "message_delta",
                    &serde_json::json!({
                        "type": "message_delta",
                        "delta": {
                            "stop_reason": "end_turn",
                            "stop_sequence": null
                        },
                        "usage": {
                            "output_tokens": output_tokens
                        }
                    }),
                ));
                events.push(sse_event(
                    "message_stop",
                    &serde_json::json!({
                        "type": "message_stop"
                    }),
                ));
            }
            StreamState::ToolCalls { tool_indices } => {
                for idx in tool_indices {
                    events.push(sse_event(
                        "content_block_stop",
                        &serde_json::json!({
                            "type": "content_block_stop",
                            "index": idx
                        }),
                    ));
                }
                events.push(sse_event(
                    "message_delta",
                    &serde_json::json!({
                        "type": "message_delta",
                        "delta": {
                            "stop_reason": "tool_use",
                            "stop_sequence": null
                        },
                        "usage": {
                            "output_tokens": 0
                        }
                    }),
                ));
                events.push(sse_event(
                    "message_stop",
                    &serde_json::json!({
                        "type": "message_stop"
                    }),
                ));
            }
            StreamState::Finished => {}
        }

        self.state = StreamState::Finished;
        events
    }

    /// Called when the stream ends (after last chunk, or on `data: [DONE]`).
    pub fn finish(&mut self) -> Vec<String> {
        self.finish_inner()
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Build a complete Anthropic SSE event string.
fn sse_event(event: &str, data: &serde_json::Value) -> String {
    let json = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    format!("event: {event}\ndata: {json}\n\n")
}

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

// ── StreamingTranslateBody ───────────────────────────────────────────

/// Wraps an upstream hyper `Incoming` body, reads OpenAI SSE chunks,
/// translates to Anthropic SSE events, and yields them as hyper frames.
///
/// Also captures the translated output and emits the exchange to the
/// frontend on completion.
pub struct StreamingTranslateBody {
    inner: Incoming,
    parser: SseLineParser,
    translator: SseTranslator,
    /// Buffer of translated output bytes waiting to be sent downstream.
    output: Vec<u8>,
    /// Captured translated bytes (for recording to the frontend).
    captured: Vec<u8>,
    truncated: bool,
    finished: bool,
    /// Recording context — emitted on stream completion.
    emit_builder: Option<ExchangeBuilder>,
    emit_status: u16,
    emit_resp_headers: std::collections::HashMap<String, String>,
    emit_duration_start: std::time::Instant,
    emit_transport_tx: Option<UnboundedSender<ProtocolMessage>>,
    emit_session_id: String,
    emit_seq: Option<Arc<AtomicU64>>,
}

impl StreamingTranslateBody {
    pub fn new(inner: Incoming) -> Self {
        Self {
            inner,
            parser: SseLineParser::new(),
            translator: SseTranslator::new(),
            output: Vec::new(),
            captured: Vec::new(),
            truncated: false,
            finished: false,
            emit_builder: None,
            emit_status: 200,
            emit_resp_headers: std::collections::HashMap::new(),
            emit_duration_start: std::time::Instant::now(),
            emit_transport_tx: None,
            emit_session_id: String::new(),
            emit_seq: None,
        }
    }

    /// Attach recording context so the translated exchange is reported to
    /// the frontend when the stream completes.
    pub fn with_recording(
        mut self,
        builder: ExchangeBuilder,
        status: u16,
        resp_headers: std::collections::HashMap<String, String>,
        duration_start: std::time::Instant,
        transport_tx: UnboundedSender<ProtocolMessage>,
        session_id: String,
        seq: Arc<AtomicU64>,
    ) -> Self {
        self.emit_builder = Some(builder);
        self.emit_status = status;
        self.emit_resp_headers = resp_headers;
        self.emit_duration_start = duration_start;
        self.emit_transport_tx = Some(transport_tx);
        self.emit_session_id = session_id;
        self.emit_seq = Some(seq);
        self
    }

    fn emit(&mut self) {
        let Some(builder) = self.emit_builder.take() else { return };
        let Some(tx) = self.emit_transport_tx.take() else { return };
        let Some(seq) = self.emit_seq.take() else { return };
        let duration_ms = self.emit_duration_start.elapsed().as_millis() as u64;
        let body = std::mem::take(&mut self.captured);
        let (body, truncated) = cap_body(body);
        let exchange = builder.finish(
            self.emit_status,
            std::mem::take(&mut self.emit_resp_headers),
            body,
            duration_ms,
            truncated || self.truncated,
        );
        let n = seq.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(ProtocolMessage::HttpTraffic {
            session_id: std::mem::take(&mut self.emit_session_id),
            exchange,
            seq: n,
        });
    }
}

impl Body for StreamingTranslateBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();

        loop {
            // If we have buffered output, return it as a frame.
            if !this.output.is_empty() {
                let data = Bytes::from(std::mem::take(&mut this.output));
                return Poll::Ready(Some(Ok(Frame::data(data))));
            }

            if this.finished {
                return Poll::Ready(None);
            }

            // Read from upstream.
            match Pin::new(&mut this.inner).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => {
                    if let Some(data) = frame.data_ref() {
                        // Parse SSE data lines from upstream bytes.
                        let payloads = this.parser.push(data);
                        for payload in payloads {
                            // Translate.
                            let events = this.translator.push_chunk(&payload);
                            for event in &events {
                                let event_bytes = event.as_bytes();
                                // Capture with BODY_CAP limit.
                                if this.captured.len() < BODY_CAP {
                                    let room = BODY_CAP - this.captured.len();
                                    if event_bytes.len() > room {
                                        this.captured.extend_from_slice(&event_bytes[..room]);
                                        this.truncated = true;
                                    } else {
                                        this.captured.extend_from_slice(event_bytes);
                                    }
                                } else if !event_bytes.is_empty() {
                                    this.truncated = true;
                                }
                                this.output.extend_from_slice(event_bytes);
                            }
                        }
                    }
                    // Continue loop to check for output or read more.
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) => {
                    // Upstream stream ended. Drain any remaining parser data.
                    let _remaining = this.parser.drain();
                    // _remaining may contain a trailing partial line; ignore it.
                    // The translator.finish() below handles final events.

                    // Call finish on translator.
                    let final_events = this.translator.finish();
                    for event in &final_events {
                        this.captured.extend_from_slice(event.as_bytes());
                        this.output.extend_from_slice(event.as_bytes());
                    }

                    this.finished = true;
                    // Continue loop to emit buffered output.
                }
                Poll::Pending => {
                    if !this.output.is_empty() {
                        let data = Bytes::from(std::mem::take(&mut this.output));
                        return Poll::Ready(Some(Ok(Frame::data(data))));
                    }
                    return Poll::Pending;
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.finished && self.output.is_empty()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for StreamingTranslateBody {
    fn drop(&mut self) {
        self.emit();
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_line_parser_basic() {
        let mut parser = SseLineParser::new();
        let data = b"data: {\"hello\":\"world\"}\n\n";
        let results = parser.push(data);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], b"{\"hello\":\"world\"}");
    }

    #[test]
    fn test_sse_line_parser_partial() {
        let mut parser = SseLineParser::new();
        let results = parser.push(b"data: {\"hel");
        assert!(results.is_empty());
        let results = parser.push(b"lo\":\"world\"}\n\n");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], b"{\"hello\":\"world\"}");
    }

    #[test]
    fn test_sse_line_parser_done() {
        let mut parser = SseLineParser::new();
        let results = parser.push(b"data: [DONE]\n\n");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], b"[DONE]");
    }

    #[test]
    fn test_sse_line_parser_multiple() {
        let mut parser = SseLineParser::new();
        let results = parser.push(
            b"data: {\"a\":1}\n\ndata: {\"b\":2}\n\n",
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], b"{\"a\":1}");
        assert_eq!(results[1], b"{\"b\":2}");
    }

    #[test]
    fn test_sse_line_parser_ignores_non_data() {
        let mut parser = SseLineParser::new();
        let results = parser.push(
            b"event: ping\ndata: {\"x\":1}\n\n",
        );
        // Only the data: line payload is returned; event: is ignored.
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], b"{\"x\":1}");
    }

    #[test]
    fn test_translator_basic_text_stream() {
        let mut t = SseTranslator::new();

        // First chunk: role delta.
        let chunk1 = serde_json::json!({
            "id": "chatcmpl-abc",
            "model": "gpt-4o",
            "choices": [{"index": 0, "delta": {"role": "assistant"}, "finish_reason": null}]
        });
        let events = t.push_chunk(serde_json::to_vec(&chunk1).unwrap().as_slice());
        assert!(!events.is_empty());
        // Should emit message_start + content_block_start.
        let joined = events.join("");
        assert!(joined.contains("message_start"));
        assert!(joined.contains("content_block_start"));

        // Text delta.
        let chunk2 = serde_json::json!({
            "id": "chatcmpl-abc",
            "model": "gpt-4o",
            "choices": [{"index": 0, "delta": {"content": "Hello!"}, "finish_reason": null}]
        });
        let events = t.push_chunk(serde_json::to_vec(&chunk2).unwrap().as_slice());
        let joined = events.join("");
        assert!(joined.contains("content_block_delta"));
        assert!(joined.contains("Hello!"));

        // Finish.
        let chunk3 = serde_json::json!({
            "id": "chatcmpl-abc",
            "model": "gpt-4o",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
        });
        let events = t.push_chunk(serde_json::to_vec(&chunk3).unwrap().as_slice());
        let joined = events.join("");
        assert!(joined.contains("content_block_stop"));
        assert!(joined.contains("message_delta"));
        assert!(joined.contains("message_stop"));
    }

    #[test]
    fn test_translator_done_sentinel() {
        let mut t = SseTranslator::new();

        // Send some content.
        let chunk1 = serde_json::json!({
            "id": "chatcmpl-abc",
            "model": "gpt-4o",
            "choices": [{"index": 0, "delta": {"content": "Hi"}, "finish_reason": null}]
        });
        let _ = t.push_chunk(serde_json::to_vec(&chunk1).unwrap().as_slice());

        // [DONE] sentinel.
        let events = t.push_chunk(b"[DONE]");
        let joined = events.join("");
        assert!(joined.contains("content_block_stop"));
        assert!(joined.contains("message_stop"));
    }

    #[test]
    fn test_translator_empty_stream() {
        let mut t = SseTranslator::new();
        // No chunks, just finish.
        let events = t.finish();
        let joined = events.join("");
        assert!(joined.contains("message_start"));
        assert!(joined.contains("message_stop"));
    }
}
