//! The tool names a run sends. Each tool keeps its own name; when two selected servers have a tool
//! of the same name, both become `<server>__<tool>`. Every name must match
//! `^[a-zA-Z0-9_-]{1,64}$` (OpenAI's rule for function names, which most servers apply), so a run
//! whose names cannot be made to fit is refused.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};
use stellarator_core::schemas::{McpTool, McpTransport, ToolMode, ToolSnapshot, ToolSnapshotServer, ToolSnapshotTool};

/// One selected server and the tools picked from it, in the server's order.
#[derive(Clone, Debug)]
pub struct Selected {
    pub server_id: i64,
    pub name: String,
    pub transport: McpTransport,
    pub tools: Vec<McpTool>,
}

/// True when `name` can be sent as a function name.
pub fn valid_tool_name(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The function's parameters: the tool's input schema, or an empty object schema.
fn parameters(schema: &Value) -> Value {
    match schema {
        Value::Object(o) if !o.is_empty() => {
            let mut o = o.clone();
            o.entry("type").or_insert_with(|| json!("object"));
            if o.get("type") == Some(&json!("object")) {
                o.entry("properties").or_insert_with(|| json!({}));
            }
            Value::Object(o)
        }
        _ => json!({"type": "object", "properties": {}}),
    }
}

/// An OpenAI function definition for `tool`, sent as `name`.
pub fn definition(name: &str, tool: &McpTool) -> Value {
    let mut f = Map::new();
    f.insert("name".into(), json!(name));
    if let Some(d) = tool.description.as_deref().filter(|d| !d.trim().is_empty()) {
        f.insert("description".into(), json!(d));
    }
    f.insert("parameters".into(), parameters(&tool.input_schema));
    json!({"type": "function", "function": Value::Object(f)})
}

/// The snapshot a run sends: names resolved, definitions built. Err lists the names that cannot be
/// sent.
pub fn snapshot(selected: &[Selected]) -> Result<ToolSnapshot, String> {
    let mut servers_with: HashMap<&str, usize> = HashMap::new();
    for s in selected {
        let unique: HashSet<&str> = s.tools.iter().map(|t| t.name.as_str()).collect();
        for n in unique {
            *servers_with.entry(n).or_default() += 1;
        }
    }
    let mut servers = Vec::new();
    let mut definitions = Vec::new();
    let mut bad: Vec<String> = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();
    for s in selected {
        let mut tools = Vec::new();
        for t in &s.tools {
            let name = if servers_with.get(t.name.as_str()).copied().unwrap_or(0) > 1 {
                format!("{}__{}", s.name, t.name)
            } else {
                t.name.clone()
            };
            if !valid_tool_name(&name) {
                bad.push(format!("\"{name}\" ({})", s.name));
                continue;
            }
            if let Some(other) = seen.insert(name.clone(), s.name.clone()) {
                bad.push(format!("\"{name}\" (both {other} and {})", s.name));
                continue;
            }
            definitions.push(definition(&name, t));
            tools.push(ToolSnapshotTool { name, server_tool: t.name.clone() });
        }
        servers.push(ToolSnapshotServer {
            server_id: s.server_id,
            name: s.name.clone(),
            transport: s.transport,
            tools,
        });
    }
    if !bad.is_empty() {
        return Err(format!(
            "These tool names can't be sent: {}. Tool names must be 1 to 64 letters, digits, _ or -, and different \
             from each other. Leave these tools out, or rename the server.",
            bad.join(", ")
        ));
    }
    if definitions.is_empty() {
        return Err("The selected servers offer no tools. Check them, or pick other servers.".into());
    }
    Ok(ToolSnapshot { servers, definitions, mode: ToolMode::Direct, code: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> McpTool {
        McpTool { name: name.into(), description: Some(format!("{name} things")), input_schema: json!({"type": "object", "properties": {"q": {"type": "string"}}}), output_schema: None }
    }

    fn sel(id: i64, name: &str, tools: &[&str]) -> Selected {
        Selected { server_id: id, name: name.into(), transport: McpTransport::Http, tools: tools.iter().map(|t| tool(t)).collect() }
    }

    #[test]
    fn own_names_unless_two_servers_clash() {
        let snap = snapshot(&[sel(1, "github", &["search", "issues"]), sel(2, "docs", &["search", "read"])]).unwrap();
        let names: Vec<&str> = snap.servers.iter().flat_map(|s| s.tools.iter().map(|t| t.name.as_str())).collect();
        assert_eq!(names, ["github__search", "issues", "docs__search", "read"]);
        assert_eq!(snap.servers[0].tools[0].server_tool, "search");
        assert_eq!(snap.definitions.len(), 4);
        assert_eq!(
            snap.definitions[1],
            json!({"type": "function", "function": {"name": "issues", "description": "issues things",
                   "parameters": {"type": "object", "properties": {"q": {"type": "string"}}}}})
        );
    }

    #[test]
    fn names_that_cannot_be_sent_are_refused() {
        let err = snapshot(&[sel(1, "a", &["has space", "ok"])]).unwrap_err();
        assert!(err.contains("\"has space\" (a)"), "{err}");
        let long = "x".repeat(60);
        let err = snapshot(&[sel(1, "server-with-a-long-name", &[&long]), sel(2, "b", &[&long])]).unwrap_err();
        assert!(err.contains("server-with-a-long-name__"), "{err}");
        // A prefixed name equal to another server's own tool name.
        let err = snapshot(&[sel(1, "a", &["b__c", "c"]), sel(2, "b", &["c"])]).unwrap_err();
        assert!(err.contains("\"b__c\""), "{err}");
        assert!(snapshot(&[sel(1, "a", &[])]).unwrap_err().contains("no tools"));
        assert!(valid_tool_name("a-b_C9") && !valid_tool_name("") && !valid_tool_name("a.b") && !valid_tool_name(&"a".repeat(65)));
    }

    #[test]
    fn parameters_get_an_object_schema() {
        let t = McpTool { name: "t".into(), description: None, input_schema: json!({}), output_schema: None };
        assert_eq!(definition("t", &t), json!({"type": "function", "function": {"name": "t", "parameters": {"type": "object", "properties": {}}}}));
        let t = McpTool { name: "t".into(), description: Some(" ".into()), input_schema: json!({"properties": {"a": {"type": "integer"}}}), output_schema: None };
        assert_eq!(definition("t", &t)["function"]["parameters"], json!({"type": "object", "properties": {"a": {"type": "integer"}}}));
    }
}
