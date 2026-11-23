//! Reading pasted MCP server configurations in the shapes other tools use:
//!
//! - Claude Desktop, Claude Code, Cursor, Windsurf, Cline: `{"mcpServers": {"name": {...}}}`
//! - VS Code: `{"servers": {"name": {...}}}`, also inside settings.json as `{"mcp": {"servers": ...}}`
//! - the inner map alone: `{"name": {...}, ...}` (also without its outer braces)
//! - one server object: `{"command": ...}` or `{"url": ...}`, named by its `name` field if it has one
//! - a list of server objects with `name` fields
//!
//! Each server object may have `type` (or `transport`): `stdio`, `http`, `streamable-http`,
//! `streamableHttp` or `sse`; without it, `command` means stdio and `url` (or Windsurf's
//! `serverUrl`) means HTTP. `command`, `args`, `env`, `url` and `headers` are read; `disabled: true`
//! saves the server turned off. Comments and trailing commas (JSONC, as in VS Code) are allowed.

use std::collections::{BTreeMap, HashSet};

use serde_json::{Map, Value};
use stellarator_core::schemas::{McpServerIn, McpTransport};

/// One pasted server, ready to validate and save.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The name in the pasted JSON ("" for a single unnamed object).
    pub entry: String,
    pub server: McpServerIn,
    /// What changed on the way in (a new name, SSE saved as HTTP, placeholders to replace).
    pub notes: Vec<String>,
}

/// What a paste holds: the servers that could be read, and an error for each that could not.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed {
    pub entries: Vec<Entry>,
    pub errors: Vec<(String, String)>,
}

