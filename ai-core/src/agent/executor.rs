use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use super::types::{AgentState, StepOutput};

pub(crate) fn run_command(command: &str, state: &mut AgentState) -> StepOutput {
    let started = Instant::now();
    let command = command.trim();

    if let Some(target) = parse_cd_target(command) {
        return change_directory(&target, state, started);
    }

    match shell_command(command)
        .current_dir(&state.cwd)
        .envs(&state.env)
        .output()
    {
        Ok(output) => step_output(
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            output.status.code().unwrap_or(-1),
            output.status.success(),
            started,
        ),
        Err(error) => step_output(String::new(), error.to_string(), -1, false, started),
    }
}

pub(crate) fn read_file(path: &str, state: &AgentState) -> StepOutput {
    let started = Instant::now();
    match fs::read_to_string(resolve_path(path, state)) {
        Ok(content) => step_output(content, String::new(), 0, true, started),
        Err(error) => step_output(String::new(), error.to_string(), 1, false, started),
    }
}

pub(crate) fn write_file(path: &str, content: &str, state: &AgentState) -> StepOutput {
    let started = Instant::now();
    match fs::write(resolve_path(path, state), content) {
        Ok(()) => step_output(String::new(), String::new(), 0, true, started),
        Err(error) => step_output(String::new(), error.to_string(), 1, false, started),
    }
}

fn change_directory(target: &str, state: &mut AgentState, started: Instant) -> StepOutput {
    let path = resolve_path(target, state);
    match fs::canonicalize(&path) {
        Ok(path) if path.is_dir() => {
            state.cwd = path;
            step_output(
                format!("{}\n", state.cwd.display()),
                String::new(),
                0,
                true,
                started,
            )
        }
        Ok(path) => step_output(
            String::new(),
            format!("not a directory: {}", path.display()),
            1,
            false,
            started,
        ),
        Err(error) => step_output(String::new(), error.to_string(), 1, false, started),
    }
}

fn resolve_path(path: &str, state: &AgentState) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        state.cwd.join(path)
    }
}

fn parse_cd_target(command: &str) -> Option<String> {
    let mut parts = command.trim().splitn(2, char::is_whitespace);
    let name = parts.next()?;
    if !name.eq_ignore_ascii_case("cd") {
        return None;
    }

    let target = parts.next()?.trim();
    if target.is_empty() {
        return None;
    }

    Some(unquote(target).to_owned())
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut process = Command::new("powershell");
        process.args(["-NoProfile", "-Command", command]);
        process
    }

    #[cfg(not(windows))]
    {
        let mut process = Command::new("sh");
        process.args(["-c", command]);
        process
    }
}

fn step_output(
    stdout: String,
    stderr: String,
    exit_code: i32,
    success: bool,
    started: Instant,
) -> StepOutput {
    StepOutput {
        stdout,
        stderr,
        exit_code,
        success,
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{read_file, run_command, write_file};
    use crate::agent::types::AgentState;

    #[test]
    fn run_command_captures_stdout() {
        let temp_dir = TempDir::new("stdout");
        let mut state = test_state(temp_dir.path());

        let output = run_command("echo hello", &mut state);

        assert!(output.success);
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("hello"));
    }

    #[test]
    fn run_command_captures_nonzero_exit() {
        let temp_dir = TempDir::new("nonzero");
        let mut state = test_state(temp_dir.path());

        let output = run_command("exit 7", &mut state);

        assert!(!output.success);
        assert_eq!(output.exit_code, 7);
    }

    #[test]
    fn run_command_handles_cd_by_updating_state() {
        let temp_dir = TempDir::new("cd");
        let child = temp_dir.path().join("child");
        fs::create_dir(&child).expect("create child dir");
        let mut state = test_state(temp_dir.path());

        let output = run_command("cd child", &mut state);

        assert!(output.success);
        assert_eq!(state.cwd, fs::canonicalize(child).expect("canonical child"));
    }

    #[test]
    fn read_file_resolves_relative_to_state_cwd() {
        let temp_dir = TempDir::new("read");
        fs::write(temp_dir.path().join("note.txt"), "hello").expect("write test file");
        let state = test_state(temp_dir.path());

        let output = read_file("note.txt", &state);

        assert!(output.success);
        assert_eq!(output.stdout, "hello");
    }

    #[test]
    fn write_file_resolves_relative_to_state_cwd() {
        let temp_dir = TempDir::new("write");
        let state = test_state(temp_dir.path());

        let output = write_file("out.txt", "hello", &state);

        assert!(output.success);
        assert_eq!(
            fs::read_to_string(temp_dir.path().join("out.txt")).expect("read output file"),
            "hello"
        );
    }

    fn test_state(cwd: &Path) -> AgentState {
        AgentState::new(
            "test goal",
            fs::canonicalize(cwd).expect("canonical cwd"),
            HashMap::new(),
        )
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "terminal-ai-executor-{}-{}-{name}",
                std::process::id(),
                unique_id()
            ));
            fs::create_dir(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn unique_id() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    }
}
