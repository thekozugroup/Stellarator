//! One connection to an MCP server over the official Rust SDK (rmcp): a child process speaking on
//! stdin and stdout, or streamable HTTP. Connect (which initializes the session), list the tools,
//! call one, close.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation};
use rmcp::service::{Peer, RunningService, ServiceError};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Map, Value};
use stellarator_core::schemas::{McpTool, McpTransport};
use tokio::io::AsyncBufReadExt;

use crate::result::{one_line, result_text};

/// The most of a stdio server's error output kept for error messages.
const STDERR_KEEP: usize = 4000;
/// Headers the streamable HTTP transport sets itself.
const RESERVED_HEADERS: [&str; 4] = ["accept", "mcp-session-id", "mcp-protocol-version", "last-event-id"];

/// How to reach one MCP server, secrets included.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ServerConfig {
    pub id: i64,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,
}

/// What went wrong, as one sentence a person can act on.
#[derive(Clone, Debug, PartialEq)]
pub struct McpError(pub String);

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for McpError {}

/// The result of one tool call.
#[derive(Clone, Debug, PartialEq)]
pub enum CallOutcome {
    /// The server answered: the tool's text (or its error, or a protocol error from the server).
    /// The model gets it either way.
    Answered { text: String, is_error: bool },
    /// The connection failed (the server is down, crashed or closed the session). Worth one
    /// reconnect.
    Transport(String),
}

/// An open, initialized MCP session.
pub struct Connection {
    name: String,
    peer: Peer<RoleClient>,
    service: tokio::sync::Mutex<Option<RunningService<RoleClient, ClientConfig>>>,
    stderr: Arc<Mutex<String>>,
    /// "name version" from the server's initialize answer.
    pub server_info: Option<String>,
}

fn client_config() -> ClientConfig {
    ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new("stellarator", env!("CARGO_PKG_VERSION")),
    )
}

/// The last characters of `s`, trimmed, on one line.
fn tail(s: &str, n: usize) -> String {
    let s = one_line(s);
    let count = s.chars().count();
    if count <= n {
        return s;
    }
    let cut: String = s.chars().skip(count - n).collect();
    format!("…{cut}")
}

/// "x." from "x", for sentences built from error texts.
fn period(s: &str) -> String {
    let s = s.trim().trim_end_matches('.');
    format!("{s}.")
}

impl Connection {
    /// Start (stdio) or reach (http) the server and initialize the session, within `timeout`.
    pub async fn connect(config: &ServerConfig, timeout: Duration) -> Result<Connection, McpError> {
        match config.transport {
            McpTransport::Stdio => Self::connect_stdio(config, timeout).await,
            McpTransport::Http => Self::connect_http(config, timeout).await,
        }
    }

