//! Minimal LLM client supporting two providers (raw HTTP -- there is no
//! official Rust SDK for either): Anthropic's Messages API and Google's
//! Gemini `generateContent` API. Both binaries program against the
//! provider-agnostic `Message`/`ContentBlock`/`ToolDef`/`ApiResponse` types
//! below; provider selection and wire-format translation happen entirely
//! inside `Client`.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

/// Per `claude-api` skill guidance: default to the most capable model
/// unless told otherwise. Override with `ANTHROPIC_MODEL` for cost tuning
/// (the challenge's "Operational Cost" evaluation dimension) without a
/// rebuild.
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-opus-5";
/// A stable, widely-available Gemini model used as the default. Override
/// with `GEMINI_MODEL` -- Google ships new model generations faster than
/// this file gets updated, so treat this default as a safe fallback, not a
/// recommendation to stay on it. `gemini-2.5-flash` was the first choice
/// here but its free tier caps at 20 requests/*day*, not per-minute --
/// `gemini-3.5-flash-lite` has a much friendlier free-tier quota and is
/// what this project was actually evaluated against (see CHANGELOG
/// Iteration 5).
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash-lite";

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
        /// Gemini-only: an opaque signature Gemini 3.x attaches to a
        /// `functionCall` part when thinking produced it. It must be
        /// echoed back unchanged on that same part in the next turn, or
        /// the API rejects the request with a 400 ("missing a
        /// thought_signature"). Always `None` for Anthropic, which has no
        /// such requirement.
        thought_signature: Option<String>,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: &'static str,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Message {
            role: "user",
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

#[derive(Debug)]
pub struct ApiResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl ApiResponse {
    /// Concatenates every text block, ignoring tool_use blocks. Convenience
    /// for the baseline binary, which never expects tool calls back.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn tool_uses(&self) -> Vec<(&str, &str, &Value)> {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse {
                    id, name, input, ..
                } => Some((id.as_str(), name.as_str(), input)),
                _ => None,
            })
            .collect()
    }
}

pub struct Client {
    provider: Provider,
    api_key: String,
    model: String,
    http: reqwest::blocking::Client,
}

/// Reads an env var, treating both "unset" and "set but empty" as absent.
/// `.env.example`'s convention of leaving optional vars as a bare `KEY=`
/// line means `std::env::var` alone would see `Ok("")` for those and never
/// fall back to a default -- this is what that convention actually needs.
fn env_var_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Client {
    /// Picks a provider from `LLM_PROVIDER` (`"anthropic"` (default) or
    /// `"gemini"`), then reads that provider's API key
    /// (`ANTHROPIC_API_KEY` / `GEMINI_API_KEY`, required) and model
    /// (`ANTHROPIC_MODEL` / `GEMINI_MODEL`, optional). Callers should run
    /// `dotenvy::dotenv().ok()` first if they want `.env` support -- see
    /// `.env.example`.
    pub fn from_env() -> Result<Self> {
        let provider = match env_var_nonempty("LLM_PROVIDER")
            .unwrap_or_else(|| "anthropic".to_string())
            .to_lowercase()
            .as_str()
        {
            "gemini" => Provider::Gemini,
            "anthropic" => Provider::Anthropic,
            other => bail!("unknown LLM_PROVIDER '{other}' (expected 'anthropic' or 'gemini')"),
        };

        let (key_var, model_var, default_model) = match provider {
            Provider::Anthropic => (
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_MODEL",
                DEFAULT_ANTHROPIC_MODEL,
            ),
            Provider::Gemini => ("GEMINI_API_KEY", "GEMINI_MODEL", DEFAULT_GEMINI_MODEL),
        };
        let api_key = env_var_nonempty(key_var).with_context(|| {
            format!("{key_var} is not set. Copy .env.example to .env and fill in a real key.")
        })?;
        let model = env_var_nonempty(model_var).unwrap_or_else(|| default_model.to_string());

        Ok(Client {
            provider,
            api_key,
            model,
            http: reqwest::blocking::Client::new(),
        })
    }

    /// Which provider this client is actually configured to call -- surfaced
    /// so callers can record it in a trajectory, since the trajectory itself
    /// otherwise has no way to prove which model produced a verdict.
    pub fn provider_label(&self) -> &'static str {
        match self.provider {
            Provider::Anthropic => "anthropic",
            Provider::Gemini => "gemini",
        }
    }

    /// The exact model string this client sends on every request (from
    /// `ANTHROPIC_MODEL`/`GEMINI_MODEL`, or the built-in default).
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn send(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<ApiResponse> {
        match self.provider {
            Provider::Anthropic => self.send_anthropic(system, messages, tools),
            Provider::Gemini => self.send_gemini(system, messages, tools),
        }
    }

    fn send_anthropic(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<ApiResponse> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "system": system,
            "messages": messages.iter().map(anthropic_message_to_json).collect::<Vec<_>>(),
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools.iter().map(anthropic_tool_to_json).collect());
        }

        let raw = send_with_retries(
            || {
                self.http
                    .post(ANTHROPIC_API_URL)
                    .header("Content-Type", "application/json")
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", ANTHROPIC_VERSION)
                    .json(&body)
            },
            "Anthropic",
        )?;
        parse_anthropic_response(&raw)
    }

    fn send_gemini(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<ApiResponse> {
        let mut body = json!({
            "contents": messages_to_gemini_contents(messages),
            "systemInstruction": {"parts": [{"text": system}]},
        });
        if !tools.is_empty() {
            body["tools"] = json!([{
                "functionDeclarations": tools.iter().map(gemini_function_declaration).collect::<Vec<_>>(),
            }]);
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );
        let raw = send_with_retries(
            || {
                self.http
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .json(&body)
            },
            "Gemini",
        )?;
        parse_gemini_response(&raw)
    }
}

