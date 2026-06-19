use std::{
    fmt,
    io::{BufRead, BufReader},
    path::PathBuf,
    time::Duration,
};

use reqwest::{StatusCode, blocking::Client};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    codex_oauth,
    config::{ProviderMode, ResolvedConfig},
    context,
    prompt::{self, PromptContext},
    types::{CommandOptions, OptionsValidationError},
};

const TEMPERATURE: f32 = 0.1;
const DEFAULT_CODEX_INSTRUCTIONS: &str =
    "You are ChatGPT, a large language model trained by OpenAI.";
#[derive(Debug)]
pub(crate) enum LlmError {
    ClientBuild(reqwest::Error),
    Request(reqwest::Error),
    Timeout { seconds: u64 },
    ApiStatus { status: StatusCode, message: String },
    ApiResponse(serde_json::Error),
    StreamRead(std::io::Error),
    EmptyMessage,
    ResponseParse(serde_json::Error),
    InvalidOptions(OptionsValidationError),
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    temperature: f32,
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Serialize)]
struct CodexResponsesRequest {
    model: String,
    instructions: String,
    input: Vec<CodexInputMessage>,
    store: bool,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct CodexInputMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

impl fmt::Display for LlmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClientBuild(source) => {
                write!(
                    formatter,
                    "error: could not initialize LLM HTTP client: {source}"
                )
            }
            Self::Request(source) => write!(formatter, "error: could not reach LLM API: {source}"),
            Self::Timeout { seconds } => {
                write!(formatter, "error: LLM request timed out after {seconds}s")
            }
            Self::ApiStatus { status, message } => {
                write!(
                    formatter,
                    "error: LLM API returned HTTP {status}: {message}"
                )
            }
            Self::ApiResponse(source) => {
                write!(
                    formatter,
                    "error: LLM API response was not valid JSON: {source}"
                )
            }
            Self::StreamRead(source) => {
                write!(
                    formatter,
                    "error: could not read streamed LLM response: {source}"
                )
            }
            Self::EmptyMessage => write!(formatter, "error: LLM response did not include content"),
            Self::ResponseParse(source) => write!(
                formatter,
                "error: model response was not valid command JSON: {source}"
            ),
            Self::InvalidOptions(source) => {
                write!(
                    formatter,
                    "error: model response contained invalid command options: {source}"
                )
            }
        }
    }
}

impl std::error::Error for LlmError {}

pub(crate) fn generate_options(
    config: &ResolvedConfig,
    request: &str,
    files: &[PathBuf],
) -> Result<CommandOptions, LlmError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(config.request_timeout_seconds))
        .build()
        .map_err(LlmError::ClientBuild)?;

    generate_options_with_sender(config, request, files, |chat_request| {
        send_chat_request(&client, config, chat_request)
    })
}

pub(crate) fn complete_chat(
    config: &ResolvedConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, LlmError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(config.request_timeout_seconds))
        .build()
        .map_err(LlmError::ClientBuild)?;

    let request = ChatRequest {
        model: config.model.clone(),
        temperature: TEMPERATURE,
        messages: vec![
            ChatMessage {
                role: "system",
                content: system_prompt.to_owned(),
            },
            ChatMessage {
                role: "user",
                content: user_prompt.to_owned(),
            },
        ],
    };

    send_chat_request(&client, config, &request)
}

fn generate_options_with_sender(
    config: &ResolvedConfig,
    request: &str,
    files: &[PathBuf],
    mut send: impl FnMut(&ChatRequest) -> Result<String, LlmError>,
) -> Result<CommandOptions, LlmError> {
    let chat_request = build_chat_request(config, request, files);
    let first_content = send(&chat_request)?;

    match parse_command_options(&first_content, config.max_options) {
        Ok(options) => Ok(options),
        Err(LlmError::ResponseParse(_)) => {
            let retry_content = send(&chat_request)?;
            parse_command_options(&retry_content, config.max_options)
        }
        Err(error) => Err(error),
    }
}

fn build_chat_request(config: &ResolvedConfig, request: &str, files: &[PathBuf]) -> ChatRequest {
    let shell_context = context::collect(config, files);
    let shell = shell_context.shell_label();
    let context = PromptContext {
        os: &shell_context.os,
        shell: &shell,
        max_options: config.max_options,
        shell_context: config.send_context.then_some(&shell_context),
    };

    ChatRequest {
        model: config.model.clone(),
        temperature: TEMPERATURE,
        messages: vec![
            ChatMessage {
                role: "system",
                content: prompt::system_prompt(context),
            },
            ChatMessage {
                role: "user",
                content: prompt::user_prompt(request, context),
            },
        ],
    }
}

