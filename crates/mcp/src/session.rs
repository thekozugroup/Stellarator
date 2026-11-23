//! A run's session with one MCP server, shared by all of the run's samples. A call whose
//! connection failed reconnects once and tries again; when that fails too, the call fails (the
//! sample fails, and Retry failed tries it again later).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value};

use crate::client::{CallOutcome, Connection, McpError, ServerConfig};

pub struct Session {
    pub config: ServerConfig,
    connect_timeout: Duration,
    conn: parking_lot::Mutex<Option<Arc<Connection>>>,
    /// Bumped by every reconnect, so concurrent callers that saw the same failure reconnect once.
    generation: AtomicU64,
    reconnecting: tokio::sync::Mutex<()>,
    closed: AtomicBool,
}

/// A finished call: the text for the model and whether it is an error.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub text: String,
    pub is_error: bool,
}

impl Session {
    /// Connect (start the process, or open the HTTP session) and initialize.
    pub async fn open(config: ServerConfig, connect_timeout: Duration) -> Result<Arc<Session>, McpError> {
        let conn = Connection::connect(&config, connect_timeout).await?;
        Ok(Arc::new(Session {
            config,
            connect_timeout,
            conn: parking_lot::Mutex::new(Some(Arc::new(conn))),
            generation: AtomicU64::new(0),
            reconnecting: tokio::sync::Mutex::new(()),
            closed: AtomicBool::new(false),
        }))
    }

    fn current(&self) -> (Option<Arc<Connection>>, u64) {
        let conn = self.conn.lock().clone();
        (conn, self.generation.load(Ordering::SeqCst))
    }

    /// A new connection, unless another caller already replaced the one that failed (`seen`).
    async fn reconnect(&self, seen: u64) -> Result<Arc<Connection>, McpError> {
        let _g = self.reconnecting.lock().await;
        let (conn, generation) = self.current();
        if generation != seen {
            if let Some(c) = conn {
                return Ok(c);
            }
        }
        tracing::info!(server = %self.config.name, "reconnecting to an MCP server");
        let fresh = Arc::new(Connection::connect(&self.config, self.connect_timeout).await?);
        let old = self.conn.lock().replace(fresh.clone());
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(old) = old {
            tokio::spawn(async move { old.close().await });
        }
        Ok(fresh)
    }

    /// Call `tool` on the server. `Err` only when the connection failed, a reconnect was tried, and
    /// the call failed again; the message names the server.
    pub async fn call(&self, tool: &str, arguments: Option<Map<String, Value>>, timeout: Duration) -> Result<Answer, String> {
        let (conn, seen) = self.current();
        let first_error = match conn {
            Some(c) => match c.call(tool, arguments.clone(), timeout).await {
                CallOutcome::Answered { text, is_error } => return Ok(Answer { text, is_error }),
                CallOutcome::Transport(e) => e,
            },
            None => "the session was closed".to_string(),
        };
        let name = &self.config.name;
        if self.closed.load(Ordering::SeqCst) {
            return Err(format!("The session with the MCP server \"{name}\" was closed."));
        }
        let conn = self.reconnect(seen).await.map_err(|e| {
            format!(
                "The MCP server \"{name}\" stopped answering ({}), and reconnecting failed: {e}",
                first_error.trim_end_matches('.')
            )
        })?;
        match conn.call(tool, arguments, timeout).await {
            CallOutcome::Answered { text, is_error } => Ok(Answer { text, is_error }),
            CallOutcome::Transport(e) => Err(format!(
                "The MCP server \"{name}\" stopped answering, also after a reconnect: {}",
                e.trim_end_matches('.').to_string() + "."
            )),
        }
    }

    /// End the session (stops a stdio server's process).
    pub async fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let _g = self.reconnecting.lock().await;
        let conn = self.conn.lock().take();
        if let Some(c) = conn {
            c.close().await;
        }
    }
}
