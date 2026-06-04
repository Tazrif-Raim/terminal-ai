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

    /// Print the resolved config with secrets redacted.
    #[arg(long)]
    print_config: bool,

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
    if cli.print_config {
        return print_config();
    }

    let prompt = prompt::join_parts(&cli.prompt);

    if prompt.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let resolved_config = match config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };

    if cli.debug {
        eprintln!("debug: shell_mode={}, prompt={:?}", cli.shell_mode, prompt);
        eprintln!("debug: config={:?}", resolved_config.redacted());
    }

    if !cli.shell_mode {
        eprintln!("Prompt: {prompt}");
    }

    let options = match llm::generate_options(&resolved_config, &prompt) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    let options = safety::apply_overrides(options);

    let result = match picker::pick(&options, resolved_config.dangerous_requires_confirm) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };

    if cli.shell_mode {
        println!("{}", result.to_json());
    } else {
        eprintln!("{}", result.to_json());
    }

    ExitCode::SUCCESS
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

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn parses_unquoted_prompt_words() {
        let cli = Cli::parse_from(["ai-core", "what", "is", "running", "on", "port", "3000"]);

        assert!(!cli.shell_mode);
        assert!(!cli.debug);
        assert!(!cli.print_config);
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
        assert!(!cli.print_config);
        assert_eq!(cli.prompt, ["what", "is", "running"]);
    }

    #[test]
    fn treats_words_after_separator_as_prompt() {
        let cli = Cli::parse_from(["ai-core", "--shell-mode", "--", "--version", "meaning"]);

        assert!(cli.shell_mode);
        assert_eq!(cli.prompt, ["--version", "meaning"]);
    }

    #[test]
    fn parses_print_config_flag_without_prompt() {
        let cli = Cli::parse_from(["ai-core", "--print-config"]);

        assert!(cli.print_config);
        assert!(cli.prompt.is_empty());
    }
}
