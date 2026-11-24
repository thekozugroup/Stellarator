//! Code mode as Cloudflare's `@cloudflare/codemode` 0.5.6 defines it (cloudflare/agents, commit
//! 4116a8bdf97660c8e79a923b6aec140256282a5d): the model gets one tool whose input is
//! `{"code": string}`; the code is a JavaScript async arrow function that calls the run's MCP tools
//! through typed namespaces; it runs in a sandbox, and only its result goes back to the model.
//!
//! - [`code_tool`]: the code tool's definition (`createCodeTool`'s description with the types
//!   `generateTypes` writes) and the namespaces' methods mapped to MCP tools
//! - [`execute`]: one execution in a fresh QuickJS context, with Cloudflare's executor harness,
//!   rendered as the AI SDK sends it to the model
//! - [`normalize_code`], [`sanitize_tool_name`]: Cloudflare's helpers, run as published
//! - [`unwrap_mcp_result`]: what the code gets from an MCP tool call
//!
//! Cloudflare's JavaScript (and acorn, which `normalizeCode` uses) runs as published in QuickJS
//! (js/cloudflare-codemode.js, js/acorn.js; see js/NOTICE.md), so the formats come from their
//! code, not from a rewrite.

mod mcp;
mod sandbox;
mod util;

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};
use stellarator_api::{
    CodeMethod, CodeModeOptions, CodeNamespace, CodeNamespaces, CodeToolSnapshot, McpTool, McpTransport,
    ToolSnapshotServer, ToolSnapshotTool, CODE_MODE_FORMAT,
};

pub use mcp::unwrap_mcp_result;
pub use sandbox::{execute, CallAnswer, Execution, Host, Limits, LATE_CALL_MESSAGE, MAX_CALLS};
pub use util::{invalid_input, normalize_code, sanitize_tool_name, truncate_result};

/// The code tool's input schema, as the AI SDK sends `z.object({ code: z.string().describe(...) })`.
pub fn code_parameters() -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"code": {"type": "string", "description": "JavaScript async arrow function to execute"}},
        "required": ["code"],
        "additionalProperties": false
    })
}

/// What the AI SDK sends the model when it calls a tool that is not offered.
pub fn unknown_tool_message(name: &str, code_tool: &str) -> String {
    format!("AI_NoSuchToolError: Model tried to call unavailable tool '{name}'. Available tools: {code_tool}.")
}

/// One server picked for a run, with the tools picked from it in the server's order.
#[derive(Clone, Debug)]
pub struct SelectedServer {
    pub server_id: i64,
    pub name: String,
    pub transport: McpTransport,
    pub tools: Vec<McpTool>,
}

/// The code tool of a run.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeTool {
    /// The request's one tool definition (OpenAI function format).
    pub definition: Value,
    pub snapshot: CodeToolSnapshot,
    /// The servers with their tools named as the code calls them (`<namespace>.<method>`).
    pub servers: Vec<ToolSnapshotServer>,
}

/// Names the sandbox's harness itself uses (Cloudflare's executor refuses provider names among
/// its reserved ones), plus the globals the namespace proxies need.
const RESERVED: &[&str] = &[
    "__dispatchers", "__connectors", "__logs", "__CODEMODE_BINARY_TAG", "__bytesToBase64", "__base64ToBytes",
    "__encodeCodemodeValue", "__decodeCodemodeValue", "__stringifyForCodemode", "__parseForCodemode",
    "WorkerEntrypoint", "CodeExecutor", "Promise", "setTimeout", "Error", "console", "Object", "Proxy", "String",
    "JSON",
];

struct Entry<'a> {
    key: String,
    server: &'a SelectedServer,
    tool: &'a McpTool,
}

