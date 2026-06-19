//! ChatGPT OAuth integration for OpenAI Codex models.
//!
//! Implements OAuth 2.0 Authorization Code Flow with PKCE against the OpenAI
//! auth endpoints used by Codex/ChatGPT subscription auth, plus a file-backed
//! token store and refresh logic.

use std::{
    fmt,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use directories::BaseDirs;
use rand::Rng;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// OAuth constants
// ---------------------------------------------------------------------------

const CHATGPT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CHATGPT_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const CHATGPT_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub(crate) const CHATGPT_CODEX_RESPONSES_URL: &str =
    "https://chatgpt.com/backend-api/codex/responses";
pub(crate) const CHATGPT_ACCOUNT_ID_HEADER: &str = "ChatGPT-Account-Id";
pub(crate) const ORIGINATOR_HEADER: &str = "originator";
pub(crate) const ORIGINATOR_VALUE: &str = "terminal-ai";
const REDIRECT_HOST: &str = "localhost";
const REDIRECT_PORT: u16 = 1455;
const REDIRECT_PATH: &str = "/auth/callback";
const DEFAULT_SCOPE: &str = "openid profile email offline_access";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(300);
const REFRESH_SKEW_SECS: i64 = 300; // 5 minutes
const POLL_INTERVAL: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Token types
// ---------------------------------------------------------------------------

/// A stored ChatGPT OAuth token bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CodexToken {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    #[serde(rename = "expires_at")]
    expires_at_iso: String,
    pub(crate) account_id: Option<String>,
    pub(crate) plan_type: Option<String>,
    pub(crate) user_id: Option<String>,
    pub(crate) id_token: Option<String>,
}

impl CodexToken {
    fn expires_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::parse_from_rfc3339(&self.expires_at_iso)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    }

    fn is_expired(&self) -> bool {
        self.expires_at()
            .map(|expires| {
                let now = chrono::Utc::now();
                now >= expires - chrono::TimeDelta::seconds(REFRESH_SKEW_SECS)
            })
            .unwrap_or(true)
    }

    fn from_response(
        payload: &serde_json::Value,
        fallback_refresh: Option<&str>,
    ) -> Result<Self, CodexOAuthError> {
        let access_token = payload
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CodexOAuthError::TokenExchange("OAuth response missing access_token".to_owned())
            })?;

        let refresh_token = payload
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .or(fallback_refresh)
            .ok_or_else(|| {
                CodexOAuthError::TokenExchange("OAuth response missing refresh_token".to_owned())
            })?;

        let expires_in = payload
            .get("expires_in")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if expires_in <= 0 {
            return Err(CodexOAuthError::TokenExchange(
                "OAuth response has missing or non-positive expires_in".to_owned(),
            ));
        }

        let expires_at = chrono::Utc::now() + chrono::TimeDelta::seconds(expires_in);
        let id_token = payload.get("id_token").and_then(|v| v.as_str());

        let (account_id, plan_type, user_id) = if let Some(id_token) = id_token {
            extract_chatgpt_claims(id_token)
        } else {
            (None, None, None)
        };

        Ok(Self {
            access_token: access_token.to_owned(),
            refresh_token: refresh_token.to_owned(),
            expires_at_iso: expires_at.to_rfc3339(),
            account_id,
            plan_type,
            user_id,
            id_token: id_token.map(String::from),
        })
    }
}

// ---------------------------------------------------------------------------
// Auth status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct CodexAuthStatus {
    pub(crate) logged_in: bool,
    #[allow(dead_code)]
    pub(crate) store_path: PathBuf,
    pub(crate) account_id: Option<String>,
    pub(crate) plan_type: Option<String>,
    pub(crate) is_expired: bool,
    #[allow(dead_code)]
    pub(crate) unreadable_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) enum CodexOAuthError {
    ConfigDirUnavailable,
    #[allow(dead_code)]
    StorePathDetermination,
    TokenExchange(String),
    Http(String),
    Io(std::io::Error),
    TokenRefresh(String),
    #[allow(dead_code)]
    LoginCancelled,
    Timeout,
    StateMismatch,
    MissingCode,
    ServerBind(String),
    NoToken,
    Parse(String),
}

