use std::{
    collections::HashMap,
    env, fmt, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crossterm::{
    QueueableCommand, cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    style::{ResetColor, SetBackgroundColor, Stylize},
    terminal,
};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::codex_oauth;

// ---------------------------------------------------------------------------
// Provider selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderMode {
    OpenAiCompatible,
    CodexOAuth,
}

impl ProviderMode {
    fn label(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI compatible API key",
            Self::CodexOAuth => "Codex OAuth (Experimental)",
        }
    }
}

impl Default for ProviderMode {
    fn default() -> Self {
        Self::OpenAiCompatible
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderMenuChoice {
    OpenAiApiKey,
    CodexOAuth,
    Exit,
}

const ENV_API_URL: &str = "LLM_API_URL";
const ENV_API_KEY: &str = "LLM_API_KEY";
const ENV_MODEL: &str = "LLM_MODEL";
const ENV_CONFIG_PATH: &str = "TERMINAL_AI_CONFIG_PATH";
const ENV_DOTENV_PATH: &str = "TERMINAL_AI_DOTENV_PATH";
const DEFAULT_MAX_OPTIONS: usize = 3;
const DEFAULT_DANGEROUS_REQUIRES_CONFIRM: bool = true;
const DEFAULT_SEND_CONTEXT: bool = true;
const DEFAULT_SEND_RECENT_COMMANDS: bool = true;
const DEFAULT_MAX_RECENT_COMMANDS: usize = 10;
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 60;
const DEFAULT_TELEMETRY_ENABLED: bool = false;
const DEFAULT_HIDE_DESCRIPTIONS: bool = false;
const CONFIG_INPUT_DEBOUNCE: Duration = Duration::from_millis(150);
const CONFIG_INPUT_POLL: Duration = Duration::from_millis(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedConfig {
    pub(crate) provider_mode: ProviderMode,
    pub(crate) api_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) default_shell: String,
    pub(crate) max_options: usize,
    pub(crate) dangerous_requires_confirm: bool,
    pub(crate) send_context: bool,
    pub(crate) send_recent_commands: bool,
    pub(crate) max_recent_commands: usize,
    pub(crate) request_timeout_seconds: u64,
    pub(crate) telemetry_enabled: bool,
    pub(crate) hide_descriptions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartialConfig {
    provider_mode: Option<ProviderMode>,
    api_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    default_shell: String,
    max_options: usize,
    dangerous_requires_confirm: bool,
    send_context: bool,
    send_recent_commands: bool,
    max_recent_commands: usize,
    request_timeout_seconds: u64,
    telemetry_enabled: bool,
    hide_descriptions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RedactedConfig {
    provider_mode: Option<ProviderMode>,
    api_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    default_shell: String,
    max_options: usize,
    dangerous_requires_confirm: bool,
    send_context: bool,
    send_recent_commands: bool,
    max_recent_commands: usize,
    request_timeout_seconds: u64,
    telemetry_enabled: bool,
    hide_descriptions: bool,
}

#[derive(Debug)]
pub(crate) enum ConfigError {
    ConfigDirUnavailable,
    InteractiveUnavailable,
    Cancelled,
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    Input {
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    Dotenv {
        path: PathBuf,
        source: dotenvy::Error,
    },
    Missing {
        fields: Vec<MissingField>,
        config_path: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MissingField {
    ApiUrl,
    ApiKey,
    Model,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
struct FileConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_mode: Option<ProviderMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_options: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dangerous_requires_confirm: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    send_context: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    send_recent_commands: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_recent_commands: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    telemetry_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hide_descriptions: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigEditMode {
    All,
    MissingOnly,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct LlmConfigValues {
    api_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
}

enum ConfigInput {
    Value(String),
    Cancel,
}

impl ResolvedConfig {
    pub(crate) fn redacted(&self) -> RedactedConfig {
        RedactedConfig {
            provider_mode: Some(self.provider_mode),
            api_url: Some(self.api_url.clone()),
            api_key: Some(mask_api_key(&self.api_key)),
            model: Some(self.model.clone()),
            default_shell: self.default_shell.clone(),
            max_options: self.max_options,
            dangerous_requires_confirm: self.dangerous_requires_confirm,
            send_context: self.send_context,
            send_recent_commands: self.send_recent_commands,
            max_recent_commands: self.max_recent_commands,
            request_timeout_seconds: self.request_timeout_seconds,
            telemetry_enabled: self.telemetry_enabled,
            hide_descriptions: self.hide_descriptions,
        }
    }
}

impl PartialConfig {
    pub(crate) fn redacted(&self) -> RedactedConfig {
        RedactedConfig {
            provider_mode: self.provider_mode,
            api_url: self.api_url.clone(),
            api_key: self.api_key.as_deref().map(mask_api_key),
            model: self.model.clone(),
            default_shell: self.default_shell.clone(),
            max_options: self.max_options,
            dangerous_requires_confirm: self.dangerous_requires_confirm,
            send_context: self.send_context,
            send_recent_commands: self.send_recent_commands,
            max_recent_commands: self.max_recent_commands,
            request_timeout_seconds: self.request_timeout_seconds,
            telemetry_enabled: self.telemetry_enabled,
            hide_descriptions: self.hide_descriptions,
        }
    }

    pub(crate) fn validate(&self, config_path: &Path) -> Result<(), ConfigError> {
        let missing = self.missing_fields();
        if missing.is_empty() {
            return Ok(());
        }

        Err(ConfigError::Missing {
            fields: missing,
            config_path: config_path.to_owned(),
        })
    }

    fn into_resolved(self, config_path: PathBuf) -> Result<ResolvedConfig, ConfigError> {
        self.validate(&config_path)?;

        let provider_mode = self.provider_mode.unwrap_or_default();

        Ok(ResolvedConfig {
            provider_mode,
            api_url: self.api_url.unwrap_or_else(default_codex_api_url),
            api_key: self.api_key.unwrap_or_default(),
            model: self.model.expect("model checked above"),
            default_shell: self.default_shell,
            max_options: self.max_options,
            dangerous_requires_confirm: self.dangerous_requires_confirm,
            send_context: self.send_context,
            send_recent_commands: self.send_recent_commands,
            max_recent_commands: self.max_recent_commands,
            request_timeout_seconds: self.request_timeout_seconds,
            telemetry_enabled: self.telemetry_enabled,
            hide_descriptions: self.hide_descriptions,
        })
    }

    fn missing_fields(&self) -> Vec<MissingField> {
        let is_codex = self.provider_mode == Some(ProviderMode::CodexOAuth);

        if is_codex {
            // Codex OAuth mode: only model is required
            if self.model.is_none() {
                return vec![MissingField::Model];
            }
            return vec![];
        }

        // OpenAI-compatible mode: all three required
        let mut fields = Vec::new();
        if self.api_url.is_none() {
            fields.push(MissingField::ApiUrl);
        }
        if self.api_key.is_none() {
            fields.push(MissingField::ApiKey);
        }
        if self.model.is_none() {
            fields.push(MissingField::Model);
        }
        fields
    }
}

impl MissingField {
    fn label(self) -> &'static str {
        match self {
            Self::ApiUrl => "api_url / LLM_API_URL",
            Self::ApiKey => "api_key / LLM_API_KEY",
            Self::Model => "model / LLM_MODEL",
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigDirUnavailable => {
                write!(formatter, "error: could not locate config directory")
            }
            Self::InteractiveUnavailable => {
                write!(formatter, "error: interactive config requires a terminal")
            }
            Self::Cancelled => write!(formatter, "cancelled config setup"),
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "error: could not read config file {}: {source}",
                    path.display()
                )
            }
            Self::Write { path, source } => {
                write!(
                    formatter,
                    "error: could not write config file {}: {source}",
                    path.display()
                )
            }
            Self::Input { source } => write!(formatter, "error: could not read input: {source}"),
            Self::Parse { path, source } => {
                write!(
                    formatter,
                    "error: could not parse config file {}: {source}",
                    path.display()
                )
            }
            Self::Dotenv { path, source } => {
                write!(
                    formatter,
                    "error: could not parse env file {}: {source}",
                    path.display()
                )
            }
            Self::Missing {
                fields,
                config_path,
            } => {
                let fields = fields
                    .iter()
                    .map(|field| field.label())
                    .collect::<Vec<_>>()
                    .join(", ");

                write!(
                    formatter,
                    "error: missing required config: {fields}. Set {ENV_API_URL}, {ENV_API_KEY}, and {ENV_MODEL}, or create {}.",
                    config_path.display()
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

pub(crate) fn default_config_path() -> Result<PathBuf, ConfigError> {
    if let Some(path) = clean(process_env(ENV_CONFIG_PATH)) {
        return Ok(PathBuf::from(path));
    }

    let dirs = BaseDirs::new().ok_or(ConfigError::ConfigDirUnavailable)?;

    Ok(dirs.config_dir().join("terminal-ai").join("config.json"))
}

pub(crate) fn load() -> Result<ResolvedConfig, ConfigError> {
    let path = default_config_path()?;
    let dotenv_path = discover_dotenv_path();
    load_from_path_with_dotenv(&path, process_env, dotenv_path.as_deref())
}

pub(crate) fn load_for_display() -> Result<(PartialConfig, PathBuf), ConfigError> {
    let path = default_config_path()?;
    let dotenv_path = discover_dotenv_path();
    let dotenv = load_dotenv_values(dotenv_path.as_deref())?;
    let config = load_partial_from_path(&path, process_env, &dotenv)?;

    Ok((config, path))
}

pub(crate) fn can_configure_interactively() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

pub(crate) fn configure_interactive(mode: ConfigEditMode) -> Result<ResolvedConfig, ConfigError> {
    if !can_configure_interactively() {
        return Err(ConfigError::InteractiveUnavailable);
    }

    let path = default_config_path()?;
    let dotenv_path = discover_dotenv_path();
    let dotenv = load_dotenv_values(dotenv_path.as_deref())?;
    let current = load_partial_from_path(&path, process_env, &dotenv)?;

    eprintln!("terminal-ai config");
    eprintln!("Config file: {}", path.display());
    eprintln!("{}", to_pretty_json(&current.redacted()));
    eprintln!();

    // Provider selection menu
    let current_provider = current.provider_mode.unwrap_or_default();
    let selection = select_provider_menu(current_provider)?;

    match selection {
        ProviderMenuChoice::OpenAiApiKey => {
            let mut updated = current;
            updated.provider_mode = Some(ProviderMode::OpenAiCompatible);

            eprintln!();
            eprintln!("Update values:");
            eprintln!("Press Enter to keep a shown value, or type q to cancel.");
            eprintln!();

            let values = prompt_for_llm_values(&updated, mode)?;
            save_llm_config_values(&path, values, Some(ProviderMode::OpenAiCompatible))?;

            eprintln!("Saved config to {}", path.display());
        }
        ProviderMenuChoice::CodexOAuth => {
            let store_path =
                codex_oauth::default_store_path().map_err(|_| ConfigError::ConfigDirUnavailable)?;
            let status = codex_oauth::get_status(&store_path);

            if status.logged_in {
                let plan = status.plan_type.as_deref().unwrap_or("unknown");
                let account = status.account_id.as_deref().unwrap_or("unknown");
                eprintln!("Already signed in to Codex (plan: {plan}, account: {account})");

                if status.is_expired {
                    eprintln!("Token is expired or near expiry. Re-authenticating...");
                    match codex_oauth::run_browser_login(
                        &store_path,
                        true,
                        &(),
                        &std::sync::atomic::AtomicBool::new(false),
                    ) {
                        Ok(_) => eprintln!("Re-authentication successful!"),
                        Err(e) => {
                            eprintln!("Warning: could not re-authenticate: {e}");
                            eprintln!("Continuing with existing token...");
                        }
                    }
                } else {
                    let action = prompt_reauth_or_continue()?;
                    if action {
                        eprintln!();
                        match codex_oauth::run_browser_login(
                            &store_path,
                            true,
                            &(),
                            &std::sync::atomic::AtomicBool::new(false),
                        ) {
                            Ok(_) => eprintln!("Re-authentication successful!"),
                            Err(e) => {
                                eprintln!("Warning: re-authentication failed: {e}");
                                eprintln!("Continuing with existing token...");
                            }
                        }
                    }
                }
            } else {
                eprintln!("Signing in to Codex...");
                match codex_oauth::run_browser_login(
                    &store_path,
                    true,
                    &(),
                    &std::sync::atomic::AtomicBool::new(false),
                ) {
                    Ok(_) => eprintln!("Sign-in successful!"),
                    Err(e) => {
                        eprintln!("Sign-in failed: {e}");
                        return Err(ConfigError::Cancelled);
                    }
                }
            }

            // Prompt for model
            eprintln!();
            let model = prompt_single_value("Model", current.model.as_deref())?;
            let mut file_config = read_file_config(&path)?;
            file_config.provider_mode = Some(ProviderMode::CodexOAuth);
            if let Some(m) = model {
                file_config.model = Some(m);
            }
            write_file_config(&path, &file_config)?;
            eprintln!("Saved config to {}", path.display());
        }
        ProviderMenuChoice::Exit => {}
    }

    load_from_path_with_dotenv(&path, process_env, dotenv_path.as_deref())
}

#[cfg(test)]
fn load_from_path(
    path: &Path,
    env_lookup: impl Fn(&str) -> Option<String>,
) -> Result<ResolvedConfig, ConfigError> {
    load_from_path_with_dotenv(path, env_lookup, None)
}

fn load_from_path_with_dotenv(
    path: &Path,
    env_lookup: impl Fn(&str) -> Option<String>,
    dotenv_path: Option<&Path>,
) -> Result<ResolvedConfig, ConfigError> {
    let dotenv = load_dotenv_values(dotenv_path)?;
    let config = load_partial_from_path(path, env_lookup, &dotenv)?;
    config.into_resolved(path.to_owned())
}

fn load_partial_from_path(
    path: &Path,
    env_lookup: impl Fn(&str) -> Option<String>,
    dotenv: &DotenvValues,
) -> Result<PartialConfig, ConfigError> {
    let file_config = read_file_config(path)?;

    Ok(PartialConfig {
        provider_mode: file_config.provider_mode,
        api_url: env_value(ENV_API_URL, &env_lookup, dotenv).or_else(|| clean(file_config.api_url)),
        api_key: env_value(ENV_API_KEY, &env_lookup, dotenv).or_else(|| clean(file_config.api_key)),
        model: env_value(ENV_MODEL, &env_lookup, dotenv).or_else(|| clean(file_config.model)),
        default_shell: clean(file_config.default_shell).unwrap_or_else(default_shell),
        max_options: file_config
            .max_options
            .unwrap_or(DEFAULT_MAX_OPTIONS)
            .clamp(1, 3),
        dangerous_requires_confirm: file_config
            .dangerous_requires_confirm
            .unwrap_or(DEFAULT_DANGEROUS_REQUIRES_CONFIRM),
        send_context: file_config.send_context.unwrap_or(DEFAULT_SEND_CONTEXT),
        send_recent_commands: file_config
            .send_recent_commands
            .unwrap_or(DEFAULT_SEND_RECENT_COMMANDS),
        max_recent_commands: file_config
            .max_recent_commands
            .unwrap_or(DEFAULT_MAX_RECENT_COMMANDS)
            .min(20),
        request_timeout_seconds: file_config
            .request_timeout_seconds
            .unwrap_or(DEFAULT_REQUEST_TIMEOUT_SECONDS)
            .clamp(5, 300),
        telemetry_enabled: file_config
            .telemetry_enabled
            .unwrap_or(DEFAULT_TELEMETRY_ENABLED),
        hide_descriptions: file_config
            .hide_descriptions
            .unwrap_or(DEFAULT_HIDE_DESCRIPTIONS),
    })
}

fn read_file_config(path: &Path) -> Result<FileConfig, ConfigError> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileConfig::default());
        }
        Err(source) => {
            return Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            });
        }
    };

    serde_json::from_str(&content).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source,
    })
}

fn write_file_config(path: &Path, config: &FileConfig) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: path.to_owned(),
            source,
        })?;
    }

    let content = serde_json::to_string_pretty(config).expect("file config serializes");
    fs::write(path, format!("{content}\n")).map_err(|source| ConfigError::Write {
        path: path.to_owned(),
        source,
    })
}

