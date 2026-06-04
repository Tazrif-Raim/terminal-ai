use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

const ENV_API_URL: &str = "LLM_API_URL";
const ENV_API_KEY: &str = "LLM_API_KEY";
const ENV_MODEL: &str = "LLM_MODEL";
const DEFAULT_MAX_OPTIONS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedConfig {
    pub(crate) api_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) default_shell: String,
    pub(crate) max_options: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartialConfig {
    api_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    default_shell: String,
    max_options: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RedactedConfig {
    api_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    default_shell: String,
    max_options: usize,
}

#[derive(Debug)]
pub(crate) enum ConfigError {
    ConfigDirUnavailable,
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
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

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FileConfig {
    api_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    default_shell: Option<String>,
    max_options: Option<usize>,
}

impl ResolvedConfig {
    pub(crate) fn redacted(&self) -> RedactedConfig {
        RedactedConfig {
            api_url: Some(self.api_url.clone()),
            api_key: Some(mask_api_key(&self.api_key)),
            model: Some(self.model.clone()),
            default_shell: self.default_shell.clone(),
            max_options: self.max_options,
        }
    }
}

impl PartialConfig {
    pub(crate) fn redacted(&self) -> RedactedConfig {
        RedactedConfig {
            api_url: self.api_url.clone(),
            api_key: self.api_key.as_deref().map(mask_api_key),
            model: self.model.clone(),
            default_shell: self.default_shell.clone(),
            max_options: self.max_options,
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

        Ok(ResolvedConfig {
            api_url: self.api_url.expect("api_url checked above"),
            api_key: self.api_key.expect("api_key checked above"),
            model: self.model.expect("model checked above"),
            default_shell: self.default_shell,
            max_options: self.max_options,
        })
    }

    fn missing_fields(&self) -> Vec<MissingField> {
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
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "error: could not read config file {}: {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(
                    formatter,
                    "error: could not parse config file {}: {source}",
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
    let dirs = BaseDirs::new().ok_or(ConfigError::ConfigDirUnavailable)?;

    Ok(dirs.config_dir().join("terminal-ai").join("config.json"))
}

pub(crate) fn load() -> Result<ResolvedConfig, ConfigError> {
    let path = default_config_path()?;
    load_from_path(&path, process_env)
}

pub(crate) fn load_for_display() -> Result<(PartialConfig, PathBuf), ConfigError> {
    let path = default_config_path()?;
    let config = load_partial_from_path(&path, process_env)?;

    Ok((config, path))
}

fn load_from_path(
    path: &Path,
    env_lookup: impl Fn(&str) -> Option<String>,
) -> Result<ResolvedConfig, ConfigError> {
    let config = load_partial_from_path(path, env_lookup)?;
    config.into_resolved(path.to_owned())
}

fn load_partial_from_path(
    path: &Path,
    env_lookup: impl Fn(&str) -> Option<String>,
) -> Result<PartialConfig, ConfigError> {
    let file_config = read_file_config(path)?;

    Ok(PartialConfig {
        api_url: env_value(ENV_API_URL, &env_lookup).or_else(|| clean(file_config.api_url)),
        api_key: env_value(ENV_API_KEY, &env_lookup).or_else(|| clean(file_config.api_key)),
        model: env_value(ENV_MODEL, &env_lookup).or_else(|| clean(file_config.model)),
        default_shell: clean(file_config.default_shell).unwrap_or_else(default_shell),
        max_options: file_config
            .max_options
            .unwrap_or(DEFAULT_MAX_OPTIONS)
            .clamp(1, 3),
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

fn process_env(key: &str) -> Option<String> {
    env::var(key).ok()
}

fn env_value(key: &str, env_lookup: &impl Fn(&str) -> Option<String>) -> Option<String> {
    clean(env_lookup(key))
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

#[cfg(test)]
mod tests {
    use super::{ConfigError, MissingField, load_from_path, load_partial_from_path, mask_api_key};
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
                "max_options": 2
            }"#,
        );

        let config = load_from_path(&path, |_| None).expect("load config");

        assert_eq!(config.api_url, "https://example.test/v1/chat/completions");
        assert_eq!(config.api_key, "file-key");
        assert_eq!(config.model, "file-model");
        assert_eq!(config.default_shell, "powershell");
        assert_eq!(config.max_options, 2);
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

        let config = load_partial_from_path(&path, |_| None)
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
}