fn send_chat_request(
    client: &Client,
    config: &ResolvedConfig,
    request: &ChatRequest,
) -> Result<String, LlmError> {
    if config.provider_mode == ProviderMode::CodexOAuth {
        return send_codex_responses_request(client, config, request);
    }

    let mut req = client.post(&config.api_url).bearer_auth(&config.api_key);

    // Only send JSON body for non-HEAD/non-GET requests
    req = req.json(request);

    let response = req
        .send()
        .map_err(|source| request_error(source, config.request_timeout_seconds))?;

    let status = response.status();
    let body = response
        .text()
        .map_err(|source| request_error(source, config.request_timeout_seconds))?;

    if !status.is_success() {
        return Err(LlmError::ApiStatus {
            status,
            message: clean_api_error_message(&body),
        });
    }

    let response: ChatResponse = serde_json::from_str(&body).map_err(LlmError::ApiResponse)?;
    first_message_content(response)
}

fn send_codex_responses_request(
    client: &Client,
    config: &ResolvedConfig,
    request: &ChatRequest,
) -> Result<String, LlmError> {
    let token = codex_oauth::default_store_path()
        .map_err(|e| LlmError::ApiStatus {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("Codex OAuth: {e}"),
        })
        .and_then(|store_path| {
            codex_oauth::get_token(&store_path).map_err(|e| LlmError::ApiStatus {
                status: StatusCode::UNAUTHORIZED,
                message: format!("Codex OAuth: {e}"),
            })
        })?;
    let codex_request = codex_responses_request(config, request);

    let mut req = client
        .post(codex_oauth::CHATGPT_CODEX_RESPONSES_URL)
        .bearer_auth(&token.access_token)
        .header(
            codex_oauth::ORIGINATOR_HEADER,
            codex_oauth::ORIGINATOR_VALUE,
        )
        .json(&codex_request);

    if let Some(account_id) = token.account_id.as_deref() {
        req = req.header(codex_oauth::CHATGPT_ACCOUNT_ID_HEADER, account_id);
    }

    let response = req
        .send()
        .map_err(|source| request_error(source, config.request_timeout_seconds))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .map_err(|source| request_error(source, config.request_timeout_seconds))?;
        return Err(LlmError::ApiStatus {
            status,
            message: clean_api_error_message(&body),
        });
    }

    parse_codex_stream(response)
}

fn codex_responses_request(
    config: &ResolvedConfig,
    request: &ChatRequest,
) -> CodexResponsesRequest {
    let mut instructions = Vec::new();
    let mut input = Vec::new();

    for message in &request.messages {
        match message.role {
            "system" | "developer" => instructions.push(message.content.as_str()),
            "assistant" => input.push(CodexInputMessage {
                role: "assistant",
                content: message.content.clone(),
            }),
            _ => input.push(CodexInputMessage {
                role: "user",
                content: message.content.clone(),
            }),
        }
    }

    CodexResponsesRequest {
        model: config.model.clone(),
        instructions: if instructions.is_empty() {
            DEFAULT_CODEX_INSTRUCTIONS.to_owned()
        } else {
            instructions.join("\n\n")
        },
        input,
        store: false,
        stream: true,
    }
}

fn parse_codex_stream(response: reqwest::blocking::Response) -> Result<String, LlmError> {
    let mut output = String::new();
    let reader = BufReader::new(response);

    for line in reader.lines() {
        let line = line.map_err(LlmError::StreamRead)?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let value: Value = serde_json::from_str(data).map_err(LlmError::ApiResponse)?;
        if let Some(error) = value.get("error") {
            return Err(LlmError::ApiStatus {
                status: StatusCode::BAD_GATEWAY,
                message: clean_api_error_message(&json!({ "error": error }).to_string()),
            });
        }
        if let Some(delta) = response_text_delta(&value) {
            output.push_str(delta);
        }
    }

    let output = output.trim().to_owned();
    if output.is_empty() {
        Err(LlmError::EmptyMessage)
    } else {
        Ok(output)
    }
}

fn response_text_delta(value: &Value) -> Option<&str> {
    let event_type = value.get("type").and_then(Value::as_str);
    if event_type.is_some_and(|event_type| event_type.ends_with(".done")) {
        return None;
    }

    value.get("delta").and_then(Value::as_str)
}

fn request_error(source: reqwest::Error, timeout_seconds: u64) -> LlmError {
    if source.is_timeout() {
        LlmError::Timeout {
            seconds: timeout_seconds,
        }
    } else {
        LlmError::Request(source)
    }
}

fn first_message_content(response: ChatResponse) -> Result<String, LlmError> {
    response
        .choices
        .into_iter()
        .map(|choice| choice.message.content.trim().to_owned())
        .find(|content| !content.is_empty())
        .ok_or(LlmError::EmptyMessage)
}

fn parse_command_options(content: &str, max_options: usize) -> Result<CommandOptions, LlmError> {
    let json = extract_json(content);
    let parsed: CommandOptions = serde_json::from_str(json).map_err(LlmError::ResponseParse)?;

    parsed
        .normalize(max_options)
        .map_err(LlmError::InvalidOptions)
}