fn save_llm_config_values(
    path: &Path,
    values: LlmConfigValues,
    provider_mode: Option<ProviderMode>,
) -> Result<bool, ConfigError> {
    if values == LlmConfigValues::default() && provider_mode.is_none() {
        return Ok(false);
    }

    let mut config = read_file_config(path)?;

    if let Some(pm) = provider_mode {
        config.provider_mode = Some(pm);
    }

    if let Some(value) = values.api_url {
        config.api_url = Some(value);
    }

    if let Some(value) = values.api_key {
        config.api_key = Some(value);
    }

    if let Some(value) = values.model {
        config.model = Some(value);
    }

    write_file_config(path, &config)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Interactive provider menu (crossterm raw-mode)
// ---------------------------------------------------------------------------

fn select_provider_menu(current: ProviderMode) -> Result<ProviderMenuChoice, ConfigError> {
    let options = [
        ProviderMenuChoice::OpenAiApiKey,
        ProviderMenuChoice::CodexOAuth,
    ];
    const EXIT_INDEX: usize = 2;

    let _raw = RawModeGuard::enter()?;
    let mut selected = match current {
        ProviderMode::OpenAiCompatible => 0usize,
        ProviderMode::CodexOAuth => 1,
    };

    loop {
        render_provider_menu(selected, current);
        let choice = read_menu_key()?;

        match choice {
            MenuKey::Up => {
                if selected == 0 {
                    selected = EXIT_INDEX;
                } else {
                    selected -= 1;
                }
            }
            MenuKey::Down => {
                if selected < EXIT_INDEX {
                    selected += 1;
                }
            }
            MenuKey::Select => {
                if selected < options.len() {
                    return Ok(options[selected]);
                }
                return Ok(ProviderMenuChoice::Exit);
            }
            MenuKey::Exit => {
                return Ok(ProviderMenuChoice::Exit);
            }
            MenuKey::MoveToExit => {
                selected = EXIT_INDEX;
            }
        }
    }
}

fn render_provider_menu(selected: usize, active_provider: ProviderMode) {
    let options = [
        (
            ProviderMenuChoice::OpenAiApiKey,
            ProviderMode::OpenAiCompatible.label(),
        ),
        (
            ProviderMenuChoice::CodexOAuth,
            ProviderMode::CodexOAuth.label(),
        ),
    ];

    let mut stderr = io::stderr();
    let _ = stderr.queue(terminal::Clear(terminal::ClearType::All));
    let _ = stderr.queue(cursor::MoveTo(0, 0));

    let _ = write!(stderr, "Select an AI provider:\r\n");
    let _ = write!(stderr, "\r\n");

    for (i, &(ref choice, label)) in options.iter().enumerate() {
        let is_selected = i == selected;
        let is_active = matches!(
            (choice, active_provider),
            (
                ProviderMenuChoice::OpenAiApiKey,
                ProviderMode::OpenAiCompatible
            ) | (ProviderMenuChoice::CodexOAuth, ProviderMode::CodexOAuth)
        );

        // Highlight the selected line with a background color
        if is_selected {
            let _ = stderr.queue(SetBackgroundColor(crossterm::style::Color::DarkGrey));
        }
        let prefix = if is_active { "(*)" } else { "( )" };
        let prefix_display: Box<dyn std::fmt::Display> = if is_active {
            Box::new(prefix.green())
        } else {
            Box::new(prefix)
        };
        let _ = write!(stderr, "  {prefix_display} {label}\r\n");
        if is_selected {
            let _ = stderr.queue(ResetColor);
        }
    }

    // Separator
    let _ = write!(stderr, "  {}\r\n", "─".repeat(42));
    // Exit option
    let exit_label = "Exit";
    const EXIT_INDEX: usize = 2;
    let is_exit_selected = selected == EXIT_INDEX;
    if is_exit_selected {
        let _ = stderr.queue(SetBackgroundColor(crossterm::style::Color::DarkGrey));
        let _ = write!(stderr, "  {exit_label}\r\n");
        let _ = stderr.queue(ResetColor);
    } else {
        let _ = write!(stderr, "  {exit_label}\r\n");
    }

    let _ = write!(stderr, "\r\n");
    let _ = write!(stderr, "Use ↑/↓ to navigate, Enter to select.\r\n");
    let _ = stderr.flush();
}

// ---------------------------------------------------------------------------
// Interactive helpers (simple yes/no, single value prompts)
// ---------------------------------------------------------------------------

fn prompt_reauth_or_continue() -> Result<bool, ConfigError> {
    loop {
        eprint!("Re-authenticate? [y/N]: ");
        io::stderr()
            .flush()
            .map_err(|source| ConfigError::Input { source })?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|source| ConfigError::Input { source })?;

        match input.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" | "" => return Ok(false),
            "q" | "quit" | "cancel" => return Err(ConfigError::Cancelled),
            _ => {
                eprintln!("Please answer y or n.");
            }
        }
    }
}

