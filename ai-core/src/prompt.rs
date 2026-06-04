use crate::context::ShellContext;

pub(crate) fn join_parts(parts: &[String]) -> String {
    parts.join(" ").trim().to_owned()
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PromptContext<'a> {
    pub(crate) os: &'a str,
    pub(crate) shell: &'a str,
    pub(crate) max_options: usize,
    pub(crate) shell_context: Option<&'a ShellContext>,
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
    let mut prompt = format!(
        "OS: {os}\nShell: {shell}\n\nUser request:\n{request}",
        os = context.os,
        shell = context.shell,
        request = request.trim(),
    );

    if let Some(shell_context) = context.shell_context {
        prompt.push_str("\n\n");
        prompt.push_str(&format_shell_context(shell_context));
    }

    prompt
}

fn format_shell_context(context: &ShellContext) -> String {
    let mut lines = vec!["Local context:".to_owned()];

    if let Some(os_version) = &context.os_version {
        lines.push(format!("OS version: {os_version}"));
    }

    if let Some(current_dir) = &context.current_dir {
        lines.push(format!("Current directory: {current_dir}"));
    }

    lines.push(format!(
        "Git repo: {}",
        if context.is_git_repo { "yes" } else { "no" }
    ));

    if let Some(branch) = &context.git_branch {
        lines.push(format!("Git branch: {branch}"));
    }

    if !context.recent_commit_hashes.is_empty() {
        lines.push(format!(
            "Recent commit hashes: {}",
            context.recent_commit_hashes.join(", ")
        ));
    }

    if !context.detected_files.is_empty() {
        lines.push(format!(
            "Detected files: {}",
            context.detected_files.join(", ")
        ));
    }

    if !context.included_files.is_empty() {
        lines.push("Explicit files:".to_owned());

        for file in &context.included_files {
            lines.push(format!("--- file: {} ---", file.path));
            lines.push(file.contents.clone());
            if file.truncated {
                lines.push("[truncated]".to_owned());
            }
            lines.push("--- end file ---".to_owned());
        }
    }

    if !context.recent_commands.is_empty() {
        lines.push("Recent commands:".to_owned());
        lines.extend(
            context
                .recent_commands
                .iter()
                .map(|command| format!("- {command}")),
        );
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{PromptContext, join_parts, system_prompt, user_prompt};
    use crate::context::ShellContext;

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
            shell_context: None,
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
                shell_context: None,
            },
        );

        assert!(prompt.contains("OS: Windows"));
        assert!(prompt.contains("Shell: powershell"));
        assert!(prompt.contains("what is running on port 3000"));
    }

    #[test]
    fn user_prompt_includes_local_context_when_enabled() {
        let context = ShellContext {
            os: "Windows".to_owned(),
            os_version: Some("Windows 11".to_owned()),
            shell_name: "PowerShell".to_owned(),
            shell_version: Some("7.5.0".to_owned()),
            current_dir: Some("E:\\personal\\terminal-ai".to_owned()),
            git_branch: Some("main".to_owned()),
            recent_commit_hashes: vec!["abc1234".to_owned()],
            is_git_repo: true,
            detected_files: vec!["package.json".to_owned(), "Cargo.toml".to_owned()],
            included_files: vec![crate::context::IncludedFile {
                path: "README.md".to_owned(),
                contents: "# terminal-ai".to_owned(),
                truncated: false,
            }],
            recent_commands: vec!["cargo test".to_owned()],
        };
        let prompt = user_prompt(
            "show project structure",
            PromptContext {
                os: "Windows",
                shell: "PowerShell 7.5.0",
                max_options: 3,
                shell_context: Some(&context),
            },
        );

        assert!(prompt.contains("Local context:"));
        assert!(prompt.contains("OS version: Windows 11"));
        assert!(prompt.contains("Current directory: E:\\personal\\terminal-ai"));
        assert!(prompt.contains("Git repo: yes"));
        assert!(prompt.contains("Git branch: main"));
        assert!(prompt.contains("Recent commit hashes: abc1234"));
        assert!(prompt.contains("Detected files: package.json, Cargo.toml"));
        assert!(prompt.contains("--- file: README.md ---"));
        assert!(prompt.contains("# terminal-ai"));
        assert!(prompt.contains("--- end file ---"));
        assert!(prompt.contains("- cargo test"));
    }
}
