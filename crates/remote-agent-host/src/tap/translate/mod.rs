//! Anthropic ↔ OpenAI API format translation.
//!
//! When the proxy routes a request to a provider that does not support the
//! Anthropic Messages API natively (e.g. OpenAI, Gemini, Groq, Ollama), this
//! module translates:
//!
//! - **Request**:  Anthropic `/v1/messages`  →  OpenAI `/v1/chat/completions`
//! - **Response**: OpenAI chat completion     →  Anthropic message response
//! - **Stream**:   OpenAI SSE chunks           →  Anthropic SSE event stream
//!
//! Providers that *do* support Anthropic natively (DeepSeek, OpenRouter) are
//! forwarded as-is (with path prefix where needed).

pub mod request;
pub mod response;
pub mod stream;

/// Error returned when translation fails.
#[derive(Debug)]
pub struct TranslateError {
    pub reason: String,
}

impl TranslateError {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for TranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "translate: {}", self.reason)
    }
}

impl std::error::Error for TranslateError {}

/// Returns `true` if the provider kind supports the Anthropic Messages API
/// format natively. The proxy forwards requests to these providers as-is
/// (applying only path prefixing via [`crate::tap::provider_path_prefix`]).
///
/// Providers that return `false` go through the translation layer.
pub fn provider_supports_anthropic_native(provider_kind: &str) -> bool {
    matches!(provider_kind, "deepseek" | "openrouter")
}