fn prompt_single_value(label: &str, current: Option<&str>) -> Result<Option<String>, ConfigError> {
    loop {
        match current {
            Some(value) => eprint!("{label} [{value}]: "),
            None => eprint!("{label}: "),
        }
        io::stderr()
            .flush()
            .map_err(|source| ConfigError::Input { source })?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|source| ConfigError::Input { source })?;

        let trimmed = input.trim().to_owned();

        if trimmed.is_empty() {
            return Ok(None); // Keep current
        }

        if matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "q" | "quit" | "cancel"
        ) {
            return Err(ConfigError::Cancelled);
        }

        return Ok(Some(trimmed));
    }
}

// ---------------------------------------------------------------------------
// Raw-mode menu key reader
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum MenuKey {
    Up,
    Down,
    Select,
    Exit,
    MoveToExit,
}

fn read_menu_key() -> Result<MenuKey, ConfigError> {
    loop {
        let event = event::read().map_err(|source| ConfigError::Input { source })?;

        let Event::Key(key) = event else {
            continue;
        };

        if !is_key_press(key) {
            continue;
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('w') => return Ok(MenuKey::Up),
            KeyCode::Down | KeyCode::Char('s') => return Ok(MenuKey::Down),
            KeyCode::Right | KeyCode::Char('e') | KeyCode::Char('l') => {
                return Ok(MenuKey::MoveToExit);
            }
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('r') => {
                return Ok(MenuKey::Select);
            }
            KeyCode::Esc | KeyCode::Char('q') => return Ok(MenuKey::Exit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(MenuKey::Exit);
            }
            _ => {}
        }
    }
}

