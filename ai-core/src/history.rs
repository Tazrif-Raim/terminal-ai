use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use directories::BaseDirs;
use serde::Serialize;

use crate::types::{CommandOption, CommandOptions, PickerResult};

const HISTORY_DIR: &str = "terminal-ai";
const COMMAND_HISTORY_FILE: &str = "commands.jsonl";
const PROMPT_RESPONSE_HISTORY_FILE: &str = "prompt-responses.jsonl";

#[derive(Serialize)]
struct PromptResponseEntry<'a> {
    timestamp_unix: u64,
    prompt: &'a str,
    options: &'a [CommandOption],
}

#[derive(Serialize)]
struct CommandEntry<'a> {
    timestamp_unix: u64,
    action: &'static str,
    command: &'a str,
}

pub(crate) fn record_prompt_response(prompt: &str, options: &CommandOptions) {
    let entry = PromptResponseEntry {
        timestamp_unix: timestamp_unix(),
        prompt,
        options: &options.options,
    };

    let _ = append_json_line(PROMPT_RESPONSE_HISTORY_FILE, &entry);
}

pub(crate) fn record_command(result: &PickerResult) {
    let Some((action, command)) = command_result(result) else {
        return;
    };

    let entry = CommandEntry {
        timestamp_unix: timestamp_unix(),
        action,
        command,
    };

    let _ = append_json_line(COMMAND_HISTORY_FILE, &entry);
}

fn command_result(result: &PickerResult) -> Option<(&'static str, &str)> {
    match result {
        PickerResult::Run { command } => Some(("run", command)),
        PickerResult::Edit { command } => Some(("edit", command)),
        PickerResult::Copy { command } => Some(("copy", command)),
        PickerResult::Regenerate | PickerResult::Cancel => None,
    }
}

fn append_json_line<T: Serialize>(file_name: &str, value: &T) -> io::Result<()> {
    let Some(dir) = history_dir() else {
        return Ok(());
    };

    fs::create_dir_all(&dir)?;
    let path = dir.join(file_name);
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    serde_json::to_writer(&mut file, value).map_err(io::Error::other)?;
    writeln!(file)
}

fn history_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.data_dir().join(HISTORY_DIR))
}

fn timestamp_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::command_result;
    use crate::types::PickerResult;

    #[test]
    fn records_only_command_bearing_results() {
        assert_eq!(
            command_result(&PickerResult::run("Get-Process")),
            Some(("run", "Get-Process"))
        );
        assert_eq!(
            command_result(&PickerResult::copy("Get-Process")),
            Some(("copy", "Get-Process"))
        );
        assert_eq!(command_result(&PickerResult::cancel()), None);
        assert_eq!(command_result(&PickerResult::regenerate()), None);
    }
}
