use std::{collections::HashMap, path::PathBuf, time::{Instant, SystemTime, UNIX_EPOCH}, process::Child};

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
    #[serde(default)]
    pub(crate) background: bool,
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

/// Serializable record of a background process for audit logging.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct BackgroundProcessRecord {
    pub(crate) label: String,
    pub(crate) command: String,
    pub(crate) pid: Option<u32>,
    pub(crate) started_at_ms: u64,
    pub(crate) kept_running: bool,
}

/// Full mutable state carried through the agent loop.
/// Note: Does not derive Serialize/Deserialize because BackgroundProcess contains Child.
/// Does not derive Clone/PartialEq/Eq because BackgroundProcess contains Child.
pub(crate) struct AgentState {
    pub(crate) goal: String,
    pub(crate) cwd: PathBuf,
    pub(crate) project_root: PathBuf,
    pub(crate) env: HashMap<String, String>,
    pub(crate) history: Vec<CompletedStep>,
    pub(crate) step_number: usize,
    pub(crate) total_estimated: usize,
    pub(crate) consecutive_failures: usize,
    // Track background processes (live handles, not serializable)
    pub(crate) background_processes: Vec<BackgroundProcess>,
}

impl std::fmt::Debug for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentState")
            .field("goal", &self.goal)
            .field("cwd", &self.cwd)
            .field("project_root", &self.project_root)
            .field("env", &self.env)
            .field("history", &self.history)
            .field("step_number", &self.step_number)
            .field("total_estimated", &self.total_estimated)
            .field("consecutive_failures", &self.consecutive_failures)
            .field("background_processes", &format!("{} background processes", self.background_processes.len()))
            .finish()
    }
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
            background_processes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AgentRunOptions {
    pub(crate) dry_run: bool,
}

/// Live background process with Child handle (not serializable).
pub(crate) struct BackgroundProcess {
    pub(crate) label: String,       // step reasoning, shown to user at exit
    pub(crate) command: String,     // the command that was run
    pub(crate) pid: Option<u32>,    // for display and audit log
    pub(crate) child: Child,        // std::process::Child handle
    pub(crate) started_at: Instant, // for "running 43s" display
}

impl std::fmt::Debug for BackgroundProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundProcess")
            .field("label", &self.label)
            .field("command", &self.command)
            .field("pid", &self.pid)
            .field("started_at", &self.started_at)
            .finish()
    }
}

impl BackgroundProcess {
    pub(crate) fn started_at_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
            - self.started_at.elapsed().as_millis() as u64
    }
}