fn prompt_for_llm_values(
    current: &PartialConfig,
    mode: ConfigEditMode,
) -> Result<LlmConfigValues, ConfigError> {
    let edit_all = mode == ConfigEditMode::All;

    Ok(LlmConfigValues {
        api_url: prompt_config_value("API URL", current.api_url.as_deref(), false, edit_all)?,
        api_key: prompt_config_value("API key", current.api_key.as_deref(), true, edit_all)?,
        model: prompt_config_value("Model", current.model.as_deref(), false, edit_all)?,
    })
}

fn prompt_config_value(
    label: &str,
    current: Option<&str>,
    secret: bool,
    edit_existing: bool,
) -> Result<Option<String>, ConfigError> {
    if current.is_some() && !edit_existing {
        return Ok(None);
    }

    loop {
        write_config_prompt(label, current, secret)?;

        let input = if secret {
            read_secret_line()?
        } else {
            read_visible_line()?
        };

        match input {
            ConfigInput::Cancel => return Err(ConfigError::Cancelled),
            ConfigInput::Value(value) if value.is_empty() && current.is_some() => return Ok(None),
            ConfigInput::Value(value) if value.is_empty() => {
                eprintln!("{label} is required.");
            }
            ConfigInput::Value(value) => return Ok(Some(value)),
        }
    }
}