const MAX_ATTEMPTS: u32 = 4;

/// Sends a request built fresh by `build_request` on each attempt (a
/// `reqwest::RequestBuilder` is consumed by `.send()`, so it can't be
/// reused directly), retrying with exponential backoff (1s, 2s, 4s) on
/// transient failures: a network-level send error, HTTP 429, or a 5xx. A
/// non-retryable error (bad auth, malformed request, unexpected 4xx)
/// returns immediately instead of burning through retries pointlessly.
///
/// This exists because rapid back-to-back calls (e.g. the acceptance
/// harness running 5 strategies x 2 binaries) hit real, intermittent
/// failures against a live provider in practice -- see CHANGELOG for the
/// concrete numbers this was built in response to.
fn send_with_retries(
    build_request: impl Fn() -> reqwest::blocking::RequestBuilder,
    provider_label: &str,
) -> Result<Value> {
    let mut last_err: Option<anyhow::Error> = None;
    let mut wait = std::time::Duration::from_secs(1);

    for attempt in 0..MAX_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(wait);
        }

        let resp = match build_request().send() {
            Ok(r) => r,
            Err(e) => {
                last_err = Some(
                    anyhow::Error::new(e)
                        .context(format!("sending request to the {provider_label} API")),
                );
                wait *= 2;
                continue;
            }
        };

        let status = resp.status();
        // A 429's `Retry-After` header (standard) or, for Gemini, a
        // `retryDelay` field in the JSON error body, tells us exactly how
        // long the quota window needs -- a per-minute RPM cap needs tens of
        // seconds, far more than blind exponential backoff would wait
        // before giving up. Prefer that over guessing.
        let server_retry_after = retry_after_from_headers(resp.headers());
        let text = match resp.text() {
            Ok(t) => t,
            Err(e) => {
                last_err = Some(anyhow::Error::new(e).context("reading response body"));
                wait *= 2;
                continue;
            }
        };

        if status.is_success() {
            return serde_json::from_str(&text)
                .with_context(|| format!("parsing {provider_label} API response body: {text}"));
        }

        let body_desc = if text.is_empty() {
            "<empty body>"
        } else {
            &text
        };
        let err = anyhow::anyhow!("{provider_label} API returned {status}: {body_desc}");
        let retryable = status.as_u16() == 429 || status.is_server_error();
        if !retryable {
            return Err(err);
        }
        wait = server_retry_after
            .or_else(|| retry_delay_from_body(&text))
            .map(|secs| std::time::Duration::from_secs(secs + 1)) // +1s buffer
            .unwrap_or(wait * 2);
        last_err = Some(err);
    }
    Err(last_err.unwrap_or_else(|| {
        anyhow::anyhow!("{provider_label} API request failed with no captured error")
    }))
}