fn extract_json(content: &str) -> &str {
    let trimmed = content.trim();

    if let Some(fenced) = extract_fenced_block(trimmed) {
        return fenced.trim();
    }

    match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if start <= end => &trimmed[start..=end],
        _ => trimmed,
    }
}

fn extract_fenced_block(content: &str) -> Option<&str> {
    let fence_start = content.find("```")?;
    let after_fence = &content[fence_start + 3..];
    let content_start = after_fence.find('\n')? + 1;
    let fenced_content = &after_fence[content_start..];
    let fence_end = fenced_content.find("```")?;

    Some(&fenced_content[..fence_end])
}

fn trim_error_body(body: &str) -> String {
    const MAX_LEN: usize = 500;

    let body = body.trim();
    if body.chars().count() <= MAX_LEN {
        return body.to_owned();
    }

    let mut truncated: String = body.chars().take(MAX_LEN).collect();
    truncated.push_str("...");
    truncated
}

fn clean_api_error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body)
        && let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .or_else(|| value.get("message").and_then(Value::as_str))
            .map(str::trim)
            .filter(|message| !message.is_empty())
    {
        return trim_error_body(message);
    }

    trim_error_body(body)
}

#[cfg(test)]
mod tests {
    use super::{
        LlmError, build_chat_request, clean_api_error_message, extract_json,
        generate_options_with_sender, parse_command_options,
    };
    use crate::{config::ResolvedConfig, types::Risk};

    #[test]
    fn builds_openai_compatible_chat_request() {
        let config = config();
        let request = build_chat_request(&config, "what is running on port 3000", &[]);

        assert_eq!(request.model, "test-model");
        assert_eq!(request.temperature, 0.1);
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, "system");
        assert_eq!(request.messages[1].role, "user");
        assert!(request.messages[0].content.contains("Return JSON only"));
        assert!(
            request.messages[1]
                .content
                .contains("what is running on port 3000")
        );
    }

    #[test]
    fn parses_raw_json_options() {
        let options = parse_command_options(
            r#"{"options":[{"title":"Show port","command":"Get-NetTCPConnection -LocalPort 3000","risk":"safe"}]}"#,
            3,
        )
        .expect("parse options");

        assert_eq!(options.options.len(), 1);
        assert_eq!(options.options[0].risk, Risk::Safe);
    }

    #[test]
    fn parses_fenced_json_options() {
        let options = parse_command_options(
            r#"```json
{"options":[{"title":"Show port","command":"Get-NetTCPConnection -LocalPort 3000","risk":"safe"}]}
```"#,
            3,
        )
        .expect("parse options");

        assert_eq!(options.options.len(), 1);
    }

    #[test]
    fn extracts_json_from_text() {
        assert_eq!(
            extract_json(r#"Here is JSON: {"options":[]} done"#),
            r#"{"options":[]}"#
        );
    }

    #[test]
    fn extracts_clean_api_error_message() {
        assert_eq!(
            clean_api_error_message(r#"{"error":{"message":"invalid api key"}}"#),
            "invalid api key"
        );
    }

    #[test]
    fn retries_once_when_assistant_content_is_malformed_json() {
        let config = config();
        let mut calls = 0;
        let options = generate_options_with_sender(&config, "show processes", &[], |_| {
            calls += 1;
            if calls == 1 {
                Ok("not json".to_owned())
            } else {
                Ok(r#"{"options":[{"title":"Show processes","command":"Get-Process","risk":"safe"}]}"#.to_owned())
            }
        })
        .expect("retry and parse");

        assert_eq!(calls, 2);
        assert_eq!(options.options[0].command, "Get-Process");
    }

    #[test]
    fn does_not_retry_invalid_options() {
        let config = config();
        let mut calls = 0;
        let error = generate_options_with_sender(&config, "show processes", &[], |_| {
            calls += 1;
            Ok(r#"{"options":[]}"#.to_owned())
        })
        .expect_err("invalid options");

        assert_eq!(calls, 1);
        assert!(matches!(error, LlmError::InvalidOptions(_)));
    }

    fn config() -> ResolvedConfig {
        ResolvedConfig {
            provider_mode: crate::config::ProviderMode::OpenAiCompatible,
            api_url: "https://example.test/v1/chat/completions".to_owned(),
            api_key: "test-key".to_owned(),
            model: "test-model".to_owned(),
            default_shell: "powershell".to_owned(),
            max_options: 3,
            dangerous_requires_confirm: true,
            send_context: true,
            send_recent_commands: true,
            max_recent_commands: 5,
            request_timeout_seconds: 60,
            telemetry_enabled: false,
            hide_descriptions: false,
        }
    }
}
