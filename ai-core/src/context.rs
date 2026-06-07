use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::config::ResolvedConfig;

const ENV_SHELL_NAME: &str = "TERMINAL_AI_SHELL_NAME";
const ENV_SHELL_VERSION: &str = "TERMINAL_AI_SHELL_VERSION";
const ENV_OS_VERSION: &str = "TERMINAL_AI_OS_VERSION";
const ENV_RECENT_COMMANDS: &str = "TERMINAL_AI_RECENT_COMMANDS";
const MAX_INCLUDED_FILES: usize = 6;
const MAX_INCLUDED_FILE_BYTES: usize = 4 * 1024;
const MAX_TOTAL_FILE_BYTES: usize = 20 * 1024;

const DETECTED_FILES: &[&str] = &[
    "package.json",
    "pnpm-lock.yaml",
    "package-lock.json",
    "yarn.lock",
    "Cargo.toml",
    "go.mod",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    ".env.example",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellContext {
    pub(crate) os: String,
    pub(crate) os_version: Option<String>,
    pub(crate) shell_name: String,
    pub(crate) shell_version: Option<String>,
    pub(crate) current_dir: Option<String>,
    pub(crate) git_branch: Option<String>,
    pub(crate) recent_commit_hashes: Vec<String>,
    pub(crate) is_git_repo: bool,
    pub(crate) detected_files: Vec<String>,
    pub(crate) included_files: Vec<IncludedFile>,
    pub(crate) recent_commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncludedFile {
    pub(crate) path: String,
    pub(crate) contents: String,
    pub(crate) truncated: bool,
}

impl ShellContext {
    pub(crate) fn shell_label(&self) -> String {
        match self.shell_version.as_deref() {
            Some(version) => format!("{} {}", self.shell_name, version),
            None => self.shell_name.clone(),
        }
    }
}

pub(crate) fn collect(config: &ResolvedConfig, files: &[PathBuf]) -> ShellContext {
    let current_dir = env::current_dir().ok();
    let include_context = config.send_context;

    ShellContext {
        os: current_os().to_owned(),
        os_version: env_value(ENV_OS_VERSION),
        shell_name: env_value(ENV_SHELL_NAME).unwrap_or_else(|| config.default_shell.clone()),
        shell_version: env_value(ENV_SHELL_VERSION),
        current_dir: include_context
            .then(|| {
                current_dir
                    .as_ref()
                    .map(|path| display_path(path.as_path()))
            })
            .flatten(),
        git_branch: include_context
            .then(|| current_dir.as_ref().and_then(|dir| git_branch(dir)))
            .flatten(),
        recent_commit_hashes: if include_context {
            current_dir
                .as_ref()
                .map_or_else(Vec::new, |dir| recent_commit_hashes(dir.as_path(), 5))
        } else {
            Vec::new()
        },
        is_git_repo: include_context
            && current_dir
                .as_ref()
                .is_some_and(|dir| is_git_repo(dir.as_path())),
        detected_files: if include_context {
            current_dir
                .as_ref()
                .map_or_else(Vec::new, |dir| detected_files(dir.as_path()))
        } else {
            Vec::new()
        },
        included_files: if include_context {
            current_dir
                .as_ref()
                .map_or_else(Vec::new, |dir| included_files(dir.as_path(), files))
        } else {
            Vec::new()
        },
        recent_commands: if include_context && config.send_recent_commands {
            recent_commands(config.max_recent_commands)
        } else {
            Vec::new()
        },
    }
}

fn current_os() -> &'static str {
    match env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    }
}

