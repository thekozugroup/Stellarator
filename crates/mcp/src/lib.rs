//! Stellarator's MCP side: connecting to MCP servers with the official Rust SDK (rmcp), over a
//! child process's stdin and stdout or streamable HTTP; listing and calling their tools; the
//! sessions a run shares between its samples; reading pasted server configurations; and the tool
//! names and definitions a run sends.
//!
//! - [`client`]: one connection ([`Connection`]): connect, list tools, call, close
//! - [`session`]: a run's shared session per server, which reconnects once when a call's
//!   connection fails
//! - [`import`]: pasted JSON in the shapes Claude Desktop, Cursor, Windsurf and VS Code use
//! - [`names`]: tool names (`<server>__<tool>` on a clash) and OpenAI function definitions
//! - [`result`]: a tool result as text for the model, cut to the run's limit
//! - [`store`]: the `mcp_servers` table

pub mod client;
pub mod import;
pub mod names;
pub mod result;
pub mod session;
pub mod store;

pub use client::{CallOutcome, Connection, McpError, RawOutcome, ServerConfig};
pub use session::{Answer, Session};