fn retry_after_from_headers(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// Best-effort search for Gemini's `"retryDelay": "22s"`-shaped field
/// anywhere in a JSON error body.
fn retry_delay_from_body(text: &str) -> Option<u64> {
    let value: Value = serde_json::from_str(text).ok()?;
    find_retry_delay(&value)
}

fn find_retry_delay(value: &Value) -> Option<u64> {
    match value {
        Value::Object(map) => {
            if let Some(seconds) = map
                .get("retryDelay")
                .and_then(|v| v.as_str())
                .and_then(|s| s.strip_suffix('s'))
                .and_then(|n| n.parse::<u64>().ok())
            {
                return Some(seconds);
            }
            map.values().find_map(find_retry_delay)
        }
        Value::Array(arr) => arr.iter().find_map(find_retry_delay),
        _ => None,
    }
}

// ---- Anthropic wire format ----

fn anthropic_tool_to_json(tool: &ToolDef) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

fn anthropic_message_to_json(message: &Message) -> Value {
    json!({
        "role": message.role,
        "content": message.content.iter().map(anthropic_content_block_to_json).collect::<Vec<_>>(),
    })
}

fn anthropic_content_block_to_json(block: &ContentBlock) -> Value {
    match block {
        ContentBlock::Text { text } => json!({"type": "text", "text": text}),
        ContentBlock::ToolUse {
            id, name, input, ..
        } => {
            json!({"type": "tool_use", "id": id, "name": name, "input": input})
        }
        ContentBlock::ToolResult {
            tool_use_id,
            content,
        } => json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content}),
    }
}

fn parse_anthropic_response(raw: &Value) -> Result<ApiResponse> {
    let content_arr = raw
        .get("content")
        .and_then(|c| c.as_array())
        .context("response missing `content` array")?;

    let mut content = Vec::with_capacity(content_arr.len());
    for block in content_arr {
        let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match block_type {
            "text" => content.push(ContentBlock::Text {
                text: block
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default()
                    .to_string(),
            }),
            "tool_use" => content.push(ContentBlock::ToolUse {
                id: block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                name: block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                input: block.get("input").cloned().unwrap_or(Value::Null),
                thought_signature: None,
            }),
            // "thinking" and other block types are intentionally ignored --
            // the trajectory log captures our own reasoning steps, not the
            // model's internal chain of thought.
            _ => {}
        }
    }

    let stop_reason = raw
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown")
        .to_string();
    let input_tokens = raw
        .get("usage")
        .and_then(|u| u.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let output_tokens = raw
        .get("usage")
        .and_then(|u| u.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    Ok(ApiResponse {
        content,
        stop_reason,
        input_tokens,
        output_tokens,
    })
}

// ---- Gemini wire format ----
//
// Gemini's `contents`/`parts`/`role` shape uses "model" where Anthropic uses
// "assistant", has no id on a functionCall, and correlates a functionResponse
// back to its call by *name* rather than by id. Our internal `ContentBlock`
// keeps Anthropic's id-based shape (that's the format both binaries are
// written against), so translating out to Gemini means recovering each
// ToolResult's function name from the preceding ToolUse with the same id --
// done here with a simple linear scan, since a conversation is always small.

fn gemini_function_declaration(tool: &ToolDef) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
    })
}

fn messages_to_gemini_contents(messages: &[Message]) -> Vec<Value> {
    let mut id_to_name: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut contents = Vec::with_capacity(messages.len());

    for message in messages {
        let role = if message.role == "assistant" {
            "model"
        } else {
            "user"
        };
        let mut parts = Vec::with_capacity(message.content.len());
        for block in &message.content {
            match block {
                ContentBlock::Text { text } => parts.push(json!({"text": text})),
                ContentBlock::ToolUse {
                    id,
                    name,
                    input,
                    thought_signature,
                } => {
                    id_to_name.insert(id.as_str(), name.as_str());
                    let mut part = json!({"functionCall": {"name": name, "args": input}});
                    // Gemini 3.x rejects a replayed functionCall part that's
                    // missing the signature it originally attached -- see
                    // the ContentBlock::ToolUse doc comment.
                    if let Some(sig) = thought_signature {
                        part["thoughtSignature"] = json!(sig);
                    }
                    parts.push(part);
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                } => {
                    let name = id_to_name
                        .get(tool_use_id.as_str())
                        .copied()
                        .unwrap_or("unknown_tool");
                    parts.push(json!({
                        "functionResponse": {
                            "name": name,
                            "response": {"result": content},
                        }
                    }));
                }
            }
        }
        contents.push(json!({"role": role, "parts": parts}));
    }

    contents
}

