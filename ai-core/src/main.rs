mod agent;
mod config;
mod context;
mod history;
mod llm;
mod loading;
mod picker;
mod prompt;
mod safety;
mod types;

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Parser;

use crate::types::PickerResult;

const USAGE: &str = "Usage: ai <what do you want to do?>";

#[derive(Debug, Parser)]
#[command(
    name = "ai-core",
    version,
    about = "Terminal-native AI command assistant"
)]
struct Cli {
    /// Print only final machine-readable output to stdout for shell wrappers.
    #[arg(long)]
    shell_mode: bool,

    /// Run the experimental agent workflow.
    #[arg(long)]
    agent: bool,

    /// Preview agent steps without executing them.
    #[arg(long)]
    dry_run: bool,

    /// List recent agent audit logs. Pass "open" to open the latest log.
    #[arg(long)]
    agent_logs: bool,

    /// Print debug diagnostics to stderr.
    #[arg(long)]
    debug: bool,

    /// Print the resolved config with secrets redacted.
    #[arg(long)]
    print_config: bool,

    /// View and edit LLM configuration.
    #[arg(long)]
    config: bool,

    /// Include explicit text file contents in the LLM context.
    #[arg(long, value_name = "FILE", num_args = 1.., value_terminator = "--")]
    files: Vec<PathBuf>,

    /// Natural language command request.
    #[arg(value_name = "PROMPT", allow_hyphen_values = true)]
    prompt: Vec<String>,
}

fn main() -> ExitCode {
    run(Cli::parse())
}

fn run(cli: Cli) -> ExitCode {
    if cli.agent_logs {
        return agent::show_logs(cli.prompt.first().is_some_and(|arg| arg == "open"));
    }

    if cli.dry_run && !cli.agent {
        eprintln!("error: --dry-run requires --agent");
        return ExitCode::from(2);
    }

    if cli.print_config {
        return print_config();
    }

    if cli.config {
        return configure();
    }

    let input = normalize_cli_input(cli.prompt, cli.files);

    let prompt = prompt::join_parts(&input.prompt);

    if prompt.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let resolved_config = match load_config_for_prompt(cli.shell_mode) {
        Ok(Some(config)) => config,
        Ok(None) => return print_result(PickerResult::cancel(), cli.shell_mode),
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };

    if cli.debug {
        eprintln!("debug: shell_mode={}, prompt={:?}", cli.shell_mode, prompt);
        eprintln!("debug: config={:?}", resolved_config.redacted());
    }

    if cli.agent {
        return agent::run(
            &prompt,
            &resolved_config,
            agent::types::AgentRunOptions {
                dry_run: cli.dry_run,
            },
        );
    }

    if !cli.shell_mode {
        eprintln!("Prompt: {prompt}");
    }

    loop {
        let options = match loading::generate_options(&resolved_config, &prompt, &input.files) {
            Ok(loading::LoadingResult::Options(options)) => options,
            Ok(loading::LoadingResult::Cancelled) => {
                return print_result(PickerResult::cancel(), cli.shell_mode);
            }
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(1);
            }
        };
        let options = safety::apply_overrides(options);
        history::record_prompt_response(&prompt, &options);

        let result = match picker::pick(
            &options,
            resolved_config.dangerous_requires_confirm,
            resolved_config.hide_descriptions,
        ) {
            Ok(PickerResult::Regenerate) => continue,
            Ok(result) => result,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(1);
            }
        };

        history::record_command(&result);
        return print_result(result, cli.shell_mode);
    }
}

fn print_result(result: PickerResult, shell_mode: bool) -> ExitCode {
    if shell_mode {
        println!("{}", result.to_json());
    } else {
        eprintln!("{}", result.to_json());
    }

    ExitCode::SUCCESS
}