fn write_config_prompt(
    label: &str,
    current: Option<&str>,
    secret: bool,
) -> Result<(), ConfigError> {
    let mut stderr = io::stderr();

    match current {
        Some(value) if secret => write!(stderr, "{label} [{}]: ", mask_api_key(value)),
        Some(value) => write!(stderr, "{label} [{value}]: "),
        None => write!(stderr, "{label}: "),
    }
    .map_err(|source| ConfigError::Input { source })?;

    stderr
        .flush()
        .map_err(|source| ConfigError::Input { source })
}

fn read_visible_line() -> Result<ConfigInput, ConfigError> {
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|source| ConfigError::Input { source })?;

    Ok(config_input(value))
}

fn read_secret_line() -> Result<ConfigInput, ConfigError> {
    let raw_mode = RawModeGuard::enter()?;
    let mut value = String::new();
    let mut pending_event = debounce_leading_enter()?;

    loop {
        let event = match pending_event.take() {
            Some(event) => event,
            None => event::read().map_err(|source| ConfigError::Input { source })?,
        };

        let Event::Key(key) = event else {
            continue;
        };

        if !is_key_press(key) {
            continue;
        }

        match key.code {
            KeyCode::Enter => {
                drop(raw_mode);
                eprintln!();
                return Ok(config_input(value));
            }
            KeyCode::Esc => {
                drop(raw_mode);
                eprintln!();
                return Ok(ConfigInput::Cancel);
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                drop(raw_mode);
                eprintln!();
                return Ok(ConfigInput::Cancel);
            }
            KeyCode::Backspace => {
                value.pop();
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                value.push(character);
            }
            _ => {}
        }
    }
}

