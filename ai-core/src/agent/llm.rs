use std::fmt;

use crate::{
    config::ResolvedConfig,
    llm::{self as core_llm, LlmError},
};

use super::types::{ActionType, AgentState, AgentStep};

const OUTPUT_LIMIT: usize = 2_000;
const FULL_HISTORY_STEPS: usize = 5;
const MESSAGE_CHAR_LIMIT: usize = 48_000;

pub(crate) const SYSTEM_PROMPT: &str = r#"You are a terminal agent. You are given a goal and the full history
of steps already completed with their outputs. Return ONLY the next
single action as a JSON object. No markdown. No explanation outside
the JSON.

JSON schema:
{
  "step": <number>,
  "total_estimated": <number>,
  "action_type": "RunCommand" | "ReadFile" | "WriteFile" | "Done" | "AskUser",
  "command": "<shell command>" | null,
  "file_path": "<path>" | null,
  "file_content": "<content>" | null,
  "cwd_override": "<path>" | null,
  "risk": "Safe" | "Dangerous",
  "reasoning": "<one sentence explaining why this step>"
}

Mark risk as Dangerous for: rm -rf, DROP TABLE, format disk,
credential changes, curl piped to sh, overwriting without backup.

When the goal is fully complete, return action_type "Done".
When you need information from the user before continuing, return
action_type "AskUser" and put your question in the reasoning field."#;

#[derive(Debug)]
pub(crate) enum AgentLlmError {
    Request(LlmError),
    InvalidResponse { first: String, second: String },
}

#[derive(Debug)]
enum AgentStepError {
    Json(serde_json::Error),
    Invalid(&'static str),
}

pub(crate) fn next_step(
    config: &ResolvedConfig,
    state: &AgentState,
) -> Result<AgentStep, AgentLlmError> {
    let user_prompt = build_user_message(state);
    let first_content = core_llm::complete_chat(config, SYSTEM_PROMPT, &user_prompt)
        .map_err(AgentLlmError::Request)?;

    match parse_agent_step(&first_content) {
        Ok(step) => Ok(step),
        Err(first) => {
            let second_content = core_llm::complete_chat(config, SYSTEM_PROMPT, &user_prompt)
                .map_err(AgentLlmError::Request)?;
            parse_agent_step(&second_content).map_err(|second| AgentLlmError::InvalidResponse {
                first: first.to_string(),
                second: second.to_string(),
            })
        }
    }
}

pub(crate) fn build_user_message(state: &AgentState) -> String {
    let mut message = format!(
        "Original goal:\n{goal}\n\nCurrent working directory:\n{cwd}\n\nCompleted steps:",
        goal = state.goal.trim(),
        cwd = state.cwd.display(),
    );

    if state.history.is_empty() {
        message.push_str("\nNone yet.");
        return message;
    }

    let full_history_start = state.history.len().saturating_sub(FULL_HISTORY_STEPS);

    for completed in &state.history[..full_history_start] {
        let step = &completed.step;
        let output = &completed.output;
        message.push_str(&format!(
            "\nStep {step_number}: {action:?} {target} -> exit {exit_code}, success={success}",
            step_number = step.step,
            action = step.action_type,
            target = compact_step_target(step),
            exit_code = output.exit_code,
            success = output.success,
        ));
    }

    for completed in &state.history[full_history_start..] {
        let step = &completed.step;
        let output = &completed.output;

        message.push_str(&format!(
            "\n\nStep {step_number}: {action:?}\nReasoning: {reasoning}\n{target}\nExit code: {exit_code}\nSuccess: {success}\nStdout:\n{stdout}\nStderr:\n{stderr}",
            step_number = step.step,
            action = step.action_type,
            reasoning = step.reasoning.trim(),
            target = step_target(step),
            exit_code = output.exit_code,
            success = output.success,
            stdout = trim_for_context(&output.stdout),
            stderr = trim_for_context(&output.stderr),
        ));
    }

    enforce_message_limit(message)
}

fn parse_agent_step(content: &str) -> Result<AgentStep, AgentStepError> {
    let mut step: AgentStep =
        serde_json::from_str(extract_json(content)).map_err(AgentStepError::Json)?;
    normalize_step(&mut step);
    validate_step(&step)?;
    Ok(step)
}

fn normalize_step(step: &mut AgentStep) {
    step.command = trim_option(step.command.take());
    step.file_path = trim_option(step.file_path.take());
    step.cwd_override = trim_option(step.cwd_override.take());
    step.reasoning = step.reasoning.trim().to_owned();
}

fn trim_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_step(step: &AgentStep) -> Result<(), AgentStepError> {
    if step.reasoning.is_empty() {
        return Err(AgentStepError::Invalid("reasoning is required"));
    }

    match step.action_type {
        ActionType::RunCommand if step.command.is_none() => {
            Err(AgentStepError::Invalid("RunCommand requires command"))
        }
        ActionType::ReadFile if step.file_path.is_none() => {
            Err(AgentStepError::Invalid("ReadFile requires file_path"))
        }
        ActionType::WriteFile if step.file_path.is_none() => {
            Err(AgentStepError::Invalid("WriteFile requires file_path"))
        }
        ActionType::WriteFile if step.file_content.is_none() => {
            Err(AgentStepError::Invalid("WriteFile requires file_content"))
        }
        _ => Ok(()),
    }
}

fn step_target(step: &AgentStep) -> String {
    if let Some(command) = &step.command {
        return format!("Command: {command}");
    }

    if let Some(path) = &step.file_path {
        return format!("Path: {path}");
    }

    "Target: none".to_owned()
}

fn compact_step_target(step: &AgentStep) -> String {
    if let Some(command) = &step.command {
        return format!("ran {command}");
    }

    if let Some(path) = &step.file_path {
        return format!("used {path}");
    }

    "completed".to_owned()
}

fn trim_for_context(value: &str) -> String {
    if value.chars().count() <= OUTPUT_LIMIT {
        return value.to_owned();
    }

    let mut trimmed: String = value.chars().take(OUTPUT_LIMIT).collect();
    trimmed.push_str("\n[truncated]");
    trimmed
}

fn enforce_message_limit(message: String) -> String {
    if message.chars().count() <= MESSAGE_CHAR_LIMIT {
        return message;
    }

    let mut trimmed = String::from("[older context truncated]\n");
    trimmed.extend(
        message
            .chars()
            .rev()
            .take(MESSAGE_CHAR_LIMIT)
            .collect::<Vec<_>>()
            .into_iter()
            .rev(),
    );
    trimmed
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

impl fmt::Display for AgentLlmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(source) => write!(formatter, "{source}"),
            Self::InvalidResponse { first, second } => write!(
                formatter,
                "error: model response was not a valid agent step after retry: {first}; {second}"
            ),
        }
    }
}

