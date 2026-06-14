use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::Instant,
    sync::{Arc, Mutex},
};

use crate::{config::ResolvedConfig, types::Risk};

use super::{
    executor, llm, logs,
    types::{ActionType, AgentRunOptions, AgentState, AgentStep, CompletedStep, StepOutput},
    ui,
};

const BLOCKED_COMMANDS: [&str; 6] = [
    "rm -rf /",
    "rm -fr /",
    "format c:",
    "format.com c:",
    "del /f /s /q c:\\",
    "del /s /q c:\\",
];
const MAX_AGENT_STEPS: usize = 50;

pub(crate) fn run(goal: &str, config: &ResolvedConfig, options: AgentRunOptions) -> ExitCode {
    let started = Instant::now();
    let mut state = match initial_state(goal) {
        Ok(state) => state,
        Err(error) => {
            ui::render_error(&format!("could not initialize agent state: {error}"));
            return ExitCode::from(1);
        }
    };

    // Track background process PIDs for Ctrl+C handler
    let background_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let pids_for_handler = Arc::clone(&background_pids);

    ctrlc::set_handler(move || {
        kill_pids(&pids_for_handler.lock().unwrap());
        std::process::exit(130);
    }).expect("Failed to set Ctrl+C handler");

    if options.dry_run {
        eprintln!("agent mode starting (dry run)");
    } else {
        eprintln!("agent mode starting");
    }

    loop {
        let step = match llm::next_step(config, &state) {
            Ok(step) => step,
            Err(error) => {
                ui::render_error(&error.to_string());
                return finish(&mut state, 1, started);
            }
        };

        state.total_estimated = step.total_estimated;
        ui::render_step_header(&step, &state);

        if step.action_type == ActionType::Done {
            ui::render_done(&state);
            return finish(&mut state, 0, started);
        }

        let output = if let Some(message) = blocked_step_message(&step) {
            blocked_output(message)
        } else if options.dry_run {
            dry_run_output(&step)
        } else if step.risk == Risk::Dangerous
            && config.dangerous_requires_confirm
            && !ui::confirm_dangerous(&step)
        {
            return finish(&mut state, 130, started);
        } else {
            execute_step(&step, &mut state, &background_pids)
        };

        if step.action_type == ActionType::AskUser {
            eprintln!("answer recorded");
        } else {
            ui::render_output(&output);
        }

        if output.success {
            state.consecutive_failures = 0;
        } else {
            state.consecutive_failures += 1;
        }

        state.history.push(CompletedStep { step, output });
        state.step_number += 1;

        if state.history.len() >= MAX_AGENT_STEPS {
            ui::render_error("agent reached the maximum step limit");
            return finish(&mut state, 1, started);
        }

        if state.consecutive_failures >= 3
            && !ui::ask_user("The last 3 steps failed. Continue anyway? (y/n)")
                .eq_ignore_ascii_case("y")
        {
            return finish(&mut state, 1, started);
        }
    }
}

fn initial_state(goal: &str) -> std::io::Result<AgentState> {
    let current_dir = env::current_dir()?;

    Ok(AgentState::new(goal, current_dir, env::vars().collect()))
}

fn execute_step(step: &AgentStep, state: &mut AgentState, background_pids: &Arc<Mutex<Vec<u32>>>) -> StepOutput {
    let previous_cwd = match apply_cwd_override(step, state) {
        Ok(previous_cwd) => previous_cwd,
        Err(output) => return output,
    };

    let output = match step.action_type {
        ActionType::RunCommand => {
            let command = step.command.as_deref().unwrap_or_default();
            if step.background {
                let output = executor::spawn_background(command, state);
                // Track PID for Ctrl+C handler
                if output.success {
                    if let Some(pid) = state.background_processes.last().and_then(|p| p.pid) {
                        background_pids.lock().unwrap().push(pid);
                    }
                }
                output
            } else {
                executor::run_command(command, state)
            }
        }
        ActionType::ReadFile => {
            let path = step.file_path.as_deref().unwrap_or_default();
            executor::read_file(path, state)
        }
        ActionType::WriteFile => {
            let path = step.file_path.as_deref().unwrap_or_default();
            let content = step.file_content.as_deref().unwrap_or_default();
            executor::write_file(path, content, state)
        }
        ActionType::AskUser => StepOutput {
            stdout: ui::ask_user(&step.reasoning),
            stderr: String::new(),
            exit_code: 0,
            success: true,
            duration_ms: 0,
        },
        ActionType::Done => StepOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            success: true,
            duration_ms: 0,
        },
    };

    if let Some(previous_cwd) = previous_cwd {
        state.cwd = previous_cwd;
    }

    output
}

fn apply_cwd_override(
    step: &AgentStep,
    state: &mut AgentState,
) -> Result<Option<PathBuf>, StepOutput> {
    let Some(cwd_override) = step.cwd_override.as_deref() else {
        return Ok(None);
    };

    let path = resolve_path(cwd_override, state);
    match fs::canonicalize(&path) {
        Ok(path) if path.is_dir() && cwd_override_allowed(&path, state) => {
            Ok(Some(std::mem::replace(&mut state.cwd, path)))
        }
        Ok(path) if path.is_dir() => Err(error_output(format!(
            "cwd_override escapes project root: {}",
            path.display()
        ))),
        Ok(path) => Err(error_output(format!("not a directory: {}", path.display()))),
        Err(error) => Err(error_output(error.to_string())),
    }
}