fn debounce_leading_enter() -> Result<Option<Event>, ConfigError> {
    let deadline = Instant::now() + CONFIG_INPUT_DEBOUNCE;

    loop {
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }

        let timeout = (deadline - now).min(CONFIG_INPUT_POLL);
        if !event::poll(timeout).map_err(|source| ConfigError::Input { source })? {
            continue;
        }

        let event = event::read().map_err(|source| ConfigError::Input { source })?;
        if matches!(&event, Event::Key(key) if is_key_press(*key) && key.code == KeyCode::Enter) {
            continue;
        }

        return Ok(Some(event));
    }
}

fn is_key_press(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
}

fn config_input(value: String) -> ConfigInput {
    let value = value.trim().to_owned();

    if matches!(value.to_ascii_lowercase().as_str(), "q" | "quit" | "cancel") {
        ConfigInput::Cancel
    } else {
        ConfigInput::Value(value)
    }
}

fn process_env(key: &str) -> Option<String> {
    env::var(key).ok()
}

type DotenvValues = HashMap<String, String>;

fn env_value(
    key: &str,
    env_lookup: &impl Fn(&str) -> Option<String>,
    dotenv: &DotenvValues,
) -> Option<String> {
    clean(env_lookup(key)).or_else(|| clean(dotenv.get(key).cloned()))
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn default_shell() -> String {
    if cfg!(windows) {
        "powershell".to_owned()
    } else {
        "sh".to_owned()
    }
}

fn default_codex_api_url() -> String {
    "https://api.openai.com/v1/chat/completions".to_owned()
}

fn load_dotenv_values(path: Option<&Path>) -> Result<DotenvValues, ConfigError> {
    let Some(path) = path else {
        return Ok(DotenvValues::new());
    };

    if !path.exists() {
        return Ok(DotenvValues::new());
    }

    let mut values = DotenvValues::new();
    let iter = dotenvy::from_path_iter(path).map_err(|source| ConfigError::Dotenv {
        path: path.to_owned(),
        source,
    })?;

    for item in iter {
        let (key, value) = item.map_err(|source| ConfigError::Dotenv {
            path: path.to_owned(),
            source,
        })?;
        values.insert(key, value);
    }

    Ok(values)
}

fn discover_dotenv_path() -> Option<PathBuf> {
    if let Some(path) = clean(process_env(ENV_DOTENV_PATH)) {
        return Some(PathBuf::from(path));
    }

    env::current_dir()
        .ok()
        .and_then(|path| find_project_dotenv(&path))
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|path| find_project_dotenv(&path))
        })
}

fn find_project_dotenv(start: &Path) -> Option<PathBuf> {
    let start = if start.is_file() {
        start.parent()?
    } else {
        start
    };

    for dir in start.ancestors() {
        let dotenv = dir.join(".env");
        if dotenv.exists() && dir.join(".env.example").exists() && dir.join("ai-core").exists() {
            return Some(dotenv);
        }
    }

    None
}

pub(crate) fn mask_api_key(value: &str) -> String {
    let value = value.trim();
    let char_count = value.chars().count();

    if char_count <= 4 {
        return "****".to_owned();
    }

    if char_count <= 8 {
        let start: String = value.chars().take(2).collect();
        let end: String = value
            .chars()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        return format!("{start}****{end}");
    }

    let start: String = value.chars().take(4).collect();
    let end: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    format!("{start}...{end}")
}

