use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
};

use directories::BaseDirs;
use serde::Serialize;

use super::types::{AgentState, CompletedStep};

const LOG_LIMIT: usize = 10;
const ENV_AGENT_LOG_DIR: &str = "TERMINAL_AI_AGENT_LOG_DIR";

#[derive(Debug, Serialize)]
struct AuditLog<'a> {
    goal: &'a str,
    cwd: String,
    exit_code: i32,
    total_duration_ms: u64,
    steps: &'a [CompletedStep],
}

pub(crate) fn write(
    state: &AgentState,
    exit_code: i32,
    total_duration_ms: u64,
) -> io::Result<PathBuf> {
    let dir = log_dir()?;
    fs::create_dir_all(&dir)?;

    let path = dir.join(format!("{}.json", timestamp_ms()));
    let log = AuditLog {
        goal: &state.goal,
        cwd: state.cwd.display().to_string(),
        exit_code,
        total_duration_ms,
        steps: &state.history,
    };
    let content = serde_json::to_string_pretty(&log).expect("audit log serializes");
    fs::write(&path, format!("{content}\n"))?;

    Ok(path)
}

pub(crate) fn show_recent(open_latest: bool) -> ExitCode {
    let logs = match recent_logs(LOG_LIMIT) {
        Ok(logs) => logs,
        Err(error) => {
            eprintln!("error: could not read agent logs: {error}");
            return ExitCode::from(1);
        }
    };

    if logs.is_empty() {
        println!("No agent logs found.");
        return ExitCode::SUCCESS;
    }

    println!("Recent agent logs:");
    for (index, path) in logs.iter().enumerate() {
        println!("{}. {}", index + 1, path.display());
    }

    if open_latest {
        match open_path(&logs[0]) {
            Ok(()) => println!("Opened {}", logs[0].display()),
            Err(error) => {
                eprintln!("error: could not open {}: {error}", logs[0].display());
                return ExitCode::from(1);
            }
        }
    }

    ExitCode::SUCCESS
}

fn recent_logs(limit: usize) -> io::Result<Vec<PathBuf>> {
    let dir = log_dir()?;
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut logs = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter_map(|path| {
            let modified = fs::metadata(&path).ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect::<Vec<_>>();

    logs.sort_by(|left, right| right.0.cmp(&left.0));
    logs.truncate(limit);

    Ok(logs.into_iter().map(|(_, path)| path).collect())
}

fn log_dir() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os(ENV_AGENT_LOG_DIR) {
        return Ok(PathBuf::from(path));
    }

    BaseDirs::new()
        .map(|dirs| dirs.data_dir().join("terminal-ai").join("agent-logs"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "data directory unavailable"))
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis()
}

fn open_path(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        Command::new("notepad").arg(path).spawn()?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn()?;
        Ok(())
    }
}