/// The code tool for `servers` with `options`. Err says which names cannot be used.
pub fn code_tool(servers: &[SelectedServer], options: &CodeModeOptions) -> Result<CodeTool, String> {
    // The namespaces and their tools, keyed as the tool records Cloudflare's generateTypes reads.
    let mut groups: Vec<(String, Vec<Entry>)> = Vec::new();
    match options.namespaces {
        CodeNamespaces::Servers => {
            let mut seen: HashMap<String, &str> = HashMap::new();
            for s in servers {
                let ns = sanitize_tool_name(&s.name);
                if RESERVED.contains(&ns.as_str()) {
                    return Err(format!(
                        "The server \"{}\" can't be a namespace in code mode: `{ns}` is a name the sandbox needs. Rename \
                         the server, or put every tool under codemode.",
                        s.name
                    ));
                }
                if let Some(other) = seen.insert(ns.clone(), &s.name) {
                    return Err(format!(
                        "The servers \"{other}\" and \"{}\" would both be the namespace `{ns}` in code mode. Rename one, \
                         or put every tool under codemode.",
                        s.name
                    ));
                }
                let entries = s.tools.iter().map(|t| Entry { key: t.name.clone(), server: s, tool: t }).collect();
                groups.push((ns, entries));
            }
        }
        CodeNamespaces::Codemode => {
            // The direct names: a tool's own name, `<server>__<tool>` when two servers have it.
            let mut servers_with: HashMap<&str, usize> = HashMap::new();
            for s in servers {
                let unique: HashSet<&str> = s.tools.iter().map(|t| t.name.as_str()).collect();
                for n in unique {
                    *servers_with.entry(n).or_default() += 1;
                }
            }
            let mut entries = Vec::new();
            let mut keys = HashSet::new();
            for s in servers {
                for t in &s.tools {
                    let key = if servers_with.get(t.name.as_str()).copied().unwrap_or(0) > 1 {
                        format!("{}__{}", s.name, t.name)
                    } else {
                        t.name.clone()
                    };
                    if !keys.insert(key.clone()) {
                        return Err(format!("Two tools would both be named \"{key}\" in code mode. Leave one out."));
                    }
                    entries.push(Entry { key, server: s, tool: t });
                }
            }
            groups.push(("codemode".to_string(), entries));
        }
    }
    if groups.iter().all(|(_, e)| e.is_empty()) {
        return Err("The selected servers offer no tools. Check them, or pick other servers.".into());
    }
    let spec = json!({
        "description": options.description,
        "namespaces": groups.iter().map(|(ns, entries)| json!({
            "name": ns,
            "tools": entries.iter().map(|e| {
                let mut d = serde_json::Map::new();
                if let Some(desc) = &e.tool.description {
                    d.insert("description".into(), json!(desc));
                }
                d.insert("inputSchema".into(), e.tool.input_schema.clone());
                if let Some(out) = &e.tool.output_schema {
                    d.insert("outputSchema".into(), out.clone());
                }
                json!([e.key, Value::Object(d)])
            }).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    let built: Value = serde_json::from_str(&util::code_tool_json(spec.to_string())?).map_err(|e| e.to_string())?;
    let description = built["description"].as_str().unwrap_or_default().to_string();
    let types = built["types"].as_str().unwrap_or_default().to_string();
    let mut namespaces = Vec::new();
    let mut by_server: HashMap<i64, Vec<ToolSnapshotTool>> = HashMap::new();
    let mut problems = Vec::new();
    for ((ns, entries), out) in groups.iter().zip(built["namespaces"].as_array().into_iter().flatten()) {
        let by_key: HashMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
        let mut methods = Vec::new();
        let mut names: HashMap<String, String> = HashMap::new();
        for m in out["methods"].as_array().into_iter().flatten() {
            let key = m["key"].as_str().unwrap_or_default();
            let name = m["name"].as_str().unwrap_or_default().to_string();
            let Some(e) = by_key.get(key) else { continue };
            if let Some(other) = names.insert(name.clone(), key.to_string()) {
                problems.push(format!("\"{other}\" and \"{key}\" both become `{ns}.{name}`"));
                continue;
            }
            by_server
                .entry(e.server.server_id)
                .or_default()
                .push(ToolSnapshotTool { name: format!("{ns}.{name}"), server_tool: e.tool.name.clone() });
            methods.push(CodeMethod { name, server_id: e.server.server_id, server: e.server.name.clone(), tool: e.tool.name.clone() });
        }
        namespaces.push(CodeNamespace { name: ns.clone(), methods });
    }
    if !problems.is_empty() {
        return Err(format!(
            "These tools would have the same method name in code mode: {}. Leave one of each out.",
            problems.join("; ")
        ));
    }
    let snapshot_servers = servers
        .iter()
        .map(|s| ToolSnapshotServer {
            server_id: s.server_id,
            name: s.name.clone(),
            transport: s.transport,
            tools: by_server.remove(&s.server_id).unwrap_or_default(),
        })
        .collect();
    let definition = json!({"type": "function", "function": {
        "name": options.tool_name,
        "description": description,
        "parameters": code_parameters(),
    }});
    Ok(CodeTool {
        definition,
        snapshot: CodeToolSnapshot {
            tool_name: options.tool_name.clone(),
            types,
            namespaces,
            format: CODE_MODE_FORMAT.to_string(),
        },
        servers: snapshot_servers,
    })
}