pub(crate) fn to_pretty_json(config: &RedactedConfig) -> String {
    serde_json::to_string_pretty(config).expect("redacted config serializes")
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self, ConfigError> {
        terminal::enable_raw_mode().map_err(|source| ConfigError::Input { source })?;
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
    use super::{
        ConfigError, FileConfig, LlmConfigValues, MissingField, load_from_path,
        load_from_path_with_dotenv, load_partial_from_path, mask_api_key, save_llm_config_values,
    };
    use std::{
        collections::HashMap,
        fs,
        path::{Path, PathBuf},
    };

    #[test]
    fn reads_config_file_values() {
        let path = test_config_path("reads_config_file_values");
        write_config(
            &path,
            r#"{
                "api_url": "https://example.test/v1/chat/completions",
                "api_key": "file-key",
                "model": "file-model",
                "default_shell": "powershell",
                "max_options": 2,
                "dangerous_requires_confirm": false,
                "send_context": false,
                "send_recent_commands": false,
                "max_recent_commands": 3,
                "request_timeout_seconds": 30,
                "telemetry_enabled": true,
                "hide_descriptions": true
            }"#,
        );

        let config = load_from_path(&path, |_| None).expect("load config");

        assert_eq!(config.api_url, "https://example.test/v1/chat/completions");
        assert_eq!(config.api_key, "file-key");
        assert_eq!(config.model, "file-model");
        assert_eq!(config.default_shell, "powershell");
        assert_eq!(config.max_options, 2);
        assert!(!config.dangerous_requires_confirm);
        assert!(!config.send_context);
        assert!(!config.send_recent_commands);
        assert_eq!(config.max_recent_commands, 3);
        assert_eq!(config.request_timeout_seconds, 30);
        assert!(config.telemetry_enabled);
        assert!(config.hide_descriptions);
    }

    #[test]
    fn environment_values_override_config_file() {
        let path = test_config_path("environment_values_override_config_file");
        write_config(
            &path,
            r#"{
                "api_url": "https://file.test",
                "api_key": "file-key",
                "model": "file-model"
            }"#,
        );

        let env = HashMap::from([
            ("LLM_API_URL", "https://env.test"),
            ("LLM_API_KEY", "env-key"),
            ("LLM_MODEL", "env-model"),
        ]);
        let config = load_from_path(&path, |key| env.get(key).map(ToString::to_string))
            .expect("load config");

        assert_eq!(config.api_url, "https://env.test");
        assert_eq!(config.api_key, "env-key");
        assert_eq!(config.model, "env-model");
    }

    #[test]
    fn reports_missing_required_values() {
        let path = test_config_path("reports_missing_required_values");
        let error = load_from_path(&path, |_| None).expect_err("missing config");

        match error {
            ConfigError::Missing { fields, .. } => {
                assert_eq!(
                    fields,
                    [
                        MissingField::ApiUrl,
                        MissingField::ApiKey,
                        MissingField::Model
                    ]
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn redacts_api_key_for_display() {
        assert_eq!(mask_api_key("abcd"), "****");
        assert_eq!(mask_api_key("abcdef"), "ab****ef");
        assert_eq!(mask_api_key("sk-123456789"), "sk-1...6789");
    }

    #[test]
    fn partial_config_display_uses_masked_api_key() {
        let path = test_config_path("partial_config_display_uses_masked_api_key");
        write_config(
            &path,
            r#"{
                "api_url": "https://example.test",
                "api_key": "secret-test-key",
                "model": "model"
            }"#,
        );

        let config = load_partial_from_path(&path, |_| None, &Default::default())
            .expect("load config")
            .redacted();

        assert_eq!(config.api_key.as_deref(), Some("secr...-key"));
    }

    #[test]
    fn clamps_max_options_to_supported_range() {
        let path = test_config_path("clamps_max_options_to_supported_range");
        write_config(
            &path,
            r#"{
                "api_url": "https://example.test",
                "api_key": "secret-test-key",
                "model": "model",
                "max_options": 99
            }"#,
        );

        let config = load_from_path(&path, |_| None).expect("load config");

        assert_eq!(config.max_options, 3);
    }

    #[test]
    fn defaults_dangerous_confirmation_to_enabled() {
        let path = test_config_path("defaults_dangerous_confirmation_to_enabled");
        write_config(
            &path,
            r#"{
                "api_url": "https://example.test",
                "api_key": "secret-test-key",
                "model": "model"
            }"#,
        );

        let config = load_from_path(&path, |_| None).expect("load config");

        assert!(config.dangerous_requires_confirm);
        assert!(config.send_context);
        assert!(config.send_recent_commands);
        assert_eq!(config.max_recent_commands, 10);
        assert_eq!(config.request_timeout_seconds, 60);
        assert!(!config.telemetry_enabled);
        assert!(!config.hide_descriptions);
    }

    #[test]
    fn clamps_max_recent_commands_to_reasonable_limit() {
        let path = test_config_path("clamps_max_recent_commands_to_reasonable_limit");
        write_config(
            &path,
            r#"{
                "api_url": "https://example.test",
                "api_key": "secret-test-key",
                "model": "model",
                "max_recent_commands": 100
            }"#,
        );

        let config = load_from_path(&path, |_| None).expect("load config");

        assert_eq!(config.max_recent_commands, 20);
    }

    #[test]
    fn clamps_request_timeout_to_reasonable_range() {
        let path = test_config_path("clamps_request_timeout_to_reasonable_range");
        write_config(
            &path,
            r#"{
                "api_url": "https://example.test",
                "api_key": "secret-test-key",
                "model": "model",
                "request_timeout_seconds": 1
            }"#,
        );

        let config = load_from_path(&path, |_| None).expect("load config");

        assert_eq!(config.request_timeout_seconds, 5);
    }

    #[test]
    fn loads_llm_values_from_dotenv_file() {
        let config_path = test_config_path("loads_llm_values_from_dotenv_file");
        let dotenv_path = config_path.with_file_name(".env");
        write_config(&config_path, r#"{}"#);
        write_config(
            &dotenv_path,
            r#"
LLM_API_URL=https://dotenv.test/v1/chat/completions
LLM_API_KEY=dotenv-key
LLM_MODEL=dotenv-model
"#,
        );

        let config = load_from_path_with_dotenv(&config_path, |_| None, Some(&dotenv_path))
            .expect("load config");

        assert_eq!(config.api_url, "https://dotenv.test/v1/chat/completions");
        assert_eq!(config.api_key, "dotenv-key");
        assert_eq!(config.model, "dotenv-model");
    }

    #[test]
    fn process_env_overrides_dotenv_file() {
        let config_path = test_config_path("process_env_overrides_dotenv_file");
        let dotenv_path = config_path.with_file_name(".env");
        write_config(&config_path, r#"{}"#);
        write_config(
            &dotenv_path,
            r#"
LLM_API_URL=https://dotenv.test
LLM_API_KEY=dotenv-key
LLM_MODEL=dotenv-model
"#,
        );
        let env = HashMap::from([
            ("LLM_API_URL", "https://env.test"),
            ("LLM_API_KEY", "env-key"),
            ("LLM_MODEL", "env-model"),
        ]);

        let config = load_from_path_with_dotenv(
            &config_path,
            |key| env.get(key).map(ToString::to_string),
            Some(&dotenv_path),
        )
        .expect("load config");

        assert_eq!(config.api_url, "https://env.test");
        assert_eq!(config.api_key, "env-key");
        assert_eq!(config.model, "env-model");
    }

    fn test_config_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir()
            .join("terminal-ai-config-tests")
            .join(format!("{}-{name}", std::process::id()))
            .join("config.json");

        let _ = fs::remove_file(&path);
        path
    }

    fn write_config(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().expect("config parent")).expect("create config dir");
        fs::write(path, content).expect("write config");
    }

    #[test]
    fn saves_llm_values_without_dropping_optional_settings() {
        let path = test_config_path("saves_llm_values_without_dropping_optional_settings");
        write_config(
            &path,
            r#"{
                "default_shell": "powershell",
                "max_options": 2,
                "dangerous_requires_confirm": false,
                "send_context": false,
                "send_recent_commands": false,
                "max_recent_commands": 3,
                "request_timeout_seconds": 30,
                "telemetry_enabled": true,
                "hide_descriptions": true
            }"#,
        );

        save_llm_config_values(
            &path,
            LlmConfigValues {
                api_url: Some("https://example.test".to_owned()),
                api_key: Some("secret-key".to_owned()),
                model: Some("test-model".to_owned()),
            },
            None,
        )
        .expect("save llm values");

        let saved: FileConfig =
            serde_json::from_str(&fs::read_to_string(&path).expect("read saved config"))
                .expect("parse saved config");

        assert_eq!(saved.api_url.as_deref(), Some("https://example.test"));
        assert_eq!(saved.api_key.as_deref(), Some("secret-key"));
        assert_eq!(saved.model.as_deref(), Some("test-model"));
        assert_eq!(saved.default_shell.as_deref(), Some("powershell"));
        assert_eq!(saved.max_options, Some(2));
        assert_eq!(saved.dangerous_requires_confirm, Some(false));
        assert_eq!(saved.send_context, Some(false));
        assert_eq!(saved.send_recent_commands, Some(false));
        assert_eq!(saved.max_recent_commands, Some(3));
        assert_eq!(saved.request_timeout_seconds, Some(30));
        assert_eq!(saved.telemetry_enabled, Some(true));
        assert_eq!(saved.hide_descriptions, Some(true));
    }
}
