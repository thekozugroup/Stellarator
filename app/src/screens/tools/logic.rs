//! The Tools screen's rules, with no UI: a server's state in words, its target line, the args
//! field's splitting and joining, the secret rows (env, headers) and the request bodies they make,
//! what a pasted config holds, and an import's outcome in sentences.

use std::collections::BTreeMap;

use crate::api::types::{McpImportOut, McpServerIn, McpServerOut, McpServerPatch, McpStatus, McpTransport, RunOut};
use crate::format::count;
use crate::ui::status::Tone;

// ------------------------------------------------------------------ state

/// A server's state as the list, the sheet and New run show it.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub word: &'static str,
    pub tone: Tone,
    /// Why it cannot be used, or why its last check failed.
    pub reason: Option<String>,
}

/// The state of `server`: blocked (a stdio server this server may not run) comes first, then off,
/// then the last check: Reachable, Error, Not checked (the endpoint tones).
pub fn server_state(server: &McpServerOut) -> State {
    if let Some(why) = server.blocked.clone() {
        return State { word: "Blocked", tone: Tone::Attention, reason: Some(why) };
    }
    if !server.enabled {
        return State { word: "Off", tone: Tone::Plain, reason: None };
    }
    match server.status {
        McpStatus::Ok => State { word: "Reachable", tone: Tone::Positive, reason: None },
        McpStatus::Error => State { word: "Error", tone: Tone::Negative, reason: server.error.clone() },
        McpStatus::Unchecked => State { word: "Not checked", tone: Tone::Plain, reason: None },
    }
}

/// "stdio" or "HTTP".
pub fn transport_label(t: McpTransport) -> &'static str {
    match t {
        McpTransport::Stdio => "stdio",
        McpTransport::Http => "HTTP",
    }
}

/// What the server runs or where it is: the command and its args, or the URL.
pub fn target(server: &McpServerOut) -> String {
    match server.transport {
        McpTransport::Stdio => {
            let mut parts = vec![quote_arg(server.command.as_deref().unwrap_or_default())];
            parts.extend(server.args.iter().map(|a| quote_arg(a)));
            parts.join(" ").trim().to_owned()
        }
        McpTransport::Http => server.url.clone().unwrap_or_default(),
    }
}

/// "12 tools", or the dash's place (`None`) before a successful check.
pub fn tool_count(server: &McpServerOut) -> Option<String> {
    (server.status == McpStatus::Ok || !server.tools.is_empty()).then(|| count(server.tools.len() as i64, "tool", "tools"))
}

// ------------------------------------------------------------------ args

/// `arg` as a shell would need it typed: as it is when it has no space or quote, else in double
/// quotes with `"` and `\` escaped.
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\')) {
        return arg.to_owned();
    }
    let mut out = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The args as the field shows them: each quoted when it needs it, spaces between.
pub fn join_args(args: &[String]) -> String {
    args.iter().map(|a| quote_arg(a)).collect::<Vec<_>>().join(" ")
}

/// The args typed in the field, split as a shell splits words: on spaces, with "double" and
/// 'single' quotes keeping spaces and `\` escaping the next character (outside single quotes).
pub fn split_args(text: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('\''), '\'') => quote = None,
            (Some('"'), '"') => quote = None,
            (Some('"') | None, '\\') => {
                in_word = true;
                match chars.next() {
                    Some(next) => current.push(next),
                    None => current.push('\\'),
                }
            }
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    args.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            (None, c) => {
                current.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return Err("A quote is not closed.".to_owned());
    }
    if in_word {
        args.push(current);
    }
    Ok(args)
}

// ------------------------------------------------------------------ secret rows

/// One env variable or header in the form: a new one (name and value typed), or one saved on the
/// server (its name fixed, its value shown as a hint until replaced).
#[derive(Debug, Clone, PartialEq)]
pub struct SecretRow {
    /// Stays with the row while it is edited.
    pub id: u64,
    pub name: String,
    pub value: String,
    /// Saved on the server: the end of its value (`None` for an empty one). `None` for a new row.
    pub saved: Option<Option<String>>,
    /// A saved row whose value is being replaced by `value`.
    pub replacing: bool,
}

