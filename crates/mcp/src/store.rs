//! The `mcp_servers` table: rows, their API shape (secrets as hints), and checks.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::NaiveDateTime;
use sqlx::types::Json;
use sqlx::SqlitePool;
use stellarator_core::config::STDIO_MCP_BLOCKED;
use stellarator_core::schemas::{McpServerOut, McpStatus, McpTool, McpTransport, SecretHint};
use stellarator_core::settings::key_hint;
use stellarator_core::time::{now_db, utc, utc_opt};

use crate::client::{Connection, McpError, ServerConfig};

/// How long a check (connect, initialize, list the tools) may take. Stdio servers started through
/// `npx` or `uvx` may download their package first.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(sqlx::FromRow, Clone, Debug)]
pub struct ServerRow {
    pub id: i64,
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Json<Vec<String>>,
    pub env: Json<BTreeMap<String, String>>,
    pub url: Option<String>,
    pub headers: Json<BTreeMap<String, String>>,
    pub enabled: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub checked_at: Option<NaiveDateTime>,
    pub server_info: Option<String>,
    pub tools: Json<Vec<McpTool>>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub run_count: i64,
}

/// Every column, and the number of runs whose tool snapshot names the server.
pub const SELECT: &str = "SELECT m.*, (SELECT COUNT(*) FROM runs r WHERE r.tools IS NOT NULL AND EXISTS \
    (SELECT 1 FROM json_each(r.tools, '$.servers') s WHERE json_extract(s.value, '$.server_id') = m.id)) AS run_count \
    FROM mcp_servers m";

fn hints(map: &BTreeMap<String, String>) -> Vec<SecretHint> {
    map.iter().map(|(k, v)| SecretHint { name: k.clone(), hint: key_hint(Some(v)) }).collect()
}

impl ServerRow {
    pub fn transport(&self) -> McpTransport {
        McpTransport::parse(&self.transport)
    }

    pub fn config(&self) -> ServerConfig {
        ServerConfig {
            id: self.id,
            name: self.name.clone(),
            transport: self.transport(),
            command: self.command.clone(),
            args: self.args.0.clone(),
            env: self.env.0.clone(),
            url: self.url.clone(),
            headers: self.headers.0.clone(),
        }
    }

    /// The API shape. `allow_stdio`: whether stdio servers may run here.
    pub fn out(self, allow_stdio: bool) -> McpServerOut {
        let transport = self.transport();
        McpServerOut {
            id: self.id,
            blocked: (transport == McpTransport::Stdio && !allow_stdio).then(|| STDIO_MCP_BLOCKED.to_string()),
            name: self.name,
            transport,
            command: self.command,
            args: self.args.0,
            env: hints(&self.env.0),
            url: self.url,
            headers: hints(&self.headers.0),
            enabled: self.enabled,
            status: McpStatus::parse(&self.status),
            error: self.last_error,
            checked_at: utc_opt(self.checked_at),
            server_info: self.server_info,
            tools: self.tools.0,
            run_count: self.run_count,
            created_at: utc(self.created_at),
            updated_at: utc(self.updated_at),
        }
    }
}

pub async fn fetch(db: &SqlitePool, id: i64) -> sqlx::Result<Option<ServerRow>> {
    sqlx::query_as::<_, ServerRow>(&format!("{SELECT} WHERE m.id = ?")).bind(id).fetch_optional(db).await
}

pub async fn fetch_all(db: &SqlitePool) -> sqlx::Result<Vec<ServerRow>> {
    sqlx::query_as::<_, ServerRow>(&format!("{SELECT} ORDER BY m.name COLLATE NOCASE, m.id")).fetch_all(db).await
}

/// Every saved name, for clash checks.
pub async fn names(db: &SqlitePool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT name FROM mcp_servers").fetch_all(db).await
}

/// What a check found.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub server_info: Option<String>,
    pub tools: Vec<McpTool>,
}

/// Connect, initialize, list the tools, close. Bounded by `timeout` overall.
pub async fn check(config: &ServerConfig, timeout: Duration) -> Result<Checked, McpError> {
    let run = async {
        let conn = Connection::connect(config, timeout).await?;
        let tools = conn.list_tools(timeout).await;
        let server_info = conn.server_info.clone();
        conn.close().await;
        Ok(Checked { server_info, tools: tools? })
    };
    match tokio::time::timeout(timeout + Duration::from_secs(5), run).await {
        Ok(r) => r,
        Err(_) => Err(McpError(format!("The server did not answer within {} s.", timeout.as_secs()))),
    }
}

/// Store a check's outcome. A failed check keeps the tools found before.
pub async fn save_check(db: &SqlitePool, id: i64, outcome: &Result<Checked, McpError>) -> sqlx::Result<()> {
    let now = now_db();
    match outcome {
        Ok(c) => {
            sqlx::query(
                "UPDATE mcp_servers SET status = 'ok', last_error = NULL, checked_at = ?, server_info = ?, tools = ? \
                 WHERE id = ?",
            )
            .bind(&now)
            .bind(&c.server_info)
            .bind(Json(&c.tools))
            .bind(id)
            .execute(db)
            .await?;
        }
        Err(e) => {
            sqlx::query("UPDATE mcp_servers SET status = 'error', last_error = ?, checked_at = ? WHERE id = ?")
                .bind(&e.0)
                .bind(&now)
                .bind(id)
                .execute(db)
                .await?;
        }
    }
    Ok(())
}
