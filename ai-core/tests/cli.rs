use std::process::Command;

fn ai_core() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ai-core"))
}

#[test]
fn accepts_unquoted_prompt_without_stdout() {
    let output = ai_core()
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
    let output = ai_core()
        .args(["--shell-mode", "--", "what", "is", "running"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn shell_mode_debug_writes_to_stderr_only() {
    let output = ai_core()
        .args(["--shell-mode", "--debug", "--", "what", "is", "running"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("debug: shell_mode=true"));
    assert!(stderr.contains("what is running"));
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