impl SecretRow {
    /// Whether the row sends a value (new, or saved and replaced); a saved row that keeps its value
    /// sends `null`.
    pub fn sends_value(&self) -> bool {
        self.saved.is_none() || self.replacing
    }
}

/// The rows of a saved server's secrets, ids from `first`.
pub fn saved_rows(hints: &[crate::api::types::SecretHint], first: u64) -> Vec<SecretRow> {
    hints
        .iter()
        .enumerate()
        .map(|(i, h)| SecretRow { id: first + i as u64, name: h.name.clone(), value: String::new(), saved: Some(h.hint.clone()), replacing: false })
        .collect()
}

/// What is wrong with the rows, if anything: a value without a name, or a name twice. `what` is
/// "variable" or "header".
pub fn check_rows(rows: &[SecretRow], what: &str) -> Option<String> {
    let mut seen = std::collections::BTreeSet::new();
    for row in rows {
        let name = row.name.trim();
        if name.is_empty() {
            if !row.value.is_empty() {
                return Some(format!("Name each {what}, or remove the row."));
            }
            continue;
        }
        if !seen.insert(name.to_owned()) {
            return Some(format!("{name} is listed twice."));
        }
    }
    None
}

/// The rows as a new server's map: every named row with its value.
pub fn rows_to_map(rows: &[SecretRow]) -> BTreeMap<String, String> {
    rows.iter().filter(|r| !r.name.trim().is_empty()).map(|r| (r.name.trim().to_owned(), r.value.clone())).collect()
}

/// The rows as a patch's complete set: a value for new and replaced rows, `null` (keep the saved
/// value) for saved rows left as they are. A saved name not in `rows` (removed) is left out, which
/// removes it.
pub fn rows_to_patch(rows: &[SecretRow]) -> BTreeMap<String, Option<String>> {
    rows.iter()
        .filter(|r| !r.name.trim().is_empty())
        .map(|r| (r.name.trim().to_owned(), r.sends_value().then(|| r.value.clone())))
        .collect()
}

// ------------------------------------------------------------------ the form

/// The form's values, as typed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerForm {
    pub name: String,
    pub transport: McpTransport,
    pub command: String,
    pub args: String,
    pub env: Vec<SecretRow>,
    pub url: String,
    pub headers: Vec<SecretRow>,
}

/// The form's field a message belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FormField {
    Name,
    Command,
    Args,
    Env,
    Url,
    Headers,
}

impl FormField {
    /// The field a server message names (a path of `McpServerIn`).
    pub fn from_server(path: &str) -> Option<FormField> {
        Some(match path.split('.').next().unwrap_or(path) {
            "name" => FormField::Name,
            "command" => FormField::Command,
            "args" => FormField::Args,
            "env" => FormField::Env,
            "url" => FormField::Url,
            "headers" => FormField::Headers,
            _ => return None,
        })
    }
}

/// Letters, digits, `_` and `-`, 1 to 64 characters (the server's rule for names).
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.chars().count()) && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// Every field's message, in screen order.
pub fn check_form(form: &ServerForm) -> Vec<(FormField, String)> {
    let mut errors = Vec::new();
    let name = form.name.trim();
    if name.is_empty() {
        errors.push((FormField::Name, "Enter a name. Tools from this server are sent under it.".to_owned()));
    } else if !valid_name(name) {
        errors.push((FormField::Name, "Use letters, digits, _ and -, up to 64 characters.".to_owned()));
    }
    match form.transport {
        McpTransport::Stdio => {
            if form.command.trim().is_empty() {
                errors.push((FormField::Command, "Enter the command that starts the server, for example npx.".to_owned()));
            }
            if let Err(e) = split_args(&form.args) {
                errors.push((FormField::Args, e));
            }
            if let Some(e) = check_rows(&form.env, "variable") {
                errors.push((FormField::Env, e));
            }
        }
        McpTransport::Http => {
            let url = form.url.trim().to_ascii_lowercase();
            if url.is_empty() {
                errors.push((FormField::Url, "Enter the server's URL, for example http://localhost:3000/mcp.".to_owned()));
            } else if !(url.starts_with("http://") || url.starts_with("https://")) {
                errors.push((FormField::Url, "The URL must start with http:// or https://.".to_owned()));
            }
            if let Some(e) = check_rows(&form.headers, "header") {
                errors.push((FormField::Headers, e));
            }
        }
    }
    errors
}

