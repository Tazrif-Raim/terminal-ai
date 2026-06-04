use std::{
    fmt,
    io::{self, IsTerminal, Stderr, Write},
};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::{Attribute, Print, SetAttribute},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::types::{CommandOptions, PickerResult};

#[derive(Debug)]
pub(crate) enum PickerError {
    Io(io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerAction {
    Run,
    Edit,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PickerState {
    selected: usize,
    len: usize,
}

impl PickerState {
    fn new(len: usize) -> Self {
        Self { selected: 0, len }
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn previous(&mut self) {
        if self.len <= 1 {
            return;
        }

        self.selected = if self.selected == 0 {
            self.len - 1
        } else {
            self.selected - 1
        };
    }

    fn next(&mut self) {
        if self.len <= 1 {
            return;
        }

        self.selected = (self.selected + 1) % self.len;
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<PickerAction> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(PickerAction::Cancel);
        }

        match key.code {
            KeyCode::Up => {
                self.previous();
                None
            }
            KeyCode::Down => {
                self.next();
                None
            }
            KeyCode::Char(value) if value.eq_ignore_ascii_case(&'a') => {
                self.previous();
                None
            }
            KeyCode::Char(value) if value.eq_ignore_ascii_case(&'s') => {
                self.next();
                None
            }
            KeyCode::Enter => Some(PickerAction::Run),
            KeyCode::Char(value) if value.eq_ignore_ascii_case(&'e') => Some(PickerAction::Edit),
            KeyCode::Char(value) if value.eq_ignore_ascii_case(&'q') => Some(PickerAction::Cancel),
            KeyCode::Esc => Some(PickerAction::Cancel),
            _ => None,
        }
    }

    fn result(&self, options: &CommandOptions, action: PickerAction) -> PickerResult {
        match action {
            PickerAction::Run => PickerResult::run(options.options[self.selected].command.clone()),
            PickerAction::Edit => {
                PickerResult::edit(options.options[self.selected].command.clone())
            }
            PickerAction::Cancel => PickerResult::cancel(),
        }
    }
}

impl fmt::Display for PickerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(source) => write!(formatter, "error: picker failed: {source}"),
        }
    }
}

impl std::error::Error for PickerError {}

impl From<io::Error> for PickerError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) fn pick(options: &CommandOptions) -> Result<PickerResult, PickerError> {
    if !io::stderr().is_terminal() {
        return Ok(PickerResult::cancel());
    }

    let mut stderr = io::stderr();
    let _guard = TerminalGuard::enter(&mut stderr)?;
    let mut state = PickerState::new(options.options.len());

    render(&mut stderr, options, state.selected())?;

    loop {
        if let Event::Key(key) = event::read()? {
            if let Some(action) = state.handle_key(key) {
                return Ok(state.result(options, action));
            }

            render(&mut stderr, options, state.selected())?;
        }
    }
}

fn render(
    stderr: &mut Stderr,
    options: &CommandOptions,
    selected: usize,
) -> Result<(), PickerError> {
    queue!(stderr, cursor::MoveTo(0, 0), Clear(ClearType::All))?;
    queue!(stderr, Print("Select command\r\n\r\n"))?;

    for (index, option) in options.options.iter().enumerate() {
        if index == selected {
            queue!(stderr, SetAttribute(Attribute::Reverse))?;
            queue!(
                stderr,
                Print(format!("> {} [{}]\r\n", option.title, option.risk))
            )?;
            queue!(stderr, SetAttribute(Attribute::Reset))?;
        } else {
            queue!(
                stderr,
                Print(format!("  {} [{}]\r\n", option.title, option.risk))
            )?;
        }

        queue!(stderr, Print(format!("  {}\r\n\r\n", option.command)))?;
    }

    queue!(
        stderr,
        Print("Up/Down or a/s = select | Enter = run | e = edit | q = cancel")
    )?;
    stderr.flush()?;

    Ok(())
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter(stderr: &mut Stderr) -> Result<Self, PickerError> {
        terminal::enable_raw_mode()?;

        if let Err(error) = execute!(stderr, EnterAlternateScreen, cursor::Hide) {
            let _ = terminal::disable_raw_mode();
            return Err(PickerError::Io(error));
        }

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stderr = io::stderr();
        let _ = execute!(
            stderr,
            SetAttribute(Attribute::Reset),
            Clear(ClearType::All),
            cursor::Show,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::{PickerAction, PickerState};
    use crate::types::{CommandOption, CommandOptions, PickerResult, Risk};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn moves_selection_with_arrows_and_as_keys() {
        let mut state = PickerState::new(3);

        state.handle_key(key(KeyCode::Down));
        assert_eq!(state.selected(), 1);

        state.handle_key(key(KeyCode::Char('s')));
        assert_eq!(state.selected(), 2);

        state.handle_key(key(KeyCode::Down));
        assert_eq!(state.selected(), 0);

        state.handle_key(key(KeyCode::Up));
        assert_eq!(state.selected(), 2);

        state.handle_key(key(KeyCode::Char('a')));
        assert_eq!(state.selected(), 1);
    }

    #[test]
    fn maps_action_keys() {
        let mut state = PickerState::new(1);

        assert_eq!(
            state.handle_key(key(KeyCode::Enter)),
            Some(PickerAction::Run)
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('e'))),
            Some(PickerAction::Edit)
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('q'))),
            Some(PickerAction::Cancel)
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Esc)),
            Some(PickerAction::Cancel)
        );
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(PickerAction::Cancel)
        );
    }

    #[test]
    fn builds_result_from_selected_option() {
        let mut state = PickerState::new(2);
        state.handle_key(key(KeyCode::Down));

        assert_eq!(
            state.result(&options(), PickerAction::Run),
            PickerResult::run("Get-Service")
        );
        assert_eq!(
            state.result(&options(), PickerAction::Edit),
            PickerResult::edit("Get-Service")
        );
        assert_eq!(
            state.result(&options(), PickerAction::Cancel),
            PickerResult::cancel()
        );
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn options() -> CommandOptions {
        CommandOptions {
            options: vec![
                option("Processes", "Get-Process"),
                option("Services", "Get-Service"),
            ],
        }
    }

    fn option(title: &str, command: &str) -> CommandOption {
        CommandOption {
            title: title.to_owned(),
            command: command.to_owned(),
            risk: Risk::Safe,
        }
    }
}