/// Remove `//` and `/* */` comments and trailing commas outside strings.
pub fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
                i += 1;
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
            }
            ',' => {
                // A comma followed only by whitespace and comments before } or ] is dropped.
                let mut j = i + 1;
                loop {
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    if chars.get(j) == Some(&'/') && chars.get(j + 1) == Some(&'/') {
                        while j < chars.len() && chars[j] != '\n' {
                            j += 1;
                        }
                    } else if chars.get(j) == Some(&'/') && chars.get(j + 1) == Some(&'*') {
                        j += 2;
                        while j < chars.len() && !(chars[j] == '*' && chars.get(j + 1) == Some(&'/')) {
                            j += 1;
                        }
                        j += 2;
                    } else {
                        break;
                    }
                }
                if !matches!(chars.get(j), Some('}') | Some(']')) {
                    out.push(c);
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Parse pasted text into JSON: as is, without comments, or wrapped in braces (a fragment such as
/// `"github": {...}` copied from inside a larger file).
pub fn parse_text(text: &str) -> Result<Value, String> {
    let cleaned = strip_jsonc(text);
    let trimmed = cleaned.trim().trim_end_matches(',');
    match serde_json::from_str::<Value>(trimmed) {
        Ok(v) => Ok(v),
        Err(first) => {
            if trimmed.starts_with('"') {
                if let Ok(v) = serde_json::from_str::<Value>(&format!("{{{trimmed}}}")) {
                    return Ok(v);
                }
            }
            Err(format!(
                "The pasted text is not valid JSON (line {}, column {}): {}.",
                first.line(),
                first.column(),
                first.to_string().split(" at line ").next().unwrap_or("")
            ))
        }
    }
}

fn looks_like_server(v: &Value) -> bool {
    v.as_object().is_some_and(|o| {
        ["command", "url", "serverUrl", "type", "transport"].iter().any(|k| o.contains_key(*k))
    })
}

/// The (entry name, server object) pairs in a parsed paste.
fn entries_of(v: &Value) -> Result<Vec<(String, Value)>, String> {
    let named = |m: &Map<String, Value>| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>();
    match v {
        Value::Object(o) => {
            if let Some(Value::Object(m)) = o.get("mcpServers") {
                return Ok(named(m));
            }
            if let Some(Value::Object(m)) = o.get("servers") {
                return Ok(named(m));
            }
            if let Some(Value::Object(mcp)) = o.get("mcp") {
                if let Some(Value::Object(m)) = mcp.get("servers").or_else(|| mcp.get("mcpServers")) {
                    return Ok(named(m));
                }
            }
            if looks_like_server(v) {
                let name = o.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                return Ok(vec![(name, v.clone())]);
            }
            if !o.is_empty() && o.values().all(Value::is_object) {
                return Ok(named(o));
            }
            Err("No MCP servers found. Paste a configuration with \"mcpServers\" or \"servers\", or one server with \"command\" or \"url\".".into())
        }
        Value::Array(list) => Ok(list
            .iter()
            .map(|item| (item.get("name").and_then(Value::as_str).unwrap_or("").to_string(), item.clone()))
            .collect()),
        _ => Err("No MCP servers found: the pasted JSON is not an object.".into()),
    }
}

fn string_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn string_map(v: Option<&Value>, what: &str) -> Result<BTreeMap<String, String>, String> {
    match v {
        None | Some(Value::Null) => Ok(BTreeMap::new()),
        Some(Value::Object(m)) => Ok(m.iter().map(|(k, v)| (k.clone(), string_of(v))).collect()),
        Some(_) => Err(format!("\"{what}\" must be an object of names and values.")),
    }
}

/// A name of letters, digits, `_` and `-` (at most 64 characters) made from `raw`.
pub fn clean_name(raw: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in raw.trim().chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    let out: String = out.chars().take(64).collect();
    out.trim_matches('-').to_string()
}

/// A name for an unnamed server: the package or program it runs, or the URL's host.
fn guess_name(server: &McpServerIn) -> String {
    let from = match server.transport {
        McpTransport::Stdio => server
            .args
            .iter()
            .rev()
            .find(|a| !a.starts_with('-'))
            .cloned()
            .or_else(|| server.command.clone())
            .map(|a| {
                let a = a.rsplit('/').next().unwrap_or(&a).to_string();
                a.split('@').find(|p| !p.is_empty()).unwrap_or("").to_string()
            }),
        McpTransport::Http => server.url.as_deref().and_then(|u| {
            let rest = u.split("://").nth(1)?;
            Some(rest.split(['/', ':']).next().unwrap_or("").to_string())
        }),
    };
    let name = clean_name(&from.unwrap_or_default());
    if name.is_empty() {
        "server".into()
    } else {
        name
    }
}

/// One server object as an `McpServerIn` (not yet validated), with notes.
fn read_server(entry: &str, v: &Value) -> Result<(McpServerIn, Vec<String>), String> {
    let Some(o) = v.as_object() else {
        return Err("This entry is not an object.".into());
    };
    let mut notes = Vec::new();
    let kind = o.get("type").or_else(|| o.get("transport")).and_then(Value::as_str).map(|s| s.trim().to_lowercase());
    let url = o.get("url").or_else(|| o.get("serverUrl")).map(string_of).filter(|u| !u.trim().is_empty());
    let command = o.get("command").map(string_of).filter(|c| !c.trim().is_empty());
    let transport = match kind.as_deref() {
        Some("stdio") | Some("local") => McpTransport::Stdio,
        Some("http") | Some("streamable-http") | Some("streamablehttp") | Some("streamable_http") | Some("remote") => {
            McpTransport::Http
        }
        Some("sse") => {
            notes.push(
                "Saved as streamable HTTP: Stellarator does not support the legacy SSE transport. If the check \
                 fails, use the server's streamable HTTP URL (often ending in /mcp instead of /sse)."
                    .into(),
            );
            McpTransport::Http
        }
        Some(other) => return Err(format!("The transport \"{other}\" is not supported; use stdio or http.")),
        None if command.is_some() => McpTransport::Stdio,
        None if url.is_some() => McpTransport::Http,
        None => return Err("This entry has neither a \"command\" nor a \"url\".".into()),
    };
    let args = match o.get("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().map(string_of).collect(),
        Some(Value::String(s)) => s.split_whitespace().map(str::to_string).collect(),
        Some(_) => return Err("\"args\" must be a list of strings.".into()),
    };
    let env = string_map(o.get("env"), "env")?;
    let headers = string_map(o.get("headers"), "headers")?;
    for (k, value) in env.iter().chain(headers.iter()) {
        if value.contains("${") {
            notes.push(format!(
                "The value of {k} is a placeholder ({value}); edit the server and enter the real value."
            ));
        }
    }
    if o.contains_key("envFile") {
        notes.push("\"envFile\" is not read; add the variables under env.".into());
    }
    let enabled = !o.get("disabled").and_then(Value::as_bool).unwrap_or(false)
        && o.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let mut server = McpServerIn {
        name: String::new(),
        transport,
        command: if transport == McpTransport::Stdio { command } else { None },
        args: if transport == McpTransport::Stdio { args } else { Vec::new() },
        env: if transport == McpTransport::Stdio { env } else { BTreeMap::new() },
        url: if transport == McpTransport::Http { url } else { None },
        headers: if transport == McpTransport::Http { headers } else { BTreeMap::new() },
        enabled,
    };
    let cleaned = clean_name(entry);
    server.name = if cleaned.is_empty() { guess_name(&server) } else { cleaned };
    if !entry.is_empty() && server.name != entry {
        notes.push(format!("Saved as \"{}\": names use only letters, digits, _ and -.", server.name));
    }
    Ok((server, notes))
}

/// Read a paste. `taken` holds the names already saved (compared ignoring case); a clash gets the
/// suffix `-2`, `-3`, ... and a note.
pub fn parse(text: &str, taken: &[String]) -> Result<Parsed, String> {
    let value = parse_text(text)?;
    let raw = entries_of(&value)?;
    if raw.is_empty() {
        return Err("The pasted configuration has no servers in it.".into());
    }
    let mut used: HashSet<String> = taken.iter().map(|n| n.to_lowercase()).collect();
    let mut out = Parsed::default();
    for (entry, v) in raw {
        match read_server(&entry, &v) {
            Err(e) => out.errors.push((entry, e)),
            Ok((mut server, mut notes)) => {
                if used.contains(&server.name.to_lowercase()) {
                    let base: String = server.name.chars().take(60).collect();
                    let mut n = 2;
                    let fresh = loop {
                        let candidate = format!("{base}-{n}");
                        if !used.contains(&candidate.to_lowercase()) {
                            break candidate;
                        }
                        n += 1;
                    };
                    notes.push(format!("Saved as \"{fresh}\": the name \"{}\" is taken.", server.name));
                    server.name = fresh;
                }
                used.insert(server.name.to_lowercase());
                out.entries.push(Entry { entry, server, notes });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(p: &Parsed) -> Vec<&str> {
        p.entries.iter().map(|e| e.server.name.as_str()).collect()
    }

    #[test]
    fn claude_desktop_cursor_windsurf() {
        let text = r#"{
          "mcpServers": {
            "filesystem": {"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]},
            "github": {"command": "docker", "args": ["run", "-i", "--rm", "ghcr.io/github/github-mcp-server"],
                       "env": {"GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_0123456789"}},
            "remote": {"url": "https://example.com/mcp", "headers": {"Authorization": "Bearer abc"}},
            "windsurf": {"serverUrl": "https://ws.example.com/mcp"}
          }
        }"#;
        let p = parse(text, &[]).unwrap();
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert_eq!(names(&p), ["filesystem", "github", "remote", "windsurf"]);
        let fs = &p.entries[0].server;
        assert_eq!(fs.transport, McpTransport::Stdio);
        assert_eq!(fs.command.as_deref(), Some("npx"));
        assert_eq!(fs.args, ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]);
        assert!(fs.enabled);
        assert_eq!(p.entries[1].server.env["GITHUB_PERSONAL_ACCESS_TOKEN"], "ghp_0123456789");
        let remote = &p.entries[2].server;
        assert_eq!((remote.transport, remote.url.as_deref()), (McpTransport::Http, Some("https://example.com/mcp")));
        assert_eq!(remote.headers["Authorization"], "Bearer abc");
        assert_eq!(p.entries[3].server.url.as_deref(), Some("https://ws.example.com/mcp"));
    }

    #[test]
    fn vs_code_with_comments_types_and_inputs() {
        let text = r#"{
          // VS Code's mcp.json
          "inputs": [{"type": "promptString", "id": "token", "password": true}],
          "servers": {
            "fetch": {"type": "stdio", "command": "uvx", "args": ["mcp-server-fetch"],},
            "api": {"type": "http", "url": "https://api.example.com/mcp", "headers": {"Authorization": "Bearer ${input:token}"}},
            "legacy": {"type": "sse", "url": "https://old.example.com/sse"}, /* trailing comma */
          },
        }"#;
        let p = parse(text, &[]).unwrap();
        assert_eq!(names(&p), ["fetch", "api", "legacy"]);
        assert_eq!(p.entries[0].server.transport, McpTransport::Stdio);
        assert!(p.entries[1].notes[0].contains("placeholder"));
        assert_eq!(p.entries[2].server.transport, McpTransport::Http);
        assert!(p.entries[2].notes[0].contains("legacy SSE"));
        // settings.json nests it under "mcp".
        let p = parse(r#"{"mcp": {"servers": {"x": {"command": "x"}}}}"#, &[]).unwrap();
        assert_eq!(names(&p), ["x"]);
    }

    #[test]
    fn single_objects_lists_and_fragments() {
        let p = parse(r#"{"command": "npx", "args": ["-y", "@acme/weather-mcp@latest"]}"#, &[]).unwrap();
        assert_eq!(names(&p), ["weather-mcp"]);
        assert_eq!(p.entries[0].entry, "");
        let p = parse(r#"{"name": "docs", "url": "https://docs.example.com/mcp"}"#, &[]).unwrap();
        assert_eq!(names(&p), ["docs"]);
        let p = parse(r#"{"url": "https://mcp.example.com:8443/mcp"}"#, &[]).unwrap();
        assert_eq!(names(&p), ["mcp-example-com"]);
        let p = parse(r#"[{"name": "a", "command": "a"}, {"name": "b", "url": "http://b"}]"#, &[]).unwrap();
        assert_eq!(names(&p), ["a", "b"]);
        // Copied from inside a file, without the outer braces.
        let p = parse(r#""time": {"command": "uvx", "args": ["mcp-server-time"]},"#, &[]).unwrap();
        assert_eq!(names(&p), ["time"]);
        // The inner map alone.
        let p = parse(r#"{"one": {"command": "a"}, "two": {"url": "http://b"}}"#, &[]).unwrap();
        assert_eq!(names(&p), ["one", "two"]);
    }

    #[test]
    fn clashes_renames_and_errors() {
        let text = r#"{"mcpServers": {
            "GitHub": {"command": "x"},
            "github": {"command": "y"},
            "my server!": {"command": "z", "disabled": true},
            "bad": {"type": "websocket", "url": "ws://x"},
            "empty": {},
            "notobj": 3
        }}"#;
        let p = parse(text, &["github".to_string()]).unwrap();
        assert_eq!(names(&p), ["GitHub-2", "github-3", "my-server"]);
        assert!(p.entries[0].notes[0].contains("is taken"));
        assert!(!p.entries[2].server.enabled);
        assert!(p.entries[2].notes[0].contains("Saved as \"my-server\""));
        let errs: Vec<&str> = p.errors.iter().map(|(e, _)| e.as_str()).collect();
        assert_eq!(errs, ["bad", "empty", "notobj"]);
        assert!(p.errors[0].1.contains("websocket"));
        assert!(parse("not json", &[]).unwrap_err().contains("not valid JSON"));
        assert!(parse(r#"{"a": 1}"#, &[]).unwrap_err().contains("No MCP servers"));
        assert!(parse(r#"{"mcpServers": {}}"#, &[]).unwrap_err().contains("no servers"));
    }

    #[test]
    fn jsonc_keeps_strings_intact() {
        assert_eq!(strip_jsonc(r#"{"a": "http://x//y", /* c */ "b": [1, 2,],}"#), r#"{"a": "http://x//y",  "b": [1, 2]}"#);
        assert_eq!(strip_jsonc(r#"{"a": "q\"//"} // end"#), r#"{"a": "q\"//"} "#);
    }
}
