use std::process::Command;

fn ai_core() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ai-core"));
    isolate_config(&mut command);
    command.env_remove("LLM_API_URL");
    command.env_remove("LLM_API_KEY");
    command.env_remove("LLM_MODEL");
    command
}

fn configured_ai_core() -> Command {
    let mut command = ai_core();
    command.env("LLM_API_URL", "https://example.test/v1/chat/completions");
    command.env("LLM_API_KEY", "test-secret-key");
    command.env("LLM_MODEL", "test-model");
    command
}

fn isolate_config(command: &mut Command) {
    let root = std::env::temp_dir().join(format!(
        "terminal-ai-cli-test-{}-{}",
        std::process::id(),
        unique_id()
    ));

    command.env("APPDATA", &root);
    command.env("XDG_CONFIG_HOME", &root);
    command.env("HOME", &root);
}

fn unique_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos()
}

#[test]
fn accepts_unquoted_prompt_without_stdout() {
    let output = configured_ai_core()
        .args(["what", "is", "running", "on", "port", "3000"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Prompt: what is running on port 3000"));
}

#[test]
fn shell_mode_keeps_stdout_empty_for_phase_two() {
    let output = configured_ai_core()
        .args(["--shell-mode", "--", "what", "is", "running"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn shell_mode_debug_writes_to_stderr_only() {
    let output = configured_ai_core()
        .args(["--shell-mode", "--debug", "--", "what", "is", "running"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("debug: shell_mode=true"));
    assert!(stderr.contains("what is running"));
    assert!(stderr.contains("test...-key"));
    assert!(!stderr.contains("test-secret-key"));
}

#[test]
fn empty_prompt_writes_usage_to_stderr_only() {
    let output = ai_core().arg("--shell-mode").output().expect("run ai-core");

    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ai <what do you want to do?>"));
}

#[test]
fn version_flag_uses_stdout() {
    let output = ai_core().arg("--version").output().expect("run ai-core");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("ai-core "));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn missing_config_writes_clear_error_to_stderr_only() {
    let output = ai_core()
        .args(["what", "is", "running"])
        .output()
        .expect("run ai-core");

    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing required config"));
    assert!(stderr.contains("LLM_API_URL"));
    assert!(stderr.contains("LLM_API_KEY"));
    assert!(stderr.contains("LLM_MODEL"));
}

#[test]
fn print_config_writes_redacted_json_to_stdout() {
    let output = configured_ai_core()
        .arg("--print-config")
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"api_url\": \"https://example.test/v1/chat/completions\""));
    assert!(stdout.contains("\"api_key\": \"test...-key\""));
    assert!(stdout.contains("\"model\": \"test-model\""));
    assert!(!stdout.contains("test-secret-key"));
}
