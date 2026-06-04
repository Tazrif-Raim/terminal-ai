mod config;
mod llm;
mod picker;
mod prompt;
mod safety;
mod types;

use std::process::ExitCode;

use clap::Parser;

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

    /// Print debug diagnostics to stderr.
    #[arg(long)]
    debug: bool,

    /// Natural language command request.
    #[arg(
        value_name = "PROMPT",
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    prompt: Vec<String>,
}

fn main() -> ExitCode {
    run(Cli::parse())
}

fn run(cli: Cli) -> ExitCode {
    let prompt = prompt::join_parts(&cli.prompt);

    if prompt.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    if cli.debug {
        eprintln!("debug: shell_mode={}, prompt={:?}", cli.shell_mode, prompt);
    }

    if !cli.shell_mode {
        eprintln!("Prompt: {prompt}");
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn parses_unquoted_prompt_words() {
        let cli = Cli::parse_from(["ai-core", "what", "is", "running", "on", "port", "3000"]);

        assert!(!cli.shell_mode);
        assert!(!cli.debug);
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
        assert!(cli.debug);
        assert_eq!(cli.prompt, ["what", "is", "running"]);
    }

    #[test]
    fn treats_words_after_separator_as_prompt() {
        let cli = Cli::parse_from(["ai-core", "--shell-mode", "--", "--version", "meaning"]);

        assert!(cli.shell_mode);
        assert_eq!(cli.prompt, ["--version", "meaning"]);
    }
}