fn cwd_override_allowed(path: &Path, state: &AgentState) -> bool {
    state.project_root.parent().is_none() || path.starts_with(&state.project_root) || {
        // Handle Windows \\?\ prefix by comparing canonical versions
        if let (Ok(canon_path), Ok(canon_root)) = (fs::canonicalize(path), fs::canonicalize(&state.project_root)) {
            canon_path.starts_with(&canon_root)
        } else {
            false
        }
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

fn error_output(message: String) -> StepOutput {
    StepOutput {
        stdout: String::new(),
        stderr: message,
        exit_code: 1,
        success: false,
        duration_ms: 0,
    }
}

fn blocked_step_message(step: &AgentStep) -> Option<String> {
    let command = step.command.as_deref()?;
    is_blocked_command(command).then(|| format!("blocked command: {command}"))
}

fn is_blocked_command(command: &str) -> bool {
    let normalized = command.trim().to_ascii_lowercase().replace('/', "\\");
    BLOCKED_COMMANDS.iter().any(|blocked| {
        let blocked = blocked.to_ascii_lowercase().replace('/', "\\");
        normalized == blocked || normalized.starts_with(&format!("{blocked} "))
    })
}

fn blocked_output(message: String) -> StepOutput {
    StepOutput {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit_code: 126,
        success: false,
        duration_ms: 0,
    }
}

fn dry_run_output(step: &AgentStep) -> StepOutput {
    StepOutput {
        stdout: format!(
            "dry run: assumed {:?} completed successfully without executing it\n",
            step.action_type
        ),
        stderr: String::new(),
        exit_code: 0,
        success: true,
        duration_ms: 0,
    }
}

fn finish(state: &mut AgentState, exit_code: u8, started: Instant) -> ExitCode {
    // Handle background processes on clean exit (only for Done, not for errors/Ctrl+C)
    if exit_code == 0 {
        handle_background_processes_on_exit(state);
    }
    
    let total_duration_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    if let Err(error) = logs::write(state, i32::from(exit_code), total_duration_ms) {
        ui::render_error(&format!("could not write agent audit log: {error}"));
    }

    ExitCode::from(exit_code)
}

fn handle_background_processes_on_exit(state: &mut AgentState) {
    if state.background_processes.is_empty() {
        return;
    }

    let keep = ui::ask_keep_background_processes(&state.background_processes);

    if keep {
        // Detach: on Windows, CREATE_NEW_PROCESS_GROUP was set at spawn time,
        // so just don't kill. On Unix, don't call kill/wait.
        // Nothing to do - just let them live.
    } else {
        // Kill each process
        for proc in &mut state.background_processes {
            let _ = proc.child.kill();
            let _ = proc.child.wait(); // reap to avoid zombies on unix
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::{Path, PathBuf},
    };

    use super::{cwd_override_allowed, is_blocked_command};
    use crate::agent::types::AgentState;

    #[test]
    fn blocks_hardcoded_destructive_commands() {
        assert!(is_blocked_command("rm -rf /"));
        assert!(is_blocked_command("format C:"));
        assert!(is_blocked_command("del /f /s /q C:\\"));
        assert!(!is_blocked_command("Get-ChildItem"));
    }

    #[test]
    fn rejects_cwd_override_outside_project_root() {
        let root = temp_path("root");
        let outside = temp_path("outside");
        fs::create_dir_all(&root).expect("create root");
        fs::create_dir_all(&outside).expect("create outside");
        let state = test_state(&root);

        assert!(cwd_override_allowed(&root, &state));
        assert!(!cwd_override_allowed(&outside, &state));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn test_cwd_override_allowed_with_windows_paths() {
        // Create a mock state with a project root
        let root = temp_path("root");
        fs::create_dir_all(&root).expect("create root");
        let state = test_state(&root);

        // Test that the project root itself is allowed
        assert!(cwd_override_allowed(&root, &state));

        // Test that a subdirectory is allowed
        let subdir = root.join("test_folder");
        fs::create_dir_all(&subdir).expect("create subdir");
        assert!(cwd_override_allowed(&subdir, &state));

        // Test that a parent directory is NOT allowed (unless project root has no parent)
        if let Some(parent) = root.parent() {
            assert!(!cwd_override_allowed(parent, &state));
        }

        let _ = fs::remove_dir_all(root);
    }

    fn test_state(root: &Path) -> AgentState {
        AgentState::new("test", root.to_path_buf(), HashMap::new())
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "terminal-ai-loop-test-{}-{name}",
            std::process::id()
        ))
    }
}

fn kill_pids(pids: &[u32]) {
    #[cfg(windows)]
    {
        for pid in pids {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
        }
    }

    #[cfg(not(windows))]
    {
        for pid in pids {
            let _ = Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output();
        }
    }
}
