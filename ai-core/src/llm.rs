use std::{fmt, time::Duration};

use reqwest::{StatusCode, blocking::Client};
use serde::{Deserialize, Serialize};

use crate::{
    config::ResolvedConfig,
    prompt::{self, PromptContext},
    types::{CommandOptions, OptionsValidationError},
};

const TEMPERATURE: f32 = 0.1;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub(crate) enum LlmError {
    ClientBuild(reqwest::Error),
    Request(reqwest::Error),
    ApiStatus { status: StatusCode, body: String },
    ApiResponse(serde_json::Error),
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
            Self::Request(source) => write!(formatter, "error: LLM request failed: {source}"),
            Self::ApiStatus { status, body } => {
                write!(formatter, "error: LLM API returned HTTP {status}: {body}")
            }
            Self::ApiResponse(source) => {
                write!(
                    formatter,
                    "error: could not parse LLM API response: {source}"
                )
            }
            Self::EmptyMessage => write!(formatter, "error: LLM response did not include content"),
            Self::ResponseParse(source) => write!(
                formatter,
                "error: could not parse LLM response as command options: {source}"
            ),
            Self::InvalidOptions(source) => {
                write!(
                    formatter,
                    "error: invalid command options from LLM: {source}"
                )
            }
        }
    }
}

impl std::error::Error for LlmError {}

pub(crate) fn generate_options(
    config: &ResolvedConfig,
    request: &str,
) -> Result<CommandOptions, LlmError> {
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(LlmError::ClientBuild)?;

    generate_options_with_sender(config, request, |chat_request| {
        send_chat_request(&client, config, chat_request)
    })
}

fn generate_options_with_sender(
    config: &ResolvedConfig,
    request: &str,
    mut send: impl FnMut(&ChatRequest) -> Result<String, LlmError>,
) -> Result<CommandOptions, LlmError> {
    let chat_request = build_chat_request(config, request);
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

fn build_chat_request(config: &ResolvedConfig, request: &str) -> ChatRequest {
    let context = PromptContext {
        os: prompt::current_os(),
        shell: &config.default_shell,
        max_options: config.max_options,
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
    let response = client
        .post(&config.api_url)
        .bearer_auth(&config.api_key)
        .json(request)
        .send()
        .map_err(LlmError::Request)?;

    let status = response.status();
    let body = response.text().map_err(LlmError::Request)?;

    if !status.is_success() {
        return Err(LlmError::ApiStatus {
            status,
            body: trim_error_body(&body),
        });
    }

    let response: ChatResponse = serde_json::from_str(&body).map_err(LlmError::ApiResponse)?;
    first_message_content(response)
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

#[cfg(test)]
mod tests {
    use super::{
        LlmError, build_chat_request, extract_json, generate_options_with_sender,
        parse_command_options,
    };
    use crate::{config::ResolvedConfig, types::Risk};

    #[test]
    fn builds_openai_compatible_chat_request() {
        let config = config();
        let request = build_chat_request(&config, "what is running on port 3000");

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
    fn retries_once_when_assistant_content_is_malformed_json() {
        let config = config();
        let mut calls = 0;
        let options = generate_options_with_sender(&config, "show processes", |_| {
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
        let error = generate_options_with_sender(&config, "show processes", |_| {
            calls += 1;
            Ok(r#"{"options":[]}"#.to_owned())
        })
        .expect_err("invalid options");

        assert_eq!(calls, 1);
        assert!(matches!(error, LlmError::InvalidOptions(_)));
    }

    fn config() -> ResolvedConfig {
        ResolvedConfig {
            api_url: "https://example.test/v1/chat/completions".to_owned(),
            api_key: "test-key".to_owned(),
            model: "test-model".to_owned(),
            default_shell: "powershell".to_owned(),
            max_options: 3,
        }
    }
}
