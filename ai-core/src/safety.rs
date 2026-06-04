use std::sync::OnceLock;

use regex::Regex;

use crate::types::{CommandOptions, Risk};

static DANGEROUS_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

pub(crate) fn apply_overrides(mut options: CommandOptions) -> CommandOptions {
    for option in &mut options.options {
        if option.risk == Risk::Dangerous || is_dangerous_command(&option.command) {
            option.risk = Risk::Dangerous;
        }
    }

    options
        .options
        .sort_by_key(|option| option.risk == Risk::Dangerous);
    options
}

pub(crate) fn is_dangerous_command(command: &str) -> bool {
    dangerous_patterns()
        .iter()
        .any(|pattern| pattern.is_match(command))
}

fn dangerous_patterns() -> &'static [Regex] {
    DANGEROUS_PATTERNS.get_or_init(|| {
        [
            r"(?i)\bremove-item\b",
            r"(?i)\brm\s+-(?:[[:alnum:]]*r[[:alnum:]]*f[[:alnum:]]*|[[:alnum:]]*f[[:alnum:]]*r[[:alnum:]]*)\b",
            r"(?i)\brm\s+-r\b[^\r\n;|&]*\s+-f\b",
            r"(?i)\brm\s+-f\b[^\r\n;|&]*\s+-r\b",
            r"(?i)\bdel\s+/s\b",
            r"(?i)\bformat(?:\.com)?\b",
            r"(?i)\bdiskpart\b",
            r"(?i)\bgit\s+reset\b[^\r\n;|&]*--hard\b",
            r"(?i)\bgit\s+clean\b[^\r\n;|&]*-(?:[[:alnum:]]*f[[:alnum:]]*d[[:alnum:]]*|[[:alnum:]]*d[[:alnum:]]*f[[:alnum:]]*)\b",
            r"(?i)\bgit\s+clean\b[^\r\n;|&]*-f\b[^\r\n;|&]*-d\b",
            r"(?i)\bgit\s+clean\b[^\r\n;|&]*-d\b[^\r\n;|&]*-f\b",
            r"(?i)\bstop-process\b",
            r"(?i)\btaskkill\b",
            r"(?i)\bdocker\s+system\s+prune\b",
            r"(?i)\bdrop\s+database\b",
            r"(?i)\bkubectl\s+delete\b",
            r"(?i)\bterraform\s+destroy\b",
        ]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("dangerous command pattern compiles"))
        .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::{apply_overrides, is_dangerous_command};
    use crate::types::{CommandOption, CommandOptions, Risk};

    #[test]
    fn detects_dangerous_patterns() {
        for command in [
            "Remove-Item .\\target -Recurse -Force",
            "rm -rf /tmp/example",
            "rm -rfx /tmp/example",
            "del /s *.tmp",
            "format C:",
            "diskpart",
            "git reset --hard HEAD",
            "git clean -fd",
            "git clean -fdx",
            "Stop-Process -Id 1234 -Force",
            "taskkill /PID 1234 /F",
            "docker system prune -af",
            "DROP DATABASE app",
            "kubectl delete pod app",
            "terraform destroy",
        ] {
            assert!(is_dangerous_command(command), "{command}");
        }
    }

    #[test]
    fn leaves_safe_diagnostic_commands_alone() {
        for command in [
            "Get-ChildItem -Recurse -Depth 2",
            "Get-NetTCPConnection -LocalPort 3000",
            "git status --short",
            "docker ps",
        ] {
            assert!(!is_dangerous_command(command), "{command}");
        }
    }

    #[test]
    fn applies_llm_risk_and_local_overrides() {
        let options = apply_overrides(CommandOptions {
            options: vec![
                option("Local override", "Stop-Process -Id 42", Risk::Safe),
                option("LLM risk", "custom-command", Risk::Dangerous),
            ],
        });

        assert_eq!(options.options[0].risk, Risk::Dangerous);
        assert_eq!(options.options[1].risk, Risk::Dangerous);
    }

    #[test]
    fn prefers_safe_options_before_dangerous_options() {
        let options = apply_overrides(CommandOptions {
            options: vec![
                option("Kill process", "Stop-Process -Id 42", Risk::Safe),
                option("Inspect process", "Get-Process -Id 42", Risk::Safe),
            ],
        });

        assert_eq!(options.options[0].title, "Inspect process");
        assert_eq!(options.options[0].risk, Risk::Safe);
        assert_eq!(options.options[1].title, "Kill process");
        assert_eq!(options.options[1].risk, Risk::Dangerous);
    }

    fn option(title: &str, command: &str, risk: Risk) -> CommandOption {
        CommandOption {
            title: title.to_owned(),
            command: command.to_owned(),
            risk,
        }
    }
}