impl std::error::Error for AgentLlmError {}

impl fmt::Display for AgentStepError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(source) => write!(formatter, "{source}"),
            Self::Invalid(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for AgentStepError {}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use super::{build_user_message, parse_agent_step};
    use crate::{
        agent::types::{ActionType, AgentState, AgentStep, CompletedStep, StepOutput},
        types::Risk,
    };

    #[test]
    fn parses_and_validates_agent_step() {
        let step = parse_agent_step(
            r#"{"step":1,"total_estimated":2,"action_type":"RunCommand","command":" echo hello ","file_path":null,"file_content":null,"cwd_override":null,"risk":"Safe","reasoning":" Inspect cwd "}"#,
        )
        .expect("parse step");

        assert_eq!(step.action_type, ActionType::RunCommand);
        assert_eq!(step.command.as_deref(), Some("echo hello"));
        assert_eq!(step.risk, Risk::Safe);
        assert_eq!(step.reasoning, "Inspect cwd");
    }

    #[test]
    fn rejects_run_command_without_command() {
        let error = parse_agent_step(
            r#"{"step":1,"total_estimated":1,"action_type":"RunCommand","command":null,"file_path":null,"file_content":null,"cwd_override":null,"risk":"Safe","reasoning":"Missing command"}"#,
        )
        .expect_err("missing command");

        assert!(error.to_string().contains("requires command"));
    }

    #[test]
    fn builds_user_message_with_trimmed_history() {
        let mut state = AgentState::new("list files", PathBuf::from("E:\\repo"), HashMap::new());
        state.history.push(CompletedStep {
            step: AgentStep {
                step: 1,
                total_estimated: 2,
                action_type: ActionType::RunCommand,
                command: Some("Get-ChildItem".to_owned()),
                file_path: None,
                file_content: None,
                cwd_override: None,
                risk: Risk::Safe,
                reasoning: "Inspect files".to_owned(),
            },
            output: StepOutput {
                stdout: "x".repeat(2_100),
                stderr: "none".to_owned(),
                exit_code: 0,
                success: true,
                duration_ms: 12,
            },
        });
        state.step_number = 2;
        state.total_estimated = 2;

        let message = build_user_message(&state);

        assert!(message.contains("Original goal:\nlist files"));
        assert!(message.contains("Current working directory:\nE:\\repo"));
        assert!(message.contains("Reasoning: Inspect files"));
        assert!(message.contains("Command: Get-ChildItem"));
        assert!(message.contains("[truncated]"));
    }

    #[test]
    fn summarizes_older_history_and_keeps_recent_steps_full() {
        let mut state = AgentState::new("inspect", PathBuf::from("E:\\repo"), HashMap::new());
        state.step_number = 8;
        state.total_estimated = 8;

        for index in 1..=7 {
            state.history.push(CompletedStep {
                step: AgentStep {
                    step: index,
                    total_estimated: 8,
                    action_type: ActionType::RunCommand,
                    command: Some(format!("cmd-{index}")),
                    file_path: None,
                    file_content: None,
                    cwd_override: None,
                    risk: Risk::Safe,
                    reasoning: format!("reason-{index}"),
                },
                output: StepOutput {
                    stdout: format!("stdout-{index}"),
                    stderr: String::new(),
                    exit_code: 0,
                    success: true,
                    duration_ms: 1,
                },
            });
        }

        let message = build_user_message(&state);

        assert!(message.contains("Step 1: RunCommand ran cmd-1 -> exit 0"));
        assert!(!message.contains("reason-1"));
        assert!(message.contains("Reasoning: reason-7"));
        assert!(message.contains("stdout-7"));
    }

    #[test]
    fn parses_done_step_with_missing_optional_fields() {
        let step = parse_agent_step(
            r#"{"step":1,"total_estimated":1,"action_type":"Done","risk":"Safe","reasoning":"Complete"}"#,
        )
        .expect("parse done step");

        assert_eq!(step.action_type, ActionType::Done);
        assert_eq!(step.command, None);
        assert_eq!(step.file_path, None);
    }
}
