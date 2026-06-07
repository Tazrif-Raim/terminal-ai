use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct CommandOptions {
    pub(crate) options: Vec<CommandOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct CommandOption {
    pub(crate) title: String,
    pub(crate) command: String,
    pub(crate) risk: Risk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Risk {
    #[serde(alias = "Safe")]
    Safe,
    #[serde(alias = "Dangerous")]
    Dangerous,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub(crate) enum PickerResult {
    Run { command: String },
    Edit { command: String },
    Copy { command: String },
    Regenerate,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OptionsValidationError {
    Empty,
    EmptyTitle { index: usize },
    EmptyCommand { index: usize },
}

impl CommandOptions {
    pub(crate) fn normalize(mut self, max_options: usize) -> Result<Self, OptionsValidationError> {
        if self.options.is_empty() {
            return Err(OptionsValidationError::Empty);
        }

        let max_options = max_options.clamp(1, 3);
        self.options.truncate(max_options);

        for (index, option) in self.options.iter_mut().enumerate() {
            option.title = option.title.trim().to_owned();
            option.command = option.command.trim().to_owned();

            if option.title.is_empty() {
                return Err(OptionsValidationError::EmptyTitle { index: index + 1 });
            }

            if option.command.is_empty() {
                return Err(OptionsValidationError::EmptyCommand { index: index + 1 });
            }
        }

        Ok(self)
    }
}

impl fmt::Display for Risk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Safe => write!(formatter, "safe"),
            Self::Dangerous => write!(formatter, "dangerous"),
        }
    }
}

impl PickerResult {
    pub(crate) fn run(command: impl Into<String>) -> Self {
        Self::Run {
            command: command.into(),
        }
    }

    pub(crate) fn edit(command: impl Into<String>) -> Self {
        Self::Edit {
            command: command.into(),
        }
    }

    pub(crate) fn copy(command: impl Into<String>) -> Self {
        Self::Copy {
            command: command.into(),
        }
    }

    pub(crate) fn regenerate() -> Self {
        Self::Regenerate
    }

    pub(crate) fn cancel() -> Self {
        Self::Cancel
    }

    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string(self).expect("picker result serializes")
    }
}

impl fmt::Display for OptionsValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "response did not include any command options"),
            Self::EmptyTitle { index } => {
                write!(formatter, "option {index} has an empty title")
            }
            Self::EmptyCommand { index } => {
                write!(formatter, "option {index} has an empty command")
            }
        }
    }
}

impl std::error::Error for OptionsValidationError {}

#[cfg(test)]
mod tests {
    use super::{CommandOption, CommandOptions, OptionsValidationError, PickerResult, Risk};

    #[test]
    fn normalizes_and_limits_options() {
        let options = CommandOptions {
            options: vec![
                option(" First ", " Get-Process ", Risk::Safe),
                option("Second", "Get-Service", Risk::Safe),
                option("Third", "Get-Location", Risk::Safe),
                option("Fourth", "Get-ChildItem", Risk::Safe),
            ],
        }
        .normalize(3)
        .expect("valid options");

        assert_eq!(options.options.len(), 3);
        assert_eq!(options.options[0].title, "First");
        assert_eq!(options.options[0].command, "Get-Process");
    }

    #[test]
    fn rejects_empty_options() {
        let error = CommandOptions { options: vec![] }
            .normalize(3)
            .expect_err("empty options");

        assert_eq!(error, OptionsValidationError::Empty);
    }

    #[test]
    fn serializes_picker_results_for_shell_wrapper() {
        assert_eq!(
            PickerResult::run("Get-Process").to_json(),
            r#"{"action":"run","command":"Get-Process"}"#
        );
        assert_eq!(
            PickerResult::edit("Get-Process").to_json(),
            r#"{"action":"edit","command":"Get-Process"}"#
        );
        assert_eq!(
            PickerResult::copy("Get-Process").to_json(),
            r#"{"action":"copy","command":"Get-Process"}"#
        );
        assert_eq!(
            PickerResult::regenerate().to_json(),
            r#"{"action":"regenerate"}"#
        );
        assert_eq!(PickerResult::cancel().to_json(), r#"{"action":"cancel"}"#);
    }

    fn option(title: &str, command: &str, risk: Risk) -> CommandOption {
        CommandOption {
            title: title.to_owned(),
            command: command.to_owned(),
            risk,
        }
    }
}