/// A new server from a valid form. Only the transport's own fields are sent.
pub fn to_create(form: &ServerForm) -> McpServerIn {
    let stdio = form.transport == McpTransport::Stdio;
    McpServerIn {
        name: form.name.trim().to_owned(),
        transport: form.transport,
        command: stdio.then(|| form.command.trim().to_owned()),
        args: if stdio { split_args(&form.args).unwrap_or_default() } else { Vec::new() },
        env: if stdio { rows_to_map(&form.env) } else { BTreeMap::new() },
        url: (!stdio).then(|| form.url.trim().to_owned()),
        headers: if stdio { BTreeMap::new() } else { rows_to_map(&form.headers) },
        enabled: true,
    }
}

/// The patch of a valid form over `saved`: every field the form shows, `env` and `headers` as
/// complete sets (a saved value kept as `null`).
pub fn to_patch(form: &ServerForm, saved: &McpServerOut) -> McpServerPatch {
    let stdio = form.transport == McpTransport::Stdio;
    let name = form.name.trim().to_owned();
    McpServerPatch {
        name: (name != saved.name).then_some(name),
        transport: Some(form.transport),
        command: stdio.then(|| form.command.trim().to_owned()),
        args: stdio.then(|| split_args(&form.args).unwrap_or_default()),
        env: stdio.then(|| rows_to_patch(&form.env)),
        url: (!stdio).then(|| form.url.trim().to_owned()),
        headers: (!stdio).then(|| rows_to_patch(&form.headers)),
        enabled: None,
    }
}

// ------------------------------------------------------------------ pasting

/// What a pasted config holds, as far as the app can tell before the server reads it.
#[derive(Debug, Clone, PartialEq)]
pub enum Pasted {
    Empty,
    /// Not JSON (yet): the parser's message.
    Invalid(String),
    /// JSON, but none of the shapes the import reads.
    Unknown,
    /// The servers' names ("" for one unnamed server object).
    Servers(Vec<String>),
}

/// A server object: it has a command or a URL.
fn is_server(v: &serde_json::Value) -> bool {
    v.as_object().is_some_and(|o| o.contains_key("command") || o.contains_key("url") || o.contains_key("serverUrl"))
}

/// The servers in `text`, read as the import reads it: `{"mcpServers": {...}}` (Claude Desktop,
/// Cursor, Windsurf), `{"servers": {...}}` (VS Code), `{"name": {...}}`, or one server object.
pub fn pasted_servers(text: &str) -> Pasted {
    if text.trim().is_empty() {
        return Pasted::Empty;
    }
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return Pasted::Invalid(e.to_string()),
    };
    let Some(object) = value.as_object() else { return Pasted::Unknown };
    if is_server(&value) {
        return Pasted::Servers(vec![String::new()]);
    }
    for key in ["mcpServers", "servers"] {
        if let Some(map) = object.get(key).and_then(|v| v.as_object()) {
            return Pasted::Servers(map.keys().cloned().collect());
        }
    }
    if !object.is_empty() && object.values().all(is_server) {
        return Pasted::Servers(object.keys().cloned().collect());
    }
    Pasted::Unknown
}

/// The line under the paste box: what was found, or what is wrong.
pub fn pasted_line(pasted: &Pasted) -> Option<(String, bool)> {
    match pasted {
        Pasted::Empty => None,
        Pasted::Invalid(e) => Some((format!("Not valid JSON yet: {e}"), true)),
        Pasted::Unknown => Some(("No servers found. Paste an object with mcpServers or servers in it.".to_owned(), true)),
        Pasted::Servers(names) if names.len() == 1 && names[0].is_empty() => Some(("1 server found.".to_owned(), false)),
        Pasted::Servers(names) if names.is_empty() => Some(("The list of servers is empty.".to_owned(), true)),
        Pasted::Servers(names) => Some((format!("{} found: {}.", count(names.len() as i64, "server", "servers"), names.join(", ")), false)),
    }
}