fn parse_gemini_response(raw: &Value) -> Result<ApiResponse> {
    let candidate = raw
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .context("response missing `candidates[0]`")?;

    let parts = candidate
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();

    let mut content = Vec::with_capacity(parts.len());
    for part in &parts {
        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
            content.push(ContentBlock::Text {
                text: text.to_string(),
            });
        } else if let Some(call) = part.get("functionCall") {
            let name = call
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            // Newer Gemini models include a real call id; fall back to the
            // name (unique enough for our single-tool usage pattern) for
            // any that don't. Either way this doubles as the id used to
            // correlate the ToolResult sent back (see
            // `messages_to_gemini_contents`).
            let id = call
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| name.clone());
            // `thoughtSignature` is a sibling of `functionCall` on the Part
            // object, not nested inside it -- see ContentBlock::ToolUse.
            let thought_signature = part
                .get("thoughtSignature")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            content.push(ContentBlock::ToolUse {
                id,
                name,
                input: call.get("args").cloned().unwrap_or(Value::Null),
                thought_signature,
            });
        }
    }

    let stop_reason = candidate
        .get("finishReason")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown")
        .to_string();
    let input_tokens = raw
        .get("usageMetadata")
        .and_then(|u| u.get("promptTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let output_tokens = raw
        .get("usageMetadata")
        .and_then(|u| u.get("candidatesTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    Ok(ApiResponse {
        content,
        stop_reason,
        input_tokens,
        output_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_anthropic_text_response() {
        let raw = json!({
            "content": [{"type": "text", "text": "hello"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });
        let resp = parse_anthropic_response(&raw).unwrap();
        assert_eq!(resp.text(), "hello");
        assert_eq!(resp.stop_reason, "end_turn");
        assert_eq!(resp.input_tokens, 10);
        assert_eq!(resp.output_tokens, 5);
    }

    #[test]
    fn parses_an_anthropic_tool_use_response() {
        let raw = json!({
            "content": [
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "toolu_1", "name": "shift_test", "input": {"strategy": "foo"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 8}
        });
        let resp = parse_anthropic_response(&raw).unwrap();
        let uses = resp.tool_uses();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].0, "toolu_1");
        assert_eq!(uses[0].1, "shift_test");
    }

    #[test]
    fn parses_a_gemini_text_response() {
        let raw = json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "hello"}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 12, "candidatesTokenCount": 6}
        });
        let resp = parse_gemini_response(&raw).unwrap();
        assert_eq!(resp.text(), "hello");
        assert_eq!(resp.input_tokens, 12);
        assert_eq!(resp.output_tokens, 6);
    }

    #[test]
    fn parses_a_gemini_function_call_response() {
        let raw = json!({
            "candidates": [{
                "content": {"role": "model", "parts": [
                    {"functionCall": {"name": "shift_test", "args": {"hypothesis": "forward window"}}}
                ]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 30, "candidatesTokenCount": 10}
        });
        let resp = parse_gemini_response(&raw).unwrap();
        let uses = resp.tool_uses();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].1, "shift_test");
    }

    #[test]
    fn gemini_translation_recovers_function_name_for_tool_result() {
        let messages = vec![
            Message::user_text("hi"),
            Message {
                role: "assistant",
                content: vec![ContentBlock::ToolUse {
                    id: "shift_test".to_string(),
                    name: "shift_test".to_string(),
                    input: json!({}),
                    thought_signature: Some("sig123".to_string()),
                }],
            },
            Message {
                role: "user",
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "shift_test".to_string(),
                    content: "delta=0.1".to_string(),
                }],
            },
        ];
        let contents = messages_to_gemini_contents(&messages);
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[1]["parts"][0]["thoughtSignature"], "sig123");
        let function_response = &contents[2]["parts"][0]["functionResponse"];
        assert_eq!(function_response["name"], "shift_test");
        assert_eq!(function_response["response"]["result"], "delta=0.1");
    }

    #[test]
    fn parses_thought_signature_from_a_gemini_function_call_part() {
        let raw = json!({
            "candidates": [{
                "content": {"role": "model", "parts": [
                    {"functionCall": {"name": "shift_test", "args": {}, "id": "call_1"}, "thoughtSignature": "abc"}
                ]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 1}
        });
        let resp = parse_gemini_response(&raw).unwrap();
        match &resp.content[0] {
            ContentBlock::ToolUse {
                id,
                thought_signature,
                ..
            } => {
                assert_eq!(id, "call_1");
                assert_eq!(thought_signature.as_deref(), Some("abc"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }
}
