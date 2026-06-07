use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    sync::mpsc::{self, Receiver},
    thread,
};

fn ai_core() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ai-core"));
    let root = isolate_config(&mut command);
    command.env_remove("LLM_API_URL");
    command.env_remove("LLM_API_KEY");
    command.env_remove("LLM_MODEL");
    command.env("TERMINAL_AI_DOTENV_PATH", root.join(".env"));
    command
}

fn configured_ai_core(api_url: &str) -> Command {
    configured_ai_core_with_root(api_url).0
}

fn configured_ai_core_with_root(api_url: &str) -> (Command, std::path::PathBuf) {
    let mut command = ai_core();
    let root = command
        .get_envs()
        .find(|(key, _)| *key == std::ffi::OsStr::new("APPDATA"))
        .and_then(|(_, value)| value.map(std::path::PathBuf::from))
        .expect("isolated appdata");
    command.env("LLM_API_URL", api_url);
    command.env("LLM_API_KEY", "test-secret-key");
    command.env("LLM_MODEL", "test-model");
    (command, root)
}

fn isolate_config(command: &mut Command) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "terminal-ai-cli-test-{}-{}",
        std::process::id(),
        unique_id()
    ));

    command.env(
        "TERMINAL_AI_CONFIG_PATH",
        root.join("terminal-ai").join("config.json"),
    );
    command.env("APPDATA", &root);
    command.env("LOCALAPPDATA", &root);
    command.env("XDG_CONFIG_HOME", &root);
    command.env("XDG_DATA_HOME", &root);
    command.env("HOME", &root);
    command.env(
        "TERMINAL_AI_AGENT_LOG_DIR",
        root.join("terminal-ai").join("agent-logs"),
    );
    root
}

fn unique_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos()
}