/// One line of an import's outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The server's name (or the pasted entry's, for a failure).
    pub name: String,
    pub state: State,
    /// What the line says after the name.
    pub detail: String,
}

/// An import's outcome: each created server with its state after the check (and a note when it
/// was renamed or changed), then each entry that failed.
pub fn import_outcome(out: &McpImportOut) -> (Vec<Outcome>, Vec<Outcome>) {
    let created = out
        .created
        .iter()
        .map(|s| {
            let state = server_state(s);
            let mut detail = match (&state.reason, s.status) {
                (Some(r), _) => r.clone(),
                (None, McpStatus::Ok) => count(s.tools.len() as i64, "tool", "tools"),
                (None, _) => String::new(),
            };
            // A note names the pasted entry; a renamed server has its new name.
            let notes: Vec<&str> = out
                .notes
                .iter()
                .filter(|n| n.entry == s.name || n.message.contains(&format!("“{}”", s.name)) || n.message.contains(&format!("\"{}\"", s.name)))
                .map(|n| n.message.as_str())
                .collect();
            for n in notes {
                if !detail.is_empty() {
                    detail.push_str(". ");
                }
                detail.push_str(n.trim_end_matches('.'));
            }
            Outcome { name: s.name.clone(), state, detail }
        })
        .collect();
    let failed = out
        .errors
        .iter()
        .map(|e| Outcome {
            name: if e.entry.is_empty() { "Server".to_owned() } else { e.entry.clone() },
            state: State { word: "Not added", tone: Tone::Negative, reason: None },
            detail: e.message.clone(),
        })
        .collect();
    (created, failed)
}

/// The notes no created server's line took (a note about an entry under another name).
pub fn stray_notes(out: &McpImportOut) -> Vec<String> {
    out.notes
        .iter()
        .filter(|n| {
            !out.created.iter().any(|s| {
                n.entry == s.name || n.message.contains(&format!("“{}”", s.name)) || n.message.contains(&format!("\"{}\"", s.name))
            })
        })
        .map(|n| if n.entry.is_empty() { n.message.clone() } else { format!("{}: {}", n.entry, n.message) })
        .collect()
}

/// The dialog's title once an import answered: "Added 3 servers", "Nothing added".
pub fn import_title(out: &McpImportOut) -> String {
    match out.created.len() {
        0 => "Nothing added".to_owned(),
        n => format!("Added {}", count(n as i64, "server", "servers")),
    }
}

// ------------------------------------------------------------------ runs

/// The runs that use server `id` (their tools snapshot, or their parameters), newest first.
pub fn runs_using(runs: &[RunOut], id: i64) -> Vec<String> {
    runs.iter()
        .filter(|r| {
            r.tools.as_ref().is_some_and(|t| t.servers.iter().any(|s| s.server_id == id))
                || r.params.tools.as_ref().is_some_and(|t| t.servers.iter().any(|s| s.server_id == id))
        })
        .map(|r| r.name.clone())
        .collect()
}