fn load_config_for_prompt(
    shell_mode: bool,
) -> Result<Option<config::ResolvedConfig>, config::ConfigError> {
    match config::load() {
        Ok(config) => Ok(Some(config)),
        Err(error @ config::ConfigError::Missing { .. }) => {
            if !config::can_configure_interactively() {
                return Err(error);
            }

            match config::configure_interactive(config::ConfigEditMode::MissingOnly) {
                Ok(config) => Ok(Some(config)),
                Err(config::ConfigError::Cancelled) if shell_mode => Ok(None),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct CliInput {
    prompt: Vec<String>,
    files: Vec<PathBuf>,
}

fn normalize_cli_input(prompt: Vec<String>, files: Vec<PathBuf>) -> CliInput {
    if !files.is_empty() {
        return CliInput { prompt, files };
    }

    let Some(files_flag_index) = prompt.iter().position(|part| part == "--files") else {
        return CliInput { prompt, files };
    };

    let prompt_parts = &prompt[..files_flag_index];
    let file_parts = &prompt[(files_flag_index + 1)..];

    if file_parts.is_empty() || !file_parts.iter().any(|part| looks_like_file_arg(part)) {
        return CliInput { prompt, files };
    }

    CliInput {
        prompt: prompt_parts.to_vec(),
        files: file_parts.iter().map(PathBuf::from).collect(),
    }
}

fn looks_like_file_arg(value: &str) -> bool {
    let path = Path::new(value);
    path.exists() || path.extension().is_some() || value.contains(['/', '\\'])
}

fn print_config() -> ExitCode {
    let (config, path) = match config::load_for_display() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };

    println!("{}", config::to_pretty_json(&config.redacted()));

    match config.validate(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn configure() -> ExitCode {
    match config::configure_interactive(config::ConfigEditMode::All) {
        Ok(config) => {
            println!("{}", config::to_pretty_json(&config.redacted()));
            ExitCode::SUCCESS
        }
        Err(config::ConfigError::Cancelled) => ExitCode::from(130),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, normalize_cli_input};
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn parses_unquoted_prompt_words() {
        let cli = Cli::parse_from(["ai-core", "what", "is", "running", "on", "port", "3000"]);

        assert!(!cli.shell_mode);
        assert!(!cli.agent);
        assert!(!cli.dry_run);
        assert!(!cli.agent_logs);
        assert!(!cli.debug);
        assert!(!cli.print_config);
        assert!(!cli.config);
        assert!(cli.files.is_empty());
        assert_eq!(cli.prompt, ["what", "is", "running", "on", "port", "3000"]);
    }

    #[test]
    fn parses_shell_mode_and_debug_flags() {
        let cli = Cli::parse_from([
            "ai-core",
            "--shell-mode",
            "--debug",
            "what",
            "is",
            "running",
        ]);

        assert!(cli.shell_mode);
        assert!(!cli.agent);
        assert!(!cli.dry_run);
        assert!(!cli.agent_logs);
        assert!(cli.debug);
        assert!(!cli.print_config);
        assert!(!cli.config);
        assert!(cli.files.is_empty());
        assert_eq!(cli.prompt, ["what", "is", "running"]);
    }

    #[test]
    fn treats_words_after_separator_as_prompt() {
        let cli = Cli::parse_from(["ai-core", "--shell-mode", "--", "--version", "meaning"]);

        assert!(cli.shell_mode);
        assert!(!cli.agent);
        assert!(!cli.dry_run);
        assert!(!cli.agent_logs);
        assert_eq!(cli.prompt, ["--version", "meaning"]);
    }

    #[test]
    fn parses_agent_flag() {
        let cli = Cli::parse_from(["ai-core", "--agent", "hello"]);

        assert!(cli.agent);
        assert!(!cli.dry_run);
        assert!(!cli.shell_mode);
        assert_eq!(cli.prompt, ["hello"]);
    }

    #[test]
    fn parses_agent_dry_run_flag() {
        let cli = Cli::parse_from(["ai-core", "--agent", "--dry-run", "hello"]);

        assert!(cli.agent);
        assert!(cli.dry_run);
        assert_eq!(cli.prompt, ["hello"]);
    }

    #[test]
    fn parses_agent_logs_flag() {
        let cli = Cli::parse_from(["ai-core", "--agent-logs", "open"]);

        assert!(cli.agent_logs);
        assert_eq!(cli.prompt, ["open"]);
    }

    #[test]
    fn parses_files_before_prompt_separator() {
        let cli = Cli::parse_from([
            "ai-core",
            "--files",
            "README.md",
            "docs/TODO.md",
            "--",
            "summarize",
            "these",
        ]);

        assert_eq!(
            cli.files,
            [PathBuf::from("README.md"), PathBuf::from("docs/TODO.md")]
        );
        assert_eq!(cli.prompt, ["summarize", "these"]);
    }

    #[test]
    fn parses_files_after_prompt() {
        let cli = Cli::parse_from([
            "ai-core",
            "summarize",
            "these",
            "files",
            "--files",
            "README.md",
            "docs/TODO.md",
        ]);
        let input = normalize_cli_input(cli.prompt, cli.files);

        assert_eq!(
            input.files,
            [PathBuf::from("README.md"), PathBuf::from("docs/TODO.md")]
        );
        assert_eq!(input.prompt, ["summarize", "these", "files"]);
    }

    #[test]
    fn keeps_files_flag_as_prompt_when_it_does_not_look_like_file_input() {
        let cli = Cli::parse_from(["ai-core", "what", "does", "--files", "mean"]);
        let input = normalize_cli_input(cli.prompt, cli.files);

        assert!(input.files.is_empty());
        assert_eq!(input.prompt, ["what", "does", "--files", "mean"]);
    }

    #[test]
    fn parses_print_config_flag_without_prompt() {
        let cli = Cli::parse_from(["ai-core", "--print-config"]);

        assert!(cli.print_config);
        assert!(!cli.config);
        assert!(cli.prompt.is_empty());
    }

    #[test]
    fn parses_config_flag_without_prompt() {
        let cli = Cli::parse_from(["ai-core", "--config"]);

        assert!(cli.config);
        assert!(cli.prompt.is_empty());
    }
}