impl fmt::Display for CodexOAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigDirUnavailable => write!(f, "could not locate config directory"),
            Self::StorePathDetermination => write!(f, "could not determine token store path"),
            Self::TokenExchange(msg) => write!(f, "token exchange failed: {msg}"),
            Self::Http(msg) => write!(f, "HTTP error: {msg}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::TokenRefresh(msg) => write!(f, "token refresh failed: {msg}"),
            Self::LoginCancelled => write!(f, "sign-in was cancelled"),
            Self::Timeout => write!(f, "timed out waiting for OAuth callback"),
            Self::StateMismatch => write!(f, "OAuth callback state mismatch (CSRF)"),
            Self::MissingCode => write!(f, "OAuth callback did not include authorization code"),
            Self::ServerBind(msg) => write!(f, "could not bind callback server: {msg}"),
            Self::NoToken => write!(f, "no ChatGPT OAuth token found"),
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for CodexOAuthError {}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Default path for the ChatGPT OAuth token store.
pub(crate) fn default_store_path() -> Result<PathBuf, CodexOAuthError> {
    let dirs = BaseDirs::new().ok_or(CodexOAuthError::ConfigDirUnavailable)?;
    let config_dir = dirs.config_dir().join("terminal-ai");
    Ok(config_dir.join("codex-auth.json"))
}

/// Return the current ChatGPT OAuth sign‑in state (passive inspect, no refresh).
///
/// The function never attempts a token refresh; it only reads the on‑disk JSON file.
/// Errors are reported via `unreadable_reason` so callers can surface a helpful UI message.
pub(crate) fn get_status(store_path: &Path) -> CodexAuthStatus {
    let content = match std::fs::read_to_string(store_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return CodexAuthStatus {
                logged_in: false,
                store_path: store_path.to_owned(),
                account_id: None,
                plan_type: None,
                is_expired: false,
                unreadable_reason: None,
            };
        }
        Err(e) => {
            return CodexAuthStatus {
                logged_in: false,
                store_path: store_path.to_owned(),
                account_id: None,
                plan_type: None,
                is_expired: false,
                unreadable_reason: Some(format!("failed to read token store: {e}")),
            };
        }
    };

    let token: CodexToken = match serde_json::from_str(&content) {
        Ok(t) => t,
        Err(e) => {
            return CodexAuthStatus {
                logged_in: false,
                store_path: store_path.to_owned(),
                account_id: None,
                plan_type: None,
                is_expired: false,
                unreadable_reason: Some(format!("token store is not valid JSON: {e}")),
            };
        }
    };

    CodexAuthStatus {
        logged_in: true,
        store_path: store_path.to_owned(),
        account_id: token.account_id.clone(),
        plan_type: token.plan_type.clone(),
        is_expired: token.is_expired(),
        unreadable_reason: None,
    }
}

/// Return the current token bundle, refreshing if necessary.
pub(crate) fn get_token(store_path: &Path) -> Result<CodexToken, CodexOAuthError> {
    let token = load_token(store_path)?;

    if !token.is_expired() {
        return Ok(token);
    }

    refresh_token_inner(store_path, &token)
}

/// Whether a ChatGPT OAuth token is stored on disk.
#[allow(dead_code)]
pub(crate) fn is_logged_in(store_path: &Path) -> bool {
    get_status(store_path).logged_in
}

/// Delete the stored ChatGPT OAuth token.
#[allow(dead_code)]
pub(crate) fn logout(store_path: &Path) -> bool {
    if !store_path.exists() {
        return false;
    }
    std::fs::remove_file(store_path).is_ok()
}

// ---------------------------------------------------------------------------
// Browser login flow
// ---------------------------------------------------------------------------

/// UI hook for displaying the authorize URL.
pub trait CodexLoginInteraction {
    /// Called once the authorize URL is ready. `opened_in_browser` indicates whether the
    /// implementation attempted to launch a browser automatically.
    fn show_authorize_url(&self, url: &str, opened_in_browser: bool);
}

