use std::io::{self, IsTerminal, Stderr, Write};

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal,
};

use crate::types::Risk;

use super::types::{ActionType, AgentState, AgentStep, BackgroundProcess, StepOutput};

const SEPARATOR: &str = "------------------------------------------";

pub(crate) fn render_step_header(step: &AgentStep, state: &AgentState) {
    let mut stderr = io::stderr();
    let _ = render_step_header_to(&mut stderr, step, state);
}

pub(crate) fn render_output(output: &StepOutput) {
    if !output.stdout.is_empty() {
        print!("{}", output.stdout);
        let _ = io::stdout().flush();
    }

    if !output.stderr.is_empty() {
        eprint!("{}", output.stderr);
    }

    let mut stderr = io::stderr();
    let _ = render_status_to(&mut stderr, output);
}

pub(crate) fn render_done(state: &AgentState) {
    let total_ms: u64 = state
        .history
        .iter()
        .map(|step| step.output.duration_ms)
        .sum();

    let mut stderr = io::stderr();
    let _ = queue!(
        stderr,
        SetForegroundColor(Color::Green),
        Print(format!(
            "Done after {} step(s) in {}\n",
            state.history.len(),
            duration_label(total_ms)
        )),
        ResetColor
    )
    .and_then(|_| stderr.flush());
}

pub(crate) fn render_error(message: &str) {
    let mut stderr = io::stderr();
    let _ = queue!(
        stderr,
        SetForegroundColor(Color::Red),
        Print(format!("error: {message}\n")),
        ResetColor
    )
    .and_then(|_| stderr.flush());
}

pub(crate) fn confirm_dangerous(step: &AgentStep) -> bool {
    let mut stderr = io::stderr();
    let _ = queue!(
        stderr,
        SetForegroundColor(Color::Red),
        SetAttribute(Attribute::Bold),
        Print("Dangerous step\n"),
        SetAttribute(Attribute::Reset),
        ResetColor,
        Print(format!("WHY: {}\n", step.reasoning)),
    );

    if let Some(command) = &step.command {
        let _ = queue!(stderr, Print(format!("CMD: {command}\n")));
    }

    let _ = queue!(
        stderr,
        Print("Press Enter to continue | q/Esc/Ctrl+C = abort")
    );
    let _ = stderr.flush();

    if io::stdin().is_terminal() && io::stderr().is_terminal() {
        return read_dangerous_confirmation().unwrap_or(false);
    }

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }

    !input.trim().eq_ignore_ascii_case("q")
}

pub(crate) fn ask_user(question: &str) -> String {
    eprintln!("{question}");
    eprint!("> ");
    let _ = io::stderr().flush();

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => input.trim_end().to_owned(),
        Err(_) => String::new(),
    }
}

fn render_step_header_to(
    stderr: &mut Stderr,
    step: &AgentStep,
    state: &AgentState,
) -> io::Result<()> {
    queue!(stderr, Print(format!("\n{SEPARATOR}\n")))?;
    queue!(
        stderr,
        SetAttribute(Attribute::Bold),
        Print(format!(
            " Step {} / ~{}  |  {:?}  |  ",
            step.step, step.total_estimated, step.action_type
        )),
        SetAttribute(Attribute::Reset)
    )?;
    render_risk_label(stderr, step.risk)?;
    queue!(stderr, Print(format!("\n{SEPARATOR}\n")))?;
    queue!(stderr, Print(format!(" CWD : {}\n", state.cwd.display())))?;

    match step.action_type {
        ActionType::RunCommand => {
            if let Some(command) = &step.command {
                queue!(stderr, Print(format!(" CMD : {command}\n")))?;
            }
        }
        ActionType::ReadFile | ActionType::WriteFile => {
            if let Some(path) = &step.file_path {
                queue!(stderr, Print(format!(" FILE: {path}\n")))?;
            }
        }
        ActionType::AskUser | ActionType::Done => {}
    }

    queue!(
        stderr,
        Print(format!(" WHY : {}\n{SEPARATOR}\n", step.reasoning))
    )?;
    stderr.flush()
}

