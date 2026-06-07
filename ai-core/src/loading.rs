use std::{
    io::{self, IsTerminal, Stderr, Write},
    path::PathBuf,
    sync::mpsc,
    thread,
    time::Duration,
};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType},
};

use crate::{
    config::ResolvedConfig,
    llm::{self, LlmError},
    types::CommandOptions,
};

const FRAME_DELAY: Duration = Duration::from_millis(140);
const FRAMES: [&str; 4] = ["thinking   ", "thinking.  ", "thinking.. ", "thinking..."];

pub(crate) enum LoadingResult {
    Options(CommandOptions),
    Cancelled,
}

pub(crate) fn generate_options(
    config: &ResolvedConfig,
    prompt: &str,
    files: &[PathBuf],
) -> Result<LoadingResult, LlmError> {
    if !can_show_loading() {
        return llm::generate_options(config, prompt, files).map(LoadingResult::Options);
    }

    let (tx, rx) = mpsc::channel();
    let config = config.clone();
    let prompt = prompt.to_owned();
    let files = files.to_vec();

    thread::spawn(move || {
        let _ = tx.send(llm::generate_options(&config, &prompt, &files));
    });

    let mut stderr = io::stderr();
    let _raw_mode = RawModeGuard::enter().ok();
    let mut frame = 0;
    let _ = render_loading(&mut stderr, frame);

    loop {
        match rx.recv_timeout(FRAME_DELAY) {
            Ok(result) => {
                let _ = clear_loading(&mut stderr);
                return result.map(LoadingResult::Options);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if is_cancel_pressed() {
                    let _ = clear_loading(&mut stderr);
                    return Ok(LoadingResult::Cancelled);
                }

                frame += 1;
                let _ = render_loading(&mut stderr, frame);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = clear_loading(&mut stderr);
                return Err(LlmError::EmptyMessage);
            }
        }
    }
}

fn can_show_loading() -> bool {
    io::stderr().is_terminal() && io::stdin().is_terminal()
}

fn render_loading(stderr: &mut Stderr, frame: usize) -> io::Result<()> {
    execute!(
        stderr,
        cursor::MoveToColumn(0),
        Clear(ClearType::CurrentLine),
        Print(format!(
            "{}  Press Ctrl+C to cancel",
            FRAMES[frame % FRAMES.len()]
        ))
    )?;
    stderr.flush()
}

fn clear_loading(stderr: &mut Stderr) -> io::Result<()> {
    execute!(
        stderr,
        cursor::MoveToColumn(0),
        Clear(ClearType::CurrentLine)
    )?;
    stderr.flush()
}

fn is_cancel_pressed() -> bool {
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        if let Ok(Event::Key(key)) = event::read()
            && key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            return true;
        }
    }

    false
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
