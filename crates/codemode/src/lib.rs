//! Code mode as Cloudflare's `@cloudflare/codemode` 0.5.6 defines it (cloudflare/agents, commit
//! 4116a8bdf97660c8e79a923b6aec140256282a5d): the model gets one tool whose input is
//! `{"code": string}`; the code is a JavaScript async arrow function that calls the run's MCP tools
//! through namespaces; it runs in a sandbox, and only its result goes back to the model.
//!
//! Two formats, Cloudflare's two setups ([`CodeFormat`]): `code_tool` is `createCodeTool` (the
//! tools' types in the description, `{result, logs}` back), `runtime` is
//! `createCodemodeRuntime(...).tool()` with one `McpConnector` per server (discovery with
//! `codemode.search` and `codemode.describe` in the sandbox, `{status, executionId, ...}` back).
//!
//! - [`code_tool`]: the code tool's definition and the namespaces' methods mapped to MCP tools
//! - [`execute`]: one execution in a fresh QuickJS context, running the module Cloudflare's
//!   executor generates, rendered as the AI SDK sends it to the model
//! - [`normalize_code`], [`sanitize_tool_name`]: Cloudflare's helpers, run as published
//! - [`unwrap_mcp_result`]: what the code gets from an MCP tool call (the trace's copy)
//! - [`resend_arguments`]: the arguments the AI SDK sends back with the next request
//!
//! Cloudflare's JavaScript (and acorn, which `normalizeCode` uses) runs as published in QuickJS
//! (js/cloudflare-codemode.js, js/cloudflare-runtime.js, js/acorn.js; see js/NOTICE.md), so the
//! formats come from their code, not from a rewrite. tests/golden holds the published package's
//! outputs, and the reference harness that runs it on workerd.

mod mcp;
mod sandbox;
mod url;
mod util;

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};
use stellarator_api::{
    CodeConnector, CodeConnectorTool, CodeFormat, CodeMethod, CodeModeOptions, CodeNamespace, CodeNamespaces, CodeToolSnapshot,
    McpTool, McpTransport, ToolSnapshotServer, ToolSnapshotTool, CODE_MODE_FORMAT,
};

pub use mcp::unwrap_mcp_result;
pub use sandbox::{execute, execution_id, CallAnswer, Execution, Host, Limits, Setup, LATE_CALL_MESSAGE, MAX_CALLS};
pub use util::{invalid_input, json_parse_error, normalize_code, resend_arguments, sanitize_tool_name, truncate_result};

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
    /// The `instructions` the server sent when the session was opened.
    pub instructions: Option<String>,
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

/// What an execution needs from a run's snapshot (see [`execute`]): the format, the tool's name,
/// the namespaces and, for the `runtime` format, the connectors' descriptions.
pub fn setup(snapshot: &CodeToolSnapshot, limits: Limits) -> Setup {
    let descriptions: Vec<Value> = snapshot
        .connectors
        .iter()
        .map(|c| {
            // As `connector.describe()` returns it: {name, instructions?, descriptors, annotations}.
            let mut d = serde_json::Map::new();
            d.insert("name".into(), json!(c.name));
            if let Some(i) = &c.instructions {
                d.insert("instructions".into(), json!(i));
            }
            let mut descriptors = serde_json::Map::new();
            for t in &c.tools {
                let mut one = serde_json::Map::new();
                if let Some(desc) = &t.description {
                    one.insert("description".into(), json!(desc));
                }
                one.insert("inputSchema".into(), t.input_schema.clone());
                if let Some(out) = &t.output_schema {
                    one.insert("outputSchema".into(), out.clone());
                }
                descriptors.insert(t.name.clone(), Value::Object(one));
            }
            d.insert("descriptors".into(), Value::Object(descriptors));
            d.insert("annotations".into(), json!({}));
            Value::Object(d)
        })
        .collect();
    Setup {
        format: snapshot.kind,
        tool_name: snapshot.tool_name.clone(),
        namespaces: snapshot.namespaces.iter().map(|n| n.name.clone()).collect(),
        descriptions: Value::Array(descriptions).to_string().into(),
        limits,
    }
}

/// The code tool for `servers` with `options`, in the format the options name. Err says which
/// names cannot be used.
pub fn code_tool(servers: &[SelectedServer], options: &CodeModeOptions) -> Result<CodeTool, String> {
    match options.format {
        CodeFormat::CodeTool => code_tool_format(servers, options),
        CodeFormat::Runtime => runtime_format(servers, options),
    }
}

fn tool_descriptor(tool: &McpTool) -> Value {
    let mut d = serde_json::Map::new();
    if let Some(desc) = &tool.description {
        d.insert("description".into(), json!(desc));
    }
    d.insert("inputSchema".into(), tool.input_schema.clone());
    if let Some(out) = &tool.output_schema {
        d.insert("outputSchema".into(), out.clone());
    }
    Value::Object(d)
}

