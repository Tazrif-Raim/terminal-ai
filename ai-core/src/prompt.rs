pub(crate) fn join_parts(parts: &[String]) -> String {
    parts.join(" ").trim().to_owned()
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PromptContext<'a> {
    pub(crate) os: &'a str,
    pub(crate) shell: &'a str,
    pub(crate) max_options: usize,
}

pub(crate) fn current_os() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    }
}

pub(crate) fn system_prompt(context: PromptContext<'_>) -> String {
    format!(
        r#"You are a terminal command assistant.

Return JSON only with this exact shape:
{{"options":[{{"title":"short title","command":"shell command","risk":"safe"}}]}}

Rules:
- Return 1-{max_options} options.
- Target OS: {os}.
- Target shell: {shell}.
- Prefer PowerShell commands on Windows.
- Prefer inspection commands before destructive commands.
- Never invent unknown file paths, container names, branches, PIDs, or process names.
- Mark destructive or risky commands as "dangerous"; otherwise use "safe".
- Keep titles brief."#,
        os = context.os,
        shell = context.shell,
        max_options = context.max_options.clamp(1, 3),
    )
}

pub(crate) fn user_prompt(request: &str, context: PromptContext<'_>) -> String {
    format!(
        "OS: {os}\nShell: {shell}\n\nUser request:\n{request}",
        os = context.os,
        shell = context.shell,
        request = request.trim(),
    )
}

#[cfg(test)]
mod tests {
    use super::{PromptContext, join_parts, system_prompt, user_prompt};

    #[test]
    fn joins_prompt_parts_with_spaces() {
        let parts = vec![
            "what".to_owned(),
            "is".to_owned(),
            "running".to_owned(),
            "on".to_owned(),
            "port".to_owned(),
            "3000".to_owned(),
        ];

        assert_eq!(join_parts(&parts), "what is running on port 3000");
    }

    #[test]
    fn trims_empty_quoted_parts() {
        let parts = vec!["".to_owned(), "  ".to_owned()];

        assert_eq!(join_parts(&parts), "");
    }

    #[test]
    fn system_prompt_includes_required_rules_and_context() {
        let prompt = system_prompt(PromptContext {
            os: "Windows",
            shell: "powershell",
            max_options: 3,
        });

        assert!(prompt.contains("Return JSON only"));
        assert!(prompt.contains("Target OS: Windows"));
        assert!(prompt.contains("Target shell: powershell"));
        assert!(prompt.contains("Prefer PowerShell commands on Windows"));
        assert!(prompt.contains("Never invent unknown file paths"));
        assert!(prompt.contains("dangerous"));
    }

    #[test]
    fn user_prompt_includes_request_and_context() {
        let prompt = user_prompt(
            "what is running on port 3000",
            PromptContext {
                os: "Windows",
                shell: "powershell",
                max_options: 3,
            },
        );

        assert!(prompt.contains("OS: Windows"));
        assert!(prompt.contains("Shell: powershell"));
        assert!(prompt.contains("what is running on port 3000"));
    }
}