// Default implementation that prints to stderr (fallback for non‑UI callers).
impl CodexLoginInteraction for () {
    fn show_authorize_url(&self, url: &str, opened_in_browser: bool) {
        if opened_in_browser {
            eprintln!("Browser opened to: {url}");
        } else {
            eprintln!("Open this URL in a browser: {url}");
        }
    }
}

// Run the ChatGPT OAuth authorization code flow with PKCE.
pub(crate) fn run_browser_login(
    store_path: &Path,
    open_browser: bool,
    interaction: &dyn CodexLoginInteraction,
    cancel_event: &std::sync::atomic::AtomicBool,
) -> Result<CodexAuthStatus, CodexOAuthError> {
    let redirect_uri = format!("http://{REDIRECT_HOST}:{REDIRECT_PORT}{REDIRECT_PATH}");

    let (verifier, challenge) = generate_pkce_pair();
    let state = generate_state();
    let authorize_url = build_authorize_url(&redirect_uri, &state, &challenge);

    // Show URL via UI hook before attempting to open the browser.
    interaction.show_authorize_url(&authorize_url, false);

    if open_browser {
        match webbrowser::open(&authorize_url) {
            Ok(_) => interaction.show_authorize_url(&authorize_url, true),
            Err(e) => {
                eprintln!("(could not launch browser: {e}; copy the URL above manually)");
            }
        }
    }

    // Wait for OAuth callback, cancel‑aware.
    let callback_result = wait_for_oauth_callback(cancel_event)?;

    // Validate state (CSRF check)
    match callback_result.get("state") {
        Some(cb_state) if cb_state == &state => {}
        _ => return Err(CodexOAuthError::StateMismatch),
    }

    // Check for OAuth error
    if let Some(error) = callback_result.get("error") {
        let description = callback_result
            .get("error_description")
            .cloned()
            .unwrap_or_default();
        return Err(CodexOAuthError::TokenExchange(format!(
            "OAuth callback returned error: {error} {description}"
        )));
    }

    // Extract authorization code
    let code = callback_result
        .get("code")
        .ok_or(CodexOAuthError::MissingCode)?;

    // Exchange code for tokens
    let payload = exchange_code(&redirect_uri, code, &verifier)?;
    let token = CodexToken::from_response(&payload, None)?;

    // Save token securely (atomic write, 0o600 perms on Unix).
    save_token(store_path, &token)?;

    Ok(get_status(store_path))
}

// ---------------------------------------------------------------------------
// PKCE helpers
// ---------------------------------------------------------------------------

fn generate_pkce_pair() -> (String, String) {
    // Code verifier: 64 random bytes, base64url-encoded (no padding)
    let mut random_bytes = [0u8; 64];
    rand::thread_rng().fill(&mut random_bytes);
    let verifier = base64_url_encode(&random_bytes);

    // Code challenge: SHA-256(verifier), base64url-encoded (no padding)
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    let challenge = base64_url_encode(&digest);

    (verifier, challenge)
}

fn generate_state() -> String {
    let mut random_bytes = [0u8; 32];
    rand::thread_rng().fill(&mut random_bytes);
    base64_url_encode(&random_bytes)
}

fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

// ---------------------------------------------------------------------------
// URL helpers – now using the `url` crate for robust encoding/decoding.
// ---------------------------------------------------------------------------

fn build_authorize_url(redirect_uri: &str, state: &str, code_challenge: &str) -> String {
    use url::form_urlencoded;
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", CHATGPT_CLIENT_ID)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", DEFAULT_SCOPE)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .finish();
    format!("{CHATGPT_AUTHORIZE_URL}?{query}")
}

