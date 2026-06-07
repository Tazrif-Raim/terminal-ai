use std::process::ExitCode;

use crate::config::ResolvedConfig;

pub(crate) mod executor;
pub(crate) mod llm;
pub(crate) mod logs;
pub(crate) mod loop_;
pub(crate) mod types;
pub(crate) mod ui;

pub(crate) fn run(
    prompt: &str,
    config: &ResolvedConfig,
    options: types::AgentRunOptions,
) -> ExitCode {
    loop_::run(prompt, config, options)
}

pub(crate) fn show_logs(open_latest: bool) -> ExitCode {
    logs::show_recent(open_latest)
}
