use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::types::Risk;

/// One action the LLM wants to take next.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct AgentStep {
    pub(crate) step: usize,
    pub(crate) total_estimated: usize,
    pub(crate) action_type: ActionType,
    pub(crate) command: Option<String>,
    pub(crate) file_path: Option<String>,
    pub(crate) file_content: Option<String>,
    pub(crate) cwd_override: Option<String>,
    pub(crate) risk: Risk,
    pub(crate) reasoning: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum ActionType {
    RunCommand,
    ReadFile,
    WriteFile,
    Done,
    AskUser,
}

/// Output captured after executing one step.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct StepOutput {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) exit_code: i32,
    pub(crate) success: bool,
    pub(crate) duration_ms: u64,
}

/// A completed step with its output, appended to history each iteration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct CompletedStep {
    pub(crate) step: AgentStep,
    pub(crate) output: StepOutput,
}

/// Full mutable state carried through the agent loop.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct AgentState {
    pub(crate) goal: String,
    pub(crate) cwd: PathBuf,
    pub(crate) project_root: PathBuf,
    pub(crate) env: HashMap<String, String>,
    pub(crate) history: Vec<CompletedStep>,
    pub(crate) step_number: usize,
    pub(crate) total_estimated: usize,
    pub(crate) consecutive_failures: usize,
}

impl AgentState {
    pub(crate) fn new(goal: impl Into<String>, cwd: PathBuf, env: HashMap<String, String>) -> Self {
        Self {
            goal: goal.into(),
            project_root: cwd.clone(),
            cwd,
            env,
            history: Vec::new(),
            step_number: 1,
            total_estimated: 1,
            consecutive_failures: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AgentRunOptions {
    pub(crate) dry_run: bool,
}