fn urldecode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hi = (bytes[index + 1] as char).to_digit(16);
                let lo = (bytes[index + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    decoded.push(((hi << 4) | lo) as u8);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

// ---------------------------------------------------------------------------
// OAuth callback HTTP server
// ---------------------------------------------------------------------------

fn wait_for_oauth_callback(
    cancel_event: &std::sync::atomic::AtomicBool,
) -> Result<std::collections::HashMap<String, String>, CodexOAuthError> {
    let listener = TcpListener::bind((REDIRECT_HOST, REDIRECT_PORT))
        .map_err(|e| CodexOAuthError::ServerBind(format!("{e}")))?;

    listener
        .set_nonblocking(true)
        .map_err(CodexOAuthError::Io)?;

    let deadline = Instant::now() + OAUTH_TIMEOUT;

    while Instant::now() < deadline {
        if cancel_event.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(CodexOAuthError::LoginCancelled);
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(params) = handle_callback_request(stream)? {
                    return Ok(params);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL_INTERVAL);
                continue;
            }
            Err(e) => return Err(CodexOAuthError::Io(e)),
        }
    }

    Err(CodexOAuthError::Timeout)
}

fn handle_callback_request(
    mut stream: TcpStream,
) -> Result<Option<std::collections::HashMap<String, String>>, CodexOAuthError> {
    let reader = BufReader::new(&stream);
    let request_line = reader
        .lines()
        .next()
        .ok_or_else(|| CodexOAuthError::Parse("empty request".to_owned()))?
        .map_err(CodexOAuthError::Io)?;

    // Parse the request line: GET /auth/callback?code=...&state=... HTTP/1.1
    let parts: Vec<&str> = request_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err(CodexOAuthError::Parse("malformed request line".to_owned()));
    }

    let target = parts[1];
    let (path, query_string) = target
        .split_once('?')
        .map_or((target, ""), |(path, query)| (path, query));

    if path != REDIRECT_PATH {
        let body = oauth_error_html(
            "ChatGPT sign-in failed",
            "Unexpected OAuth callback path. Return to your terminal and try again.",
        );
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        return Ok(None);
    }

    let mut params = std::collections::HashMap::new();
    for pair in query_string.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(urldecode(key), urldecode(value));
    }

    // Send response
    let has_error = params.contains_key("error");
    let (status_line, body) = if has_error {
        let description = params.get("error_description").cloned().unwrap_or_default();
        (
            "HTTP/1.1 200 OK\r\n",
            oauth_error_html("ChatGPT sign-in failed", &description),
        )
    } else {
        (
            "HTTP/1.1 200 OK\r\n",
            oauth_success_html(
                "ChatGPT sign-in complete",
                "You can close this browser tab and return to your terminal.",
            ),
        )
    };

    let response = format!(
        "{status_line}Content-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    Ok(Some(params))
}

// ---------------------------------------------------------------------------
// Token exchange
// ---------------------------------------------------------------------------

fn exchange_code(
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
) -> Result<serde_json::Value, CodexOAuthError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| CodexOAuthError::Http(e.to_string()))?;

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", CHATGPT_CLIENT_ID),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post(CHATGPT_TOKEN_URL)
        .form(&params)
        .send()
        .map_err(|e| CodexOAuthError::Http(e.to_string()))?;

    parse_token_response(response, "token response").map_err(CodexOAuthError::TokenExchange)
}

// ---------------------------------------------------------------------------
// Token refresh
// ---------------------------------------------------------------------------

fn refresh_token_inner(
    store_path: &Path,
    token: &CodexToken,
) -> Result<CodexToken, CodexOAuthError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| CodexOAuthError::Http(e.to_string()))?;

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", &token.refresh_token),
        ("client_id", CHATGPT_CLIENT_ID),
    ];

    let response = client
        .post(CHATGPT_TOKEN_URL)
        .form(&params)
        .send()
        .map_err(|e| CodexOAuthError::Http(e.to_string()))?;

    let body = match parse_token_response(response, "refresh response") {
        Ok(body) => body,
        Err(message) if message.contains("invalid_grant") => {
            return Err(CodexOAuthError::TokenRefresh(
                "ChatGPT session expired. Run `ai --config` and select Codex OAuth to sign in again."
                    .to_owned(),
            ));
        }
        Err(message) => return Err(CodexOAuthError::TokenRefresh(message)),
    };

    let new_token = CodexToken::from_response(&body, Some(&token.refresh_token))?;
    save_token(store_path, &new_token)?;
    Ok(new_token)
}