    async fn connect_stdio(config: &ServerConfig, timeout: Duration) -> Result<Connection, McpError> {
        let program = config.command.as_deref().map(str::trim).filter(|c| !c.is_empty());
        let Some(program) = program else {
            return Err(McpError("This server has no command to start.".into()));
        };
        // `which` finds `npx.cmd` and the like on Windows, where Command::new("npx") does not.
        let mut cmd = match rmcp::transport::which_command(program) {
            Ok(cmd) => cmd,
            Err(_) => {
                return Err(McpError(format!(
                    "Couldn't find the command \"{program}\". Install it, or enter its full path."
                )))
            }
        };
        cmd.args(&config.args);
        // Only a short list of Stellarator's own variables reaches the server (as the MCP SDKs do),
        // then the server's `env`: tokens in Stellarator's environment, such as HF_TOKEN, stay out.
        cmd.env_clear();
        cmd.envs(inherited_env(std::env::vars_os()));
        cmd.envs(&config.env);
        cmd.kill_on_drop(true);
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: no console window flashes up from the desktop app.
            cmd.creation_flags(0x0800_0000);
        }
        let (process, stderr_pipe) = TokioChildProcess::builder(cmd)
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| McpError(format!("Couldn't start \"{program}\": {}", period(&e.to_string()))))?;
        let stderr = Arc::new(Mutex::new(String::new()));
        if let Some(pipe) = stderr_pipe {
            let keep = stderr.clone();
            let name = config.name.clone();
            tokio::spawn(async move {
                let mut lines = tokio::io::BufReader::new(pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(server = %name, "{line}");
                    let mut s = keep.lock();
                    s.push_str(&line);
                    s.push('\n');
                    if s.len() > STDERR_KEEP * 2 {
                        let cut = s.len() - STDERR_KEEP;
                        let cut = (cut..s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(s.len());
                        s.drain(..cut);
                    }
                }
            });
        }
        let started = tokio::time::timeout(timeout, client_config().serve(process)).await;
        let service = match started {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                // Give the error output a moment to arrive: it usually says why.
                tokio::time::sleep(Duration::from_millis(150)).await;
                return Err(McpError(with_stderr(
                    &format!("The server did not complete the MCP handshake ({})", one_line(&e.to_string())),
                    &stderr.lock(),
                )));
            }
            Err(_) => {
                return Err(McpError(with_stderr(
                    &format!("The server did not answer within {} s", timeout.as_secs()),
                    &stderr.lock(),
                )))
            }
        };
        Ok(Self::from_service(&config.name, service, stderr))
    }

    async fn connect_http(config: &ServerConfig, timeout: Duration) -> Result<Connection, McpError> {
        let Some(url) = config.url.as_deref().map(str::trim).filter(|u| !u.is_empty()) else {
            return Err(McpError("This server has no URL.".into()));
        };
        let mut headers = std::collections::HashMap::new();
        for (k, v) in &config.headers {
            if RESERVED_HEADERS.iter().any(|r| k.eq_ignore_ascii_case(r)) {
                continue;
            }
            let name = reqwest::header::HeaderName::from_bytes(k.as_bytes())
                .map_err(|_| McpError(format!("\"{k}\" is not a valid HTTP header name.")))?;
            let value = reqwest::header::HeaderValue::from_str(v)
                .map_err(|_| McpError(format!("The value of the header \"{k}\" is not valid in an HTTP header.")))?;
            headers.insert(name, value);
        }
        let client = reqwest::Client::builder()
            .user_agent(concat!("stellarator/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(timeout.min(Duration::from_secs(15)))
            .build()
            .map_err(|e| McpError(format!("Couldn't set up the HTTP client: {e}.")))?;
        let transport = StreamableHttpClientTransport::with_client(
            client,
            StreamableHttpClientTransportConfig::with_uri(url.to_string())
                .custom_headers(headers)
                .reinit_on_expired_session(true),
        );
        let service = match tokio::time::timeout(timeout, client_config().serve(transport)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                return Err(McpError(period(&format!(
                    "Couldn't open an MCP session at {url} ({})",
                    describe_init_error(&e.to_string())
                ))))
            }
            Err(_) => return Err(McpError(format!("{url} did not answer within {} s.", timeout.as_secs()))),
        };
        Ok(Self::from_service(&config.name, service, Arc::new(Mutex::new(String::new()))))
    }

    fn from_service(name: &str, service: RunningService<RoleClient, ClientConfig>, stderr: Arc<Mutex<String>>) -> Self {
        let server_info = service.peer_info().and_then(|info| info.server_info.clone()).map(|i| {
            let n = i.name.trim();
            let v = i.version.trim();
            if v.is_empty() {
                n.to_string()
            } else {
                format!("{n} {v}")
            }
        });
        Connection {
            name: name.to_string(),
            peer: service.peer().clone(),
            service: tokio::sync::Mutex::new(Some(service)),
            stderr,
            server_info: server_info.filter(|s| !s.is_empty()),
        }
    }

    /// Every tool the server offers.
    pub async fn list_tools(&self, timeout: Duration) -> Result<Vec<McpTool>, McpError> {
        let listed = tokio::time::timeout(timeout, self.peer.list_all_tools()).await;
        let tools = match listed {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                return Err(McpError(self.with_stderr(&format!("Listing the tools failed ({})", describe_service_error(&e)))))
            }
            Err(_) => return Err(McpError(format!("Listing the tools took longer than {} s.", timeout.as_secs()))),
        };
        Ok(tools
            .into_iter()
            .map(|t| {
                let schema = Value::Object((*t.input_schema).clone());
                McpTool {
                    name: t.name.to_string(),
                    description: t.description.map(|d| d.to_string()).filter(|d| !d.trim().is_empty()),
                    input_schema: schema,
                }
            })
            .collect())
    }

    /// Call `tool` with `arguments`, waiting at most `timeout`.
    pub async fn call(&self, tool: &str, arguments: Option<Map<String, Value>>, timeout: Duration) -> CallOutcome {
        let mut params = CallToolRequestParams::new(tool.to_string());
        if let Some(args) = arguments {
            params = params.with_arguments(args);
        }
        let started = Instant::now();
        match tokio::time::timeout(timeout, self.peer.call_tool(params)).await {
            Err(_) => CallOutcome::Answered {
                text: format!("The tool did not finish within {} s.", fmt_secs(timeout.as_secs_f64())),
                is_error: true,
            },
            Ok(Ok(result)) => {
                let value = serde_json::to_value(&result).unwrap_or(Value::Null);
                let (text, is_error) = result_text(&value);
                tracing::debug!(server = %self.name, tool, ms = started.elapsed().as_millis() as u64, is_error, "tool call");
                CallOutcome::Answered { text, is_error }
            }
            Ok(Err(ServiceError::McpError(e))) => CallOutcome::Answered {
                text: format!("The server could not run the tool: {}", period(&e.message)),
                is_error: true,
            },
            Ok(Err(e @ (ServiceError::TransportClosed | ServiceError::TransportSend(_)))) => {
                CallOutcome::Transport(self.with_stderr(&describe_service_error(&e)))
            }
            Ok(Err(e)) => CallOutcome::Answered { text: period(&describe_service_error(&e)), is_error: true },
        }
    }

    /// True once the session ended (the process exited, the server closed it, or [`Self::close`]).
    pub fn is_closed(&self) -> bool {
        self.peer.is_transport_closed()
    }

    /// End the session: for stdio, close the pipes and stop the process.
    pub async fn close(&self) {
        let service = self.service.lock().await.take();
        if let Some(mut s) = service {
            let _ = s.close_with_timeout(Duration::from_secs(5)).await;
        }
    }

    fn with_stderr(&self, sentence: &str) -> String {
        with_stderr(sentence, &self.stderr.lock())
    }
}