fn render_status_to(stderr: &mut Stderr, output: &StepOutput) -> io::Result<()> {
    let color = if output.success {
        Color::Green
    } else {
        Color::Red
    };
    let label = if output.success { "ok" } else { "failed" };

    queue!(
        stderr,
        SetForegroundColor(color),
        Print(format!(
            "{label}: exit {} in {}\n",
            output.exit_code,
            duration_label(output.duration_ms)
        )),
        ResetColor
    )?;
    stderr.flush()
}

fn render_risk_label(stderr: &mut Stderr, risk: Risk) -> io::Result<()> {
    let color = match risk {
        Risk::Safe => Color::Green,
        Risk::Dangerous => Color::Red,
    };

    queue!(
        stderr,
        SetForegroundColor(color),
        Print(format!("[{}]", risk_label(risk))),
        ResetColor
    )
}

fn read_dangerous_confirmation() -> io::Result<bool> {
    let _guard = RawModeGuard::enter()?;

    loop {
        if let Event::Key(key) = event::read()? {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                eprintln!();
                return Ok(false);
            }

            match key.code {
                KeyCode::Enter => {
                    eprintln!();
                    return Ok(true);
                }
                KeyCode::Char(value) if value.eq_ignore_ascii_case(&'q') => {
                    eprintln!();
                    return Ok(false);
                }
                KeyCode::Esc => {
                    eprintln!();
                    return Ok(false);
                }
                _ => {}
            }
        }
    }
}

fn risk_label(risk: Risk) -> &'static str {
    match risk {
        Risk::Safe => "Safe",
        Risk::Dangerous => "Dangerous",
    }
}

fn duration_label(duration_ms: u64) -> String {
    if duration_ms < 1_000 {
        format!("{duration_ms}ms")
    } else {
        format!("{:.1}s", duration_ms as f64 / 1_000.0)
    }
}

pub(crate) fn ask_keep_background_processes(processes: &[BackgroundProcess]) -> bool {
    let mut stderr = io::stderr();

    let _ = queue!(
        stderr,
        Print(format!("\n{SEPARATOR}\n")),
        SetForegroundColor(Color::Yellow),
        Print(" Agent complete. Background processes still running:\n"),
        ResetColor,
        Print(format!("{SEPARATOR}\n"))
    );

    for proc in processes {
        let elapsed = proc.started_at.elapsed();
        let elapsed_str = duration_label(elapsed.as_millis() as u64);
        let pid_str = proc
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".to_string());
        let _ = queue!(
            stderr,
            Print(format!(
                "   • {:<20} (PID {}, running {})\n     cmd: {}\n",
                proc.label, pid_str, elapsed_str, proc.command
            ))
        );
    }

    let _ = queue!(
        stderr,
        Print(format!("{SEPARATOR}\n")),
        Print(" Keep them running after agent exits? (y/n) ")
    );
    let _ = stderr.flush();

    if io::stdin().is_terminal() && io::stderr().is_terminal() {
        return read_keep_confirmation().unwrap_or(true);
    }

    // Non-interactive: default to keeping them running
    eprintln!("y");
    true
}

fn read_keep_confirmation() -> io::Result<bool> {
    let _guard = RawModeGuard::enter()?;

    loop {
        if let Event::Key(key) = event::read()? {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                eprintln!();
                return Ok(false);
            }

            match key.code {
                KeyCode::Char(value) if value.eq_ignore_ascii_case(&'y') => {
                    eprintln!();
                    return Ok(true);
                }
                KeyCode::Char(value) if value.eq_ignore_ascii_case(&'n') => {
                    eprintln!();
                    return Ok(false);
                }
                KeyCode::Char(value) if value.eq_ignore_ascii_case(&'q') => {
                    eprintln!();
                    return Ok(false);
                }
                KeyCode::Esc => {
                    eprintln!();
                    return Ok(false);
                }
                _ => {}
            }
        }
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::{duration_label, risk_label};
    use crate::types::Risk;

    #[test]
    fn formats_duration_labels() {
        assert_eq!(duration_label(42), "42ms");
        assert_eq!(duration_label(1_250), "1.2s");
    }

    #[test]
    fn formats_risk_labels_for_agent_ui() {
        assert_eq!(risk_label(Risk::Safe), "Safe");
        assert_eq!(risk_label(Risk::Dangerous), "Dangerous");
    }
}