fn parse_token_response(
    response: Response,
    context: &'static str,
) -> Result<serde_json::Value, String> {
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("failed to read {context}: {e}"))?;
    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("failed to parse {context}: {e}; body: {}", trim_body(&body)))?;

    if status.is_success() {
        return Ok(parsed);
    }

    let error = parsed
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown error");
    let description = parsed
        .get("error_description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let detail = if description.is_empty() {
        error.to_owned()
    } else {
        format!("{error}: {description}")
    };

    Err(format!("HTTP {status}: {detail}"))
}

fn trim_body(body: &str) -> String {
    const MAX_LEN: usize = 500;
    let body = body.trim();
    if body.chars().count() <= MAX_LEN {
        return body.to_owned();
    }

    let mut truncated: String = body.chars().take(MAX_LEN).collect();
    truncated.push_str("...");
    truncated
}

// ---------------------------------------------------------------------------
// Token file storage
// ---------------------------------------------------------------------------

fn load_token(store_path: &Path) -> Result<CodexToken, CodexOAuthError> {
    let content = std::fs::read_to_string(store_path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => CodexOAuthError::NoToken,
        _ => CodexOAuthError::Io(e),
    })?;

    serde_json::from_str(&content).map_err(|e| CodexOAuthError::Parse(e.to_string()))
}

fn save_token(store_path: &Path, token: &CodexToken) -> Result<(), CodexOAuthError> {
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    if let Some(parent) = store_path.parent() {
        fs::create_dir_all(parent).map_err(CodexOAuthError::Io)?;
    }

    let json =
        serde_json::to_string_pretty(token).map_err(|e| CodexOAuthError::Parse(e.to_string()))?;
    let tmp_path = store_path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp_path)
        .map_err(CodexOAuthError::Io)?;
    file.write_all(json.as_bytes())
        .map_err(CodexOAuthError::Io)?;
    file.flush().map_err(CodexOAuthError::Io)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))
            .map_err(CodexOAuthError::Io)?;
    }

    fs::rename(&tmp_path, store_path).map_err(CodexOAuthError::Io)
}

// ---------------------------------------------------------------------------
// JWT claim extraction
// ---------------------------------------------------------------------------

