use std::{
    fmt,
    io::{self, IsTerminal, Stderr, Write},
    time::{Duration, Instant},
};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::types::{CommandOptions, PickerResult, Risk};

const NAVIGATION_DEBOUNCE: Duration = Duration::from_millis(90);

#[derive(Debug)]
pub(crate) enum PickerError {
    Io(io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerAction {
    Run,
    Edit,
    Copy,
    Regenerate,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmDecision {
    Run,
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
            KeyCode::Char(value) if value.eq_ignore_ascii_case(&'w') => {
                self.previous();
                None
            }
            KeyCode::Char(value) if value.eq_ignore_ascii_case(&'s') => {
                self.next();
                None
            }
            KeyCode::Enter => Some(PickerAction::Run),
            KeyCode::Char(value) if value.eq_ignore_ascii_case(&'e') => Some(PickerAction::Edit),
            KeyCode::Char(value) if value.eq_ignore_ascii_case(&'c') => Some(PickerAction::Copy),
            KeyCode::Char(value) if value.eq_ignore_ascii_case(&'r') => {
                Some(PickerAction::Regenerate)
            }
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
            PickerAction::Copy => {
                PickerResult::copy(options.options[self.selected].command.clone())
            }
            PickerAction::Regenerate => PickerResult::regenerate(),
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

pub(crate) fn pick(
    options: &CommandOptions,
    dangerous_requires_confirm: bool,
    hide_descriptions: bool,
) -> Result<PickerResult, PickerError> {
    if !io::stderr().is_terminal() {
        return Ok(PickerResult::cancel());
    }

    let mut stderr = io::stderr();
    let _guard = TerminalGuard::enter(&mut stderr)?;
    let mut state = PickerState::new(options.options.len());
    let mut last_navigation = None;

    drain_pending_events()?;
    render(&mut stderr, options, &state, hide_descriptions)?;

    loop {
        if let Event::Key(key) = event::read()? {
            if should_ignore_navigation(&key, &mut last_navigation) {
                continue;
            }

            if let Some(action) = state.handle_key(key) {
                if requires_dangerous_confirmation(
                    options,
                    state.selected(),
                    action,
                    dangerous_requires_confirm,
                ) {
                    render_dangerous_confirmation(&mut stderr, options, state.selected())?;
                    drain_pending_events()?;

                    return match read_dangerous_confirmation()? {
                        ConfirmDecision::Run => Ok(state.result(options, action)),
                        ConfirmDecision::Cancel => Ok(PickerResult::cancel()),
                    };
                }

                return Ok(state.result(options, action));
            }

            render(&mut stderr, options, &state, hide_descriptions)?;
        }
    }
}

fn requires_dangerous_confirmation(
    options: &CommandOptions,
    selected: usize,
    action: PickerAction,
    dangerous_requires_confirm: bool,
) -> bool {
    dangerous_requires_confirm
        && action == PickerAction::Run
        && options.options[selected].risk == Risk::Dangerous
}

fn render(
    stderr: &mut Stderr,
    options: &CommandOptions,
    state: &PickerState,
    hide_descriptions: bool,
) -> Result<(), PickerError> {
    queue!(stderr, cursor::MoveTo(0, 0), Clear(ClearType::All))?;
    queue!(stderr, Print("Select command\r\n\r\n"))?;

    for (index, option) in options.options.iter().enumerate() {
        let is_selected = index == state.selected();

        if hide_descriptions {
            render_command_only_option(stderr, &option.command, option.risk, is_selected)?;
        } else if is_selected {
            queue!(stderr, SetAttribute(Attribute::Bold))?;
            queue!(stderr, Print(format!("  {} ", option.title)))?;
            render_risk_label(stderr, option.risk)?;
            queue!(stderr, SetAttribute(Attribute::Reset), Print("\r\n"))?;
            queue!(
                stderr,
                SetForegroundColor(Color::Cyan),
                Print("> "),
                ResetColor
            )?;
            queue!(stderr, SetAttribute(Attribute::Reverse))?;
            queue!(stderr, Print(&option.command))?;
            queue!(stderr, SetAttribute(Attribute::Reset), Print("\r\n\r\n"))?;
        } else {
            queue!(stderr, Print(format!("  {} ", option.title)))?;
            render_risk_label(stderr, option.risk)?;
            queue!(stderr, Print("\r\n"))?;
            queue!(stderr, Print(format!("  {}\r\n\r\n", option.command)))?;
        }
    }

    queue!(
        stderr,
        Print(
            "Up/Down or w/s = select | Enter = run | e = edit | c = copy | r = regenerate | q/Esc = cancel"
        )
    )?;

    stderr.flush()?;

    Ok(())
}

fn render_command_only_option(
    stderr: &mut Stderr,
    command: &str,
    risk: Risk,
    is_selected: bool,
) -> Result<(), PickerError> {
    if is_selected {
        queue!(
            stderr,
            SetForegroundColor(Color::Cyan),
            Print("> "),
            ResetColor
        )?;
        queue!(stderr, SetAttribute(Attribute::Reverse), Print(command))?;
        queue!(stderr, SetAttribute(Attribute::Reset), Print(" "))?;
        render_risk_label(stderr, risk)?;
        queue!(stderr, Print("\r\n\r\n"))?;
    } else {
        queue!(stderr, Print(format!("  {command} ")))?;
        render_risk_label(stderr, risk)?;
        queue!(stderr, Print("\r\n\r\n"))?;
    }

    Ok(())
}

fn render_risk_label(stderr: &mut Stderr, risk: Risk) -> Result<(), PickerError> {
    let color = match risk {
        Risk::Safe => Color::Green,
        Risk::Dangerous => Color::Red,
    };

    queue!(
        stderr,
        SetForegroundColor(color),
        Print(format!("[{risk}]")),
        ResetColor
    )?;

    Ok(())
}

fn render_dangerous_confirmation(
    stderr: &mut Stderr,
    options: &CommandOptions,
    selected: usize,
) -> Result<(), PickerError> {
    let option = &options.options[selected];

    queue!(stderr, cursor::MoveTo(0, 0), Clear(ClearType::All))?;
    queue!(stderr, Print("Dangerous command\r\n\r\n"))?;
    queue!(stderr, Print(format!("{} ", option.title)))?;
    render_risk_label(stderr, option.risk)?;
    queue!(stderr, Print("\r\n"))?;
    queue!(stderr, Print(format!("{}\r\n\r\n", option.command)))?;
    queue!(
        stderr,
        Print("Press Enter again to run | q/Esc/Ctrl+C = cancel")
    )?;
    stderr.flush()?;

    Ok(())
}

fn read_dangerous_confirmation() -> Result<ConfirmDecision, PickerError> {
    loop {
        if let Event::Key(key) = event::read()? {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(ConfirmDecision::Cancel);
            }

            match key.code {
                KeyCode::Enter => return Ok(ConfirmDecision::Run),
                KeyCode::Char(value) if value.eq_ignore_ascii_case(&'q') => {
                    return Ok(ConfirmDecision::Cancel);
                }
                KeyCode::Esc => return Ok(ConfirmDecision::Cancel),
                _ => {}
            }
        }
    }
}

fn should_ignore_navigation(key: &KeyEvent, last_navigation: &mut Option<Instant>) -> bool {
    if !is_navigation_key(key) {
        return false;
    }

    if key.kind == KeyEventKind::Repeat {
        return true;
    }

    let now = Instant::now();
    if let Some(last) = *last_navigation {
        if now.duration_since(last) < NAVIGATION_DEBOUNCE {
            return true;
        }
    }

    *last_navigation = Some(now);
    false
}

fn is_navigation_key(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Up | KeyCode::Down => true,
        KeyCode::Char(value) => {
            value.eq_ignore_ascii_case(&'w') || value.eq_ignore_ascii_case(&'s')
        }
        _ => false,
    }
}

fn drain_pending_events() -> Result<(), PickerError> {
    for _ in 0..64 {
        if !event::poll(Duration::from_millis(0))? {
            break;
        }

        let _ = event::read()?;
    }

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
    use super::{
        PickerAction, PickerState, requires_dangerous_confirmation, should_ignore_navigation,
    };
    use crate::types::{CommandOption, CommandOptions, PickerResult, Risk};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    #[test]
    fn moves_selection_with_arrows_and_ws_keys() {
        let mut state = PickerState::new(3);

        state.handle_key(key(KeyCode::Down));
        assert_eq!(state.selected(), 1);

        state.handle_key(key(KeyCode::Char('s')));
        assert_eq!(state.selected(), 2);

        state.handle_key(key(KeyCode::Down));
        assert_eq!(state.selected(), 0);

        state.handle_key(key(KeyCode::Up));
        assert_eq!(state.selected(), 2);

        state.handle_key(key(KeyCode::Char('w')));
        assert_eq!(state.selected(), 1);
    }

    #[test]
    fn a_key_no_longer_moves_selection() {
        let mut state = PickerState::new(3);

        state.handle_key(key(KeyCode::Char('a')));

        assert_eq!(state.selected(), 0);
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
            state.handle_key(key(KeyCode::Char('c'))),
            Some(PickerAction::Copy)
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('r'))),
            Some(PickerAction::Regenerate)
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
            state.result(&options(), PickerAction::Copy),
            PickerResult::copy("Get-Service")
        );
        assert_eq!(
            state.result(&options(), PickerAction::Regenerate),
            PickerResult::regenerate()
        );
        assert_eq!(
            state.result(&options(), PickerAction::Cancel),
            PickerResult::cancel()
        );
    }

    #[test]
    fn requires_confirmation_only_for_dangerous_run_actions() {
        let options = CommandOptions {
            options: vec![
                option("Inspect", "Get-Process"),
                CommandOption {
                    title: "Kill".to_owned(),
                    command: "Stop-Process -Id 42".to_owned(),
                    risk: Risk::Dangerous,
                },
            ],
        };

        assert!(!requires_dangerous_confirmation(
            &options,
            0,
            PickerAction::Run,
            true
        ));
        assert!(requires_dangerous_confirmation(
            &options,
            1,
            PickerAction::Run,
            true
        ));
        assert!(!requires_dangerous_confirmation(
            &options,
            1,
            PickerAction::Run,
            false
        ));
        assert!(!requires_dangerous_confirmation(
            &options,
            1,
            PickerAction::Edit,
            true
        ));
        assert!(!requires_dangerous_confirmation(
            &options,
            1,
            PickerAction::Copy,
            true
        ));
        assert!(!requires_dangerous_confirmation(
            &options,
            1,
            PickerAction::Regenerate,
            true
        ));
    }

    #[test]
    fn ignores_repeated_navigation_events() {
        let mut last_navigation = None;

        assert!(!should_ignore_navigation(
            &KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut last_navigation
        ));
        assert!(should_ignore_navigation(
            &KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Repeat,
                state: KeyEventState::empty(),
            },
            &mut last_navigation
        ));
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
