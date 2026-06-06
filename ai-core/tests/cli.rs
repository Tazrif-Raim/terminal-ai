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
    let mut command = ai_core();
    command.env("LLM_API_URL", api_url);
    command.env("LLM_API_KEY", "test-secret-key");
    command.env("LLM_MODEL", "test-model");
    command
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
    command.env("XDG_CONFIG_HOME", &root);
    command.env("HOME", &root);
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

fn mock_llm(contents: Vec<&'static str>) -> MockLlm {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().expect("mock server address")
    );
    let (request_tx, requests) = mpsc::channel();

    thread::spawn(move || {
        for content in contents {
            let (stream, _) = listener.accept().expect("accept mock request");
            handle_mock_request(stream, content, &request_tx);
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