/// Extract ChatGPT account/plan/user IDs from an ID-token JWT payload.
fn extract_chatgpt_claims(id_token: &str) -> (Option<String>, Option<String>, Option<String>) {
    let payload = decode_jwt_payload(id_token);
    let auth = payload
        .get("https://api.openai.com/auth")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let account_id = auth
        .get("chatgpt_account_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    let plan_type = auth
        .get("chatgpt_plan_type")
        .and_then(|v| v.as_str())
        .map(String::from);

    let user_id = auth
        .get("chatgpt_user_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    (account_id, plan_type, user_id)
}

fn decode_jwt_payload(token: &str) -> serde_json::Value {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return serde_json::Value::Null;
    }

    let payload = match base64_url_decode(parts[1]) {
        Ok(bytes) => bytes,
        Err(_) => return serde_json::Value::Null,
    };

    serde_json::from_slice(&payload).unwrap_or(serde_json::Value::Null)
}

fn base64_url_decode(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    // Add padding
    let padded = match input.len() % 4 {
        2 => format!("{input}=="),
        3 => format!("{input}="),
        _ => input.to_owned(),
    };
    base64::engine::general_purpose::URL_SAFE.decode(padded.as_bytes())
}

// ---------------------------------------------------------------------------
// HTML response helpers
// ---------------------------------------------------------------------------

fn oauth_success_html(title: &str, message: &str) -> String {
    oauth_result_html(title, "You're signed in", message, "success")
}

fn oauth_error_html(title: &str, message: &str) -> String {
    oauth_result_html(title, "Sign-in failed", message, "error")
}

fn oauth_result_html(title: &str, heading: &str, message: &str, status: &str) -> String {
    let (accent, background, mark) = if status == "success" {
        ("#137333", "#eef7f0", "&#10003;")
    } else {
        ("#b3261e", "#fceeee", "!")
    };

    format!(
        r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
body{{margin:0;min-height:100vh;display:grid;place-items:center;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:#f8faf9;color:#1f2328}}
.panel{{width:min(480px,calc(100vw - 40px));box-sizing:border-box;padding:32px;border:1px solid #d8dee4;border-radius:8px;background:#fff;box-shadow:0 18px 45px rgba(31,35,40,.08)}}
.mark{{width:44px;height:44px;border-radius:50%;display:grid;place-items:center;margin-bottom:20px;font-weight:700;font-size:22px}}
h1{{font-size:24px;line-height:1.2;margin:0 0 10px}}
p{{font-size:15px;line-height:1.5;margin:0;color:#57606a}}
@media (prefers-color-scheme:dark){{body{{background:#0d1117;color:#e6edf3}}.panel{{background:#161b22;border-color:#30363d;box-shadow:0 18px 45px rgba(0,0,0,.4)}}p{{color:#9da7b3}}}}
</style></head><body>
<main class="panel">
<div class="mark" style="background:{background};color:{accent}">{mark}</div>
<h1>{heading}</h1><p>{message}</p>
</main>
</body></html>"#,
    )
}

#[cfg(test)]
mod tests {
    use super::{extract_chatgpt_claims, trim_body, urldecode, CodexToken};
    use serde_json::json;

    #[test]
    fn token_response_uses_refresh_fallback_and_claims() {
        let payload = json!({
            "access_token": "access-token",
            "expires_in": 3600,
            "id_token": jwt_with_chatgpt_claims("acct_123", "plus", "user_456"),
        });

        let token = CodexToken::from_response(&payload, Some("refresh-token"))
            .expect("token from response");

        assert_eq!(token.access_token, "access-token");
        assert_eq!(token.refresh_token, "refresh-token");
        assert_eq!(token.account_id.as_deref(), Some("acct_123"));
        assert_eq!(token.plan_type.as_deref(), Some("plus"));
        assert_eq!(token.user_id.as_deref(), Some("user_456"));
        assert!(!token.is_expired());
    }

    #[test]
    fn token_response_rejects_missing_refresh_without_fallback() {
        let payload = json!({
            "access_token": "access-token",
            "expires_in": 3600,
        });

        let error = CodexToken::from_response(&payload, None).expect_err("missing refresh token");

        assert!(error.to_string().contains("refresh_token"));
    }

    #[test]
    fn extracts_chatgpt_claims_from_id_token() {
        let id_token = jwt_with_chatgpt_claims("acct_abc", "pro", "user_def");

        let (account_id, plan_type, user_id) = extract_chatgpt_claims(&id_token);

        assert_eq!(account_id.as_deref(), Some("acct_abc"));
        assert_eq!(plan_type.as_deref(), Some("pro"));
        assert_eq!(user_id.as_deref(), Some("user_def"));
    }

    #[test]
    fn decodes_callback_query_components() {
        assert_eq!(urldecode("hello+world%21"), "hello world!");
        assert_eq!(urldecode("a%2Fb%3Fc%3Dd"), "a/b?c=d");
    }

    #[test]
    fn trims_long_error_bodies() {
        let body = "x".repeat(600);

        let trimmed = trim_body(&body);

        assert_eq!(trimmed.chars().count(), 503);
        assert!(trimmed.ends_with("..."));
    }

    fn jwt_with_chatgpt_claims(account_id: &str, plan_type: &str, user_id: &str) -> String {
        use base64::Engine;

        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("{}");
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            json!({
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": account_id,
                    "chatgpt_plan_type": plan_type,
                    "chatgpt_user_id": user_id,
                }
            })
            .to_string(),
        );

        format!("{header}.{payload}.signature")
    }
}