/// `sentence`, with the end of the server's error output when it wrote any.
fn with_stderr(sentence: &str, stderr: &str) -> String {
    let base = period(sentence);
    if stderr.trim().is_empty() {
        base
    } else {
        format!("{base} Its error output ends: {}", tail(stderr, 300))
    }
}

/// `{:g}`-style seconds: 60 -> "60", 0.5 -> "0.5".
pub fn fmt_secs(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn describe_service_error(e: &ServiceError) -> String {
    match e {
        ServiceError::TransportClosed => "the connection to the server closed".into(),
        ServiceError::TransportSend(err) => format!("the request could not be sent: {}", one_line(&err.to_string())),
        ServiceError::McpError(err) => format!("the server answered with an error: {}", one_line(&err.message)),
        ServiceError::Timeout { .. } => "the server did not answer in time".into(),
        other => one_line(&other.to_string()),
    }
}

/// rmcp's initialize errors carry Debug output of reqwest errors; keep the readable part.
fn describe_init_error(raw: &str) -> String {
    let raw = one_line(raw);
    let lower = raw.to_lowercase();
    if lower.contains("connection refused") || lower.contains("tcp connect error") || lower.contains("error sending request") {
        return "nothing answered; check the URL and that the server is running".into();
    }
    if lower.contains("dns error") || lower.contains("failed to lookup address") {
        return "the host name could not be found".into();
    }
    if lower.contains("401") || lower.contains("unauthorized") || lower.contains("auth required") {
        return "the server asks for authorization; add an Authorization header".into();
    }
    if lower.contains("404") {
        return "nothing was found at this URL (404); MCP endpoints often end in /mcp".into();
    }
    raw.chars().take(300).collect()
}

/// The variables of Stellarator's own environment a stdio server starts with (its `env` is added
/// on top): what commands such as `npx` and `uvx` need to run (the list the MCP SDKs pass), plus
/// locale, temp folder, proxy and certificate settings. Everything else, tokens included, is left
/// out.
const INHERITED: &[&str] = &[
    // Unix
    "HOME", "LOGNAME", "PATH", "SHELL", "TERM", "USER", "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR",
    // Windows (names are matched without regard to case)
    "APPDATA", "HOMEDRIVE", "HOMEPATH", "LOCALAPPDATA", "PROCESSOR_ARCHITECTURE", "PROGRAMFILES",
    "SYSTEMDRIVE", "SYSTEMROOT", "TEMP", "TMP", "USERNAME", "USERPROFILE", "PATHEXT", "COMSPEC",
    // Network
    "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY", "ALL_PROXY", "SSL_CERT_FILE", "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS", "REQUESTS_CA_BUNDLE",
];

/// The entries of `vars` a stdio server inherits (see [`INHERITED`]).
fn inherited_env(
    vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    vars.filter(|(key, _)| {
        key.to_str().is_some_and(|k| INHERITED.iter().any(|allowed| allowed.eq_ignore_ascii_case(k)))
    })
    .collect()
}

#[cfg(test)]
mod env_tests {
    use super::inherited_env;
    use std::ffi::OsString;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))).collect()
    }

    #[test]
    fn a_stdio_server_gets_the_basics_and_no_tokens() {
        let kept = inherited_env(
            vars(&[
                ("PATH", "/usr/bin"),
                ("HOME", "/home/a"),
                ("https_proxy", "http://proxy:3128"),
                ("Path", "C:\\Windows"),
                ("HF_TOKEN", "hf_secret"),
                ("OPENAI_API_KEY", "sk-secret"),
                ("STELLARATOR_DATA_DIR", "/data"),
            ])
            .into_iter(),
        );
        let names: Vec<_> = kept.iter().map(|(k, _)| k.to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["PATH", "HOME", "https_proxy", "Path"]);
    }
}