#[test]
fn accepts_unquoted_prompt_without_stdout() {
    let server = mock_llm(vec![valid_options()]);
    let output = configured_ai_core(&server.url)
        .args(["what", "is", "running", "on", "port", "3000"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Prompt: what is running on port 3000"));
    assert!(stderr.contains(r#"{"action":"cancel"}"#));

    let request = server.request();
    assert!(request.contains("test-model"));
    assert!(request.contains("what is running on port 3000"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-secret-key")
    );
}

#[test]
fn shell_mode_prints_final_action_json_to_stdout() {
    let server = mock_llm(vec![valid_options()]);
    let output = configured_ai_core(&server.url)
        .args(["--shell-mode", "--", "what", "is", "running"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{\"action\":\"cancel\"}\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");

    let request = server.request();
    assert!(request.contains("what is running"));
}

#[test]
fn shell_mode_debug_writes_to_stderr_only() {
    let server = mock_llm(vec![valid_options()]);
    let output = configured_ai_core(&server.url)
        .args(["--shell-mode", "--debug", "--", "what", "is", "running"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{\"action\":\"cancel\"}\n"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("debug: shell_mode=true"));
    assert!(stderr.contains("what is running"));
    assert!(stderr.contains("test...-key"));
    assert!(!stderr.contains("test-secret-key"));
}

#[test]
fn agent_mode_runs_steps_from_mock_llm() {
    let server = mock_llm(vec![
        agent_pwd_step(1, 4),
        agent_pwd_step(2, 4),
        agent_pwd_step(3, 4),
        agent_done_step(4, 4),
    ]);
    let (mut command, root) = configured_ai_core_with_root(&server.url);
    let output = command
        .args(["--agent", "list", "files"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).trim().is_empty());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("agent mode starting"));
    assert!(stderr.contains("Step 1 / ~4"));
    assert!(stderr.contains("Done after 3 step(s)"));

    let first_request = server.request();
    assert!(first_request.contains("Original goal:\\nlist files"));

    let _ = server.request();
    let _ = server.request();
    let fourth_request = server.request();
    assert!(fourth_request.contains("Completed steps:"));
    assert!(fourth_request.contains("Command: pwd"));

    let logs = read_agent_logs(&root);
    assert_eq!(logs.len(), 1);
    let log = fs_read_to_string(&logs[0]);
    assert!(log.contains("\"goal\": \"list files\""));
    assert!(log.contains("\"total_duration_ms\""));
    assert!(log.contains("\"steps\""));
}

#[test]
fn agent_dry_run_skips_execution() {
    let temp_dir = test_temp_dir("dry-run");
    let server = mock_llm(vec![agent_create_file_step(), agent_done_step(2, 2)]);
    let output = configured_ai_core(&server.url)
        .current_dir(&temp_dir)
        .args(["--agent", "--dry-run", "create", "a", "file"])
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert!(!temp_dir.join("dry-run-created.txt").exists());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("agent mode starting (dry run)"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("dry run: assumed RunCommand"));
}

#[test]
fn agent_logs_lists_without_requiring_config() {
    let output = ai_core().arg("--agent-logs").output().expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert!(String::from_utf8_lossy(&output.stdout).contains("No agent logs found."));
}

#[cfg(windows)]
#[test]
fn powershell_agent_logs_routes_without_prompt_llm() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("shell")
        .join("powershell.ps1");
    let root = test_temp_dir("powershell-agent-logs");
    let command = format!(
        ". {}; $env:TERMINAL_AI_DOTENV_PATH = {}; $env:TERMINAL_AI_AGENT_LOG_DIR = {}; ai --agent-logs",
        ps_quote(&script.display().to_string()),
        ps_quote(&root.join(".env").display().to_string()),
        ps_quote(&root.join("agent-logs").display().to_string())
    );

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &command,
        ])
        .output()
        .expect("run powershell wrapper");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No agent logs found."));
    assert!(!stdout.contains("Prompt:"));
    assert!(!stdout.contains(r#""action":"#));
}

#[test]
fn dangerous_confirmation_can_abort_execution() {
    let temp_dir = test_temp_dir("dangerous-confirm");
    let server = mock_llm(vec![agent_dangerous_echo_step()]);
    let mut child = configured_ai_core(&server.url)
        .current_dir(&temp_dir)
        .args(["--agent", "dangerous", "test"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn ai-core");

    child
        .stdin
        .as_mut()
        .expect("child stdin")
        .write_all(b"q\n")
        .expect("write abort input");

    let output = child.wait_with_output().expect("wait ai-core");

    assert_eq!(output.status.code(), Some(130));
    assert!(!temp_dir.join("dangerous-created.txt").exists());
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
    let output = configured_ai_core("https://example.test/v1/chat/completions")
        .arg("--print-config")
        .output()
        .expect("run ai-core");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"api_url\": \"https://example.test/v1/chat/completions\""));
    assert!(stdout.contains("\"api_key\": \"test...-key\""));
    assert!(stdout.contains("\"model\": \"test-model\""));
    assert!(stdout.contains("\"dangerous_requires_confirm\": true"));
    assert!(stdout.contains("\"send_context\": true"));
    assert!(stdout.contains("\"send_recent_commands\": true"));
    assert!(stdout.contains("\"max_recent_commands\": 10"));
    assert!(stdout.contains("\"request_timeout_seconds\": 60"));
    assert!(stdout.contains("\"telemetry_enabled\": false"));
    assert!(stdout.contains("\"hide_descriptions\": false"));
    assert!(!stdout.contains("test-secret-key"));
}

#[test]
fn config_flag_requires_terminal_for_interactive_edit() {
    let output = ai_core().arg("--config").output().expect("run ai-core");

    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("interactive config requires a terminal"));
}

struct MockLlm {
    url: String,
    requests: Receiver<String>,
}

impl MockLlm {
    fn request(&self) -> String {
        self.requests.recv().expect("mock server request")
    }
}

fn mock_llm(contents: Vec<impl Into<String>>) -> MockLlm {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().expect("mock server address")
    );
    let (request_tx, requests) = mpsc::channel();
    let contents = contents.into_iter().map(Into::into).collect::<Vec<_>>();

    thread::spawn(move || {
        for content in contents {
            let (stream, _) = listener.accept().expect("accept mock request");
            handle_mock_request(stream, &content, &request_tx);
        }
    });

    MockLlm { url, requests }
}

fn handle_mock_request(
    mut stream: std::net::TcpStream,
    content: &str,
    request_tx: &mpsc::Sender<String>,
) {
    let mut buffer = [0_u8; 16 * 1024];
    let read = stream.read(&mut buffer).expect("read mock request");
    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
    request_tx.send(request).expect("record mock request");

    let body = serde_json::json!({
        "choices": [
            {
                "message": {
                    "content": content
                }
            }
        ]
    })
    .to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    stream
        .write_all(response.as_bytes())
        .expect("write mock response");
}

fn valid_options() -> &'static str {
    r#"{"options":[{"title":"Show process using port 3000","command":"Get-NetTCPConnection -LocalPort 3000 | Select-Object LocalAddress,LocalPort,OwningProcess","risk":"safe"}]}"#
}

fn agent_pwd_step(step: usize, total: usize) -> String {
    format!(
        r#"{{"step":{step},"total_estimated":{total},"action_type":"RunCommand","command":"pwd","risk":"Safe","reasoning":"Inspect the current directory."}}"#
    )
}

fn agent_done_step(step: usize, total: usize) -> String {
    format!(
        r#"{{"step":{step},"total_estimated":{total},"action_type":"Done","risk":"Safe","reasoning":"The requested inspection is complete."}}"#
    )
}

fn agent_create_file_step() -> String {
    r#"{"step":1,"total_estimated":2,"action_type":"RunCommand","command":"echo hello > dry-run-created.txt","risk":"Safe","reasoning":"Create the requested file."}"#.to_owned()
}

fn agent_dangerous_echo_step() -> String {
    r#"{"step":1,"total_estimated":1,"action_type":"RunCommand","command":"echo dangerous > dangerous-created.txt","risk":"Dangerous","reasoning":"Exercise dangerous confirmation."}"#.to_owned()
}

fn test_temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "terminal-ai-cli-test-{}-{}-{name}",
        std::process::id(),
        unique_id()
    ));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn read_agent_logs(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let dir = root.join("terminal-ai").join("agent-logs");
    let mut logs = std::fs::read_dir(dir)
        .expect("read agent logs")
        .map(|entry| entry.expect("log entry").path())
        .collect::<Vec<_>>();
    logs.sort();
    logs
}

fn fs_read_to_string(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).expect("read file")
}

#[cfg(windows)]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