fn env_value(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn is_git_repo(dir: &Path) -> bool {
    git_output(dir, ["rev-parse", "--is-inside-work-tree"])
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn git_branch(dir: &Path) -> Option<String> {
    git_output(dir, ["rev-parse", "--abbrev-ref", "HEAD"]).filter(|branch| branch != "HEAD")
}

fn recent_commit_hashes(dir: &Path, max_count: usize) -> Vec<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("log")
        .arg("-n")
        .arg(max_count.to_string())
        .arg("--format=%h")
        .output();

    output_lines(output)
}

fn git_output<const N: usize>(dir: &Path, args: [&str; N]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn output_lines(output: std::io::Result<std::process::Output>) -> Vec<String> {
    let Ok(output) = output else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8(output.stdout)
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn detected_files(dir: &Path) -> Vec<String> {
    DETECTED_FILES
        .iter()
        .filter(|name| file_exists(dir, name))
        .map(|name| (*name).to_owned())
        .collect()
}

fn file_exists(dir: &Path, name: &str) -> bool {
    fs::metadata(dir.join(name)).is_ok()
}

fn included_files(base_dir: &Path, files: &[PathBuf]) -> Vec<IncludedFile> {
    let mut included = Vec::new();
    let mut total_bytes = 0;

    for file in files.iter().take(MAX_INCLUDED_FILES) {
        if total_bytes >= MAX_TOTAL_FILE_BYTES || is_sensitive_file(file) {
            continue;
        }

        let path = resolve_input_file(base_dir, file);
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };

        if !metadata.is_file() {
            continue;
        }

        let Ok(bytes) = fs::read(&path) else {
            continue;
        };

        if bytes.contains(&0) {
            continue;
        }

        let available = MAX_TOTAL_FILE_BYTES.saturating_sub(total_bytes);
        let limit = MAX_INCLUDED_FILE_BYTES.min(available);
        if limit == 0 {
            break;
        }

        let truncated = bytes.len() > limit;
        let contents = String::from_utf8_lossy(&bytes[..bytes.len().min(limit)]).to_string();
        total_bytes += contents.len();

        included.push(IncludedFile {
            path: display_included_path(base_dir, &path),
            contents,
            truncated,
        });
    }

    included
}

fn resolve_input_file(base_dir: &Path, file: &Path) -> PathBuf {
    if file.is_absolute() {
        file.to_owned()
    } else {
        base_dir.join(file)
    }
}

fn display_included_path(base_dir: &Path, path: &Path) -> String {
    path.strip_prefix(base_dir)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn is_sensitive_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    let lower = name.to_ascii_lowercase();
    lower == ".env"
        || lower.starts_with(".env.")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".pfx")
        || lower.ends_with(".p12")
}

fn recent_commands(max_commands: usize) -> Vec<String> {
    let Some(raw) = env_value(ENV_RECENT_COMMANDS) else {
        return Vec::new();
    };

    let mut commands = raw
        .lines()
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .filter(|command| !is_recent_command_to_skip(command))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let max_commands = max_commands.min(20);
    if commands.len() > max_commands {
        commands = commands[commands.len() - max_commands..].to_vec();
    }

    commands
}

fn is_recent_command_to_skip(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    let upper = command.to_ascii_uppercase();

    lower.starts_with("ai ")
        || lower == "ai"
        || lower.contains(".env")
        || upper.contains("API_KEY")
        || upper.contains("TOKEN")
        || upper.contains("PASSWORD")
        || upper.contains("SECRET")
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_INCLUDED_FILE_BYTES, ShellContext, detected_files, included_files,
        is_recent_command_to_skip,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn shell_label_includes_version_when_available() {
        let context = ShellContext {
            os: "Windows".to_owned(),
            os_version: None,
            shell_name: "PowerShell".to_owned(),
            shell_version: Some("7.5.0".to_owned()),
            current_dir: None,
            git_branch: None,
            recent_commit_hashes: vec![],
            is_git_repo: false,
            detected_files: vec![],
            included_files: vec![],
            recent_commands: vec![],
        };

        assert_eq!(context.shell_label(), "PowerShell 7.5.0");
    }

    #[test]
    fn detects_only_expected_project_hint_files() {
        let dir =
            std::env::temp_dir().join(format!("terminal-ai-context-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("package.json"), "{}").expect("write package json");
        fs::write(dir.join(".env"), "SECRET=value").expect("write env");
        fs::write(dir.join(".env.example"), "LLM_API_KEY=").expect("write env example");

        let files = detected_files(&dir);

        assert!(files.contains(&"package.json".to_owned()));
        assert!(files.contains(&".env.example".to_owned()));
        assert!(!files.contains(&".env".to_owned()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_recent_commands_that_may_leak_secrets() {
        assert!(is_recent_command_to_skip("ai what is running"));
        assert!(is_recent_command_to_skip("notepad .env"));
        assert!(is_recent_command_to_skip("$env:LLM_API_KEY = 'secret'"));
        assert!(is_recent_command_to_skip("set TOKEN=value"));
        assert!(!is_recent_command_to_skip("cargo test"));
    }

    #[test]
    fn skips_sensitive_explicit_files_and_caps_contents() {
        let dir = std::env::temp_dir().join(format!(
            "terminal-ai-context-files-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("README.md"), "hello").expect("write readme");
        fs::write(dir.join(".env"), "SECRET=value").expect("write env");
        fs::write(
            dir.join("large.txt"),
            "x".repeat(MAX_INCLUDED_FILE_BYTES + 10),
        )
        .expect("write large file");

        let files = included_files(
            &dir,
            &[
                PathBuf::from("README.md"),
                PathBuf::from(".env"),
                PathBuf::from("large.txt"),
            ],
        );

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "README.md");
        assert_eq!(files[0].contents, "hello");
        assert_eq!(files[1].path, "large.txt");
        assert!(files[1].truncated);
        assert_eq!(files[1].contents.len(), MAX_INCLUDED_FILE_BYTES);

        let _ = fs::remove_dir_all(&dir);
    }
}