/// The delete alert's sentence: the runs it was used by, named (three at most).
pub fn delete_description(names: &[String], run_count: i64) -> String {
    let n = run_count.max(names.len() as i64);
    if n == 0 {
        return "No runs use this server.".to_owned();
    }
    let shown: Vec<String> = names.iter().take(3).map(|n| format!("“{n}”")).collect();
    let rest = n - shown.len() as i64;
    let list = match (shown.len(), rest) {
        (0, _) => count(n, "run", "runs"),
        (_, r) if r > 0 => format!("{} and {}", shown.join(", "), count(r, "other run", "other runs")),
        (1, _) => shown[0].clone(),
        (k, _) => format!("{} and {}", shown[..k - 1].join(", "), shown[k - 1]),
    };
    format!("Used by {list}. They keep their samples and traces.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub fn server(id: i64, name: &str, status: &str, tools: &[&str]) -> McpServerOut {
        serde_json::from_value(json!({
            "id": id, "name": name, "transport": "stdio", "command": "npx", "args": ["-y", "pkg name"],
            "env": [{"name": "TOKEN", "hint": "…a9f2"}], "url": null, "headers": [], "enabled": true,
            "status": status, "error": if status == "error" { json!("Connection closed") } else { json!(null) },
            "checked_at": null, "server_info": null,
            "tools": tools.iter().map(|t| json!({"name": t, "description": null, "input_schema": {}})).collect::<Vec<_>>(),
            "blocked": null, "run_count": 0,
            "created_at": "2026-11-01T00:00:00Z", "updated_at": "2026-11-01T00:00:00Z"
        }))
        .unwrap()
    }

    #[test]
    fn states_put_blocked_and_off_before_the_check() {
        let mut s = server(1, "github", "ok", &["a"]);
        assert_eq!(server_state(&s).word, "Reachable");
        s.status = McpStatus::Error;
        s.error = Some("Connection closed".into());
        assert_eq!((server_state(&s).word, server_state(&s).reason.as_deref()), ("Error", Some("Connection closed")));
        s.enabled = false;
        assert_eq!(server_state(&s).word, "Off");
        s.blocked = Some("Stdio is off.".into());
        assert_eq!(server_state(&s).tone, Tone::Attention);
        assert_eq!(target(&server(1, "x", "ok", &[])), "npx -y \"pkg name\"");
    }

    #[test]
    fn args_split_and_join_as_a_shell_does() {
        assert_eq!(split_args("-y @scope/pkg  --dir \"/my files\" 'a b' c\\ d").unwrap(), ["-y", "@scope/pkg", "--dir", "/my files", "a b", "c d"]);
        assert_eq!(split_args("").unwrap(), Vec::<String>::new());
        assert_eq!(split_args("\"\"").unwrap(), [""]);
        assert!(split_args("\"open").is_err());
        let args = vec!["-y".to_owned(), "a b".to_owned(), "say \"hi\"".to_owned(), String::new()];
        assert_eq!(split_args(&join_args(&args)).unwrap(), args);
    }

    #[test]
    fn saved_secrets_are_kept_replaced_or_removed() {
        let hints = vec![
            crate::api::types::SecretHint { name: "A".into(), hint: Some("…1234".into()) },
            crate::api::types::SecretHint { name: "B".into(), hint: None },
            crate::api::types::SecretHint { name: "C".into(), hint: Some("…".into()) },
        ];
        let mut rows = saved_rows(&hints, 1);
        // B replaced, C removed, D added.
        rows[1].replacing = true;
        rows[1].value = "new".into();
        rows.remove(2);
        rows.push(SecretRow { id: 9, name: " D ".into(), value: "d".into(), saved: None, replacing: false });
        let patch = rows_to_patch(&rows);
        assert_eq!(patch.get("A"), Some(&None));
        assert_eq!(patch.get("B"), Some(&Some("new".to_owned())));
        assert_eq!(patch.get("C"), None);
        assert_eq!(patch.get("D"), Some(&Some("d".to_owned())));
        assert_eq!(check_rows(&rows, "variable"), None);
        rows.push(SecretRow { id: 10, name: "A".into(), value: String::new(), saved: None, replacing: false });
        assert_eq!(check_rows(&rows, "variable").as_deref(), Some("A is listed twice."));
        let blank = vec![SecretRow { id: 1, name: " ".into(), value: "x".into(), saved: None, replacing: false }];
        assert_eq!(check_rows(&blank, "header").as_deref(), Some("Name each header, or remove the row."));
    }

    #[test]
    fn the_form_checks_and_makes_its_bodies() {
        let mut f = ServerForm { name: "git hub".into(), command: " ".into(), ..Default::default() };
        let fields: Vec<FormField> = check_form(&f).into_iter().map(|(f, _)| f).collect();
        assert_eq!(fields, [FormField::Name, FormField::Command]);
        f.name = "github".into();
        f.command = "npx".into();
        f.args = "-y @modelcontextprotocol/server-github".into();
        assert!(check_form(&f).is_empty());
        let body = to_create(&f);
        assert_eq!(body.command.as_deref(), Some("npx"));
        assert_eq!(body.args, ["-y", "@modelcontextprotocol/server-github"]);
        assert_eq!(body.url, None);
        f.transport = McpTransport::Http;
        f.url = "ftp://x".into();
        assert_eq!(check_form(&f)[0].0, FormField::Url);
        f.url = "https://mcp.example.com/mcp".into();
        let body = to_create(&f);
        assert_eq!((body.command, body.url.as_deref()), (None, Some("https://mcp.example.com/mcp")));
        // A patch sends the name only when it changed.
        let saved = server(4, "github", "ok", &[]);
        let patch = to_patch(&f, &saved);
        assert_eq!(patch.name, None);
        assert_eq!(patch.transport, Some(McpTransport::Http));
        assert_eq!(patch.headers, Some(BTreeMap::new()));
        assert_eq!(patch.env, None);
    }

    #[test]
    fn pasted_configs_are_read_in_the_imports_shapes() {
        assert_eq!(pasted_servers("  "), Pasted::Empty);
        assert!(matches!(pasted_servers("{\"mcpServers\": {"), Pasted::Invalid(_)));
        let claude = r#"{"mcpServers": {"github": {"command": "npx"}, "docs": {"url": "http://x/mcp"}}}"#;
        // In the order they were pasted.
        assert_eq!(pasted_servers(claude), Pasted::Servers(vec!["github".into(), "docs".into()]));
        let vscode = r#"{"servers": {"fetch": {"type": "stdio", "command": "uvx", "args": ["mcp-server-fetch"]}}}"#;
        assert_eq!(pasted_servers(vscode), Pasted::Servers(vec!["fetch".into()]));
        assert_eq!(pasted_servers(r#"{"command": "npx", "args": []}"#), Pasted::Servers(vec![String::new()]));
        assert_eq!(pasted_servers(r#"{"a": {"url": "http://x"}}"#), Pasted::Servers(vec!["a".into()]));
        assert_eq!(pasted_servers("[1]"), Pasted::Unknown);
        assert_eq!(pasted_line(&Pasted::Servers(vec!["a".into(), "b".into()])), Some(("2 servers found: a, b.".into(), false)));
    }

    #[test]
    fn an_import_says_what_was_added_renamed_and_not_added() {
        let out: McpImportOut = serde_json::from_value(json!({
            "created": [serde_json::to_value(server(1, "github-2", "ok", &["a", "b"])).unwrap(), serde_json::to_value(server(2, "docs", "error", &[])).unwrap()],
            "errors": [{"entry": "bad", "message": "Give it a command or a url."}],
            "notes": [{"entry": "github", "message": "Renamed to “github-2”: the name was taken."}]
        }))
        .unwrap();
        let (created, failed) = import_outcome(&out);
        assert_eq!(created[0].detail, "2 tools. Renamed to “github-2”: the name was taken");
        assert_eq!(created[1].state.word, "Error");
        assert_eq!(created[1].detail, "Connection closed");
        assert_eq!(failed[0].name, "bad");
        assert!(stray_notes(&out).is_empty());
        assert_eq!(import_title(&out), "Added 2 servers");
    }

    #[test]
    fn the_delete_alert_names_the_runs() {
        assert_eq!(delete_description(&[], 0), "No runs use this server.");
        assert_eq!(delete_description(&["A".into()], 1), "Used by “A”. They keep their samples and traces.");
        assert_eq!(delete_description(&["A".into(), "B".into()], 2), "Used by “A” and “B”. They keep their samples and traces.");
        assert_eq!(
            delete_description(&["A".into(), "B".into(), "C".into(), "D".into()], 4),
            "Used by “A”, “B”, “C” and 1 other run. They keep their samples and traces."
        );
        assert_eq!(delete_description(&[], 2), "Used by 2 runs. They keep their samples and traces.");
    }
}