/// The `runtime` format: `createCodemodeRuntime({connectors}).tool()` with one `McpConnector` per
/// server. The description names the connectors; their tools are found in the sandbox.
fn runtime_format(servers: &[SelectedServer], options: &CodeModeOptions) -> Result<CodeTool, String> {
    let mut names: Vec<String> = Vec::new();
    for s in servers {
        let ns = sanitize_tool_name(&s.name);
        if ns == "codemode" {
            return Err(format!(
                "The server \"{}\" can't be a connector in the runtime format: `codemode` is the runtime's own namespace. Rename \
                 the server.",
                s.name
            ));
        }
        if RESERVED.contains(&ns.as_str()) {
            return Err(format!(
                "The server \"{}\" can't be a connector in code mode: `{ns}` is a name the sandbox needs. Rename the server.",
                s.name
            ));
        }
        if let Some(other) = servers.iter().zip(&names).find(|(_, n)| **n == ns).map(|(o, _)| &o.name) {
            return Err(format!("The servers \"{other}\" and \"{}\" would both be the connector `{ns}`. Rename one.", s.name));
        }
        names.push(ns);
    }
    if servers.iter().all(|s| s.tools.is_empty()) {
        return Err("The selected servers offer no tools. Check them, or pick other servers.".into());
    }
    // Hints are keyed by the servers' names, as given or as their connectors are named.
    let mut hints = serde_json::Map::new();
    for (key, hint) in &options.connector_hints {
        let ns = sanitize_tool_name(key);
        if !names.contains(&ns) {
            return Err(format!(
                "connector_hints names \"{key}\", which is not one of the run's servers ({}).",
                servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        hints.insert(ns, json!(hint));
    }
    let spec = json!({
        "description": options.description,
        "hints": hints,
        "connectors": servers.iter().zip(&names).map(|(s, ns)| json!({
            "name": ns,
            "instructions": if options.server_instructions { s.instructions.as_deref().filter(|i| !i.is_empty()) } else { None },
            "tools": s.tools.iter().map(|t| json!([t.name, tool_descriptor(t)])).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    let built: Value = serde_json::from_str(&util::runtime_tool_json(spec.to_string())?).map_err(|e| e.to_string())?;
    if let Some(clashes) = built["clashes"].as_array().filter(|c| !c.is_empty()) {
        let list: Vec<&str> = clashes.iter().filter_map(Value::as_str).collect();
        return Err(format!(
            "These tools would have the same method name in code mode: {}. Leave one of each out.",
            list.join("; ")
        ));
    }
    let mut namespaces = Vec::new();
    let mut connectors = Vec::new();
    let mut snapshot_servers = Vec::new();
    for ((s, ns), out) in servers.iter().zip(&names).zip(built["connectors"].as_array().into_iter().flatten()) {
        let by_key: HashMap<&str, &McpTool> = s.tools.iter().map(|t| (t.name.as_str(), t)).collect();
        let (mut methods, mut tools, mut listed) = (Vec::new(), Vec::new(), Vec::new());
        for m in out["methods"].as_array().into_iter().flatten() {
            let (key, name) = (m["key"].as_str().unwrap_or_default(), m["name"].as_str().unwrap_or_default().to_string());
            let Some(tool) = by_key.get(key) else { continue };
            let described = &out["description"]["descriptors"][&name];
            tools.push(CodeConnectorTool {
                name: name.clone(),
                description: tool.description.clone(),
                input_schema: described["inputSchema"].clone(),
                output_schema: tool.output_schema.clone(),
            });
            listed.push(ToolSnapshotTool { name: format!("{ns}.{name}"), server_tool: tool.name.clone() });
            methods.push(CodeMethod { name, server_id: s.server_id, server: s.name.clone(), tool: tool.name.clone() });
        }
        namespaces.push(CodeNamespace { name: ns.clone(), methods });
        connectors.push(CodeConnector {
            name: ns.clone(),
            instructions: out["description"]["instructions"].as_str().map(str::to_string),
            hint: hints.get(ns).and_then(Value::as_str).map(str::to_string),
            tools,
        });
        snapshot_servers.push(ToolSnapshotServer { server_id: s.server_id, name: s.name.clone(), transport: s.transport, tools: listed });
    }
    let definition = json!({"type": "function", "function": {
        "name": options.tool_name,
        "description": built["description"],
        "parameters": built["parameters"],
    }});
    Ok(CodeTool {
        definition,
        snapshot: CodeToolSnapshot {
            tool_name: options.tool_name.clone(),
            types: built["types"].as_str().unwrap_or_default().to_string(),
            namespaces,
            format: CODE_MODE_FORMAT.to_string(),
            kind: CodeFormat::Runtime,
            connectors,
        },
        servers: snapshot_servers,
    })
}

/// The `code_tool` format: `createCodeTool`, the tools' types in the description.
fn code_tool_format(servers: &[SelectedServer], options: &CodeModeOptions) -> Result<CodeTool, String> {
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
            "tools": entries.iter().map(|e| json!([e.key, tool_descriptor(e.tool)])).collect::<Vec<_>>(),
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
            ..CodeToolSnapshot::default()
        },
        servers: snapshot_servers,
    })
}
