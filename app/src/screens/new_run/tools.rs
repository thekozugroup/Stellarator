//! The Tools group's rules, with no UI: which servers can be picked, the picks as the run's
//! `params.tools`, the summary and the name clashes the group shows, and a copied run's tools
//! checked against the servers there are now (Run again).

use crate::api::types::{McpServerOut as Server, McpStatus, RunToolServer, RunTools, ToolChoice, ToolSnapshot};
use crate::format::{count, int};

/// One server picked for the run: all its tools (`None`), or these, in the server's order.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub server_id: i64,
    pub tools: Option<Vec<String>>,
}

/// The group's choices (the numbers are form fields).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolsForm {
    /// "Use tools".
    pub on: bool,
    /// In the order the servers are listed.
    pub picks: Vec<Pick>,
    pub choice: ToolChoice,
}

/// Why `server` cannot be picked, or `None` when it can: blocked here, turned off, its last check
/// failed, or it has no tools listed (not checked yet).
pub fn unpickable(server: &Server) -> Option<String> {
    if let Some(why) = &server.blocked {
        return Some(why.clone());
    }
    if !server.enabled {
        return Some("Turned off in Tools.".to_owned());
    }
    match server.status {
        McpStatus::Error => Some(match &server.error {
            Some(e) => format!("Its last check failed: {e}"),
            None => "Its last check failed.".to_owned(),
        }),
        _ if server.tools.is_empty() && server.status == McpStatus::Ok => Some("It lists no tools.".to_owned()),
        _ if server.tools.is_empty() => Some("Not checked yet.".to_owned()),
        _ => None,
    }
}

/// [`unpickable`] in a few words, for a note that lists several.
pub fn short_reason(server: &Server) -> &'static str {
    if server.blocked.is_some() {
        "blocked here"
    } else if !server.enabled {
        "turned off"
    } else if server.status == McpStatus::Error {
        "last check failed"
    } else if server.status == McpStatus::Ok {
        "no tools listed"
    } else {
        "not checked"
    }
}

impl ToolsForm {
    /// The pick of server `id`.
    pub fn pick(&self, id: i64) -> Option<&Pick> {
        self.picks.iter().find(|p| p.server_id == id)
    }

    /// Whether tool `tool` of `server` is offered.
    pub fn has_tool(&self, server: &Server, tool: &str) -> bool {
        match self.pick(server.id) {
            Some(Pick { tools: None, .. }) => true,
            Some(Pick { tools: Some(list), .. }) => list.iter().any(|t| t == tool),
            None => false,
        }
    }

    /// Turns server `server` on (all its tools) or off.
    pub fn set_server(&mut self, server: &Server, on: bool, order: &[Server]) {
        self.picks.retain(|p| p.server_id != server.id);
        if on {
            self.picks.push(Pick { server_id: server.id, tools: None });
            let place = |id: i64| order.iter().position(|s| s.id == id).unwrap_or(usize::MAX);
            self.picks.sort_by_key(|p| place(p.server_id));
        }
    }

    /// Offers tool `tool` of `server`, or stops offering it. The last tool unchecked turns the
    /// server off; checking every tool makes it "all".
    pub fn set_tool(&mut self, server: &Server, tool: &str, on: bool, order: &[Server]) {
        let mut chosen: Vec<String> = server.tools.iter().map(|t| t.name.clone()).filter(|t| self.has_tool(server, t)).collect();
        if on && !chosen.iter().any(|t| t == tool) {
            chosen.push(tool.to_owned());
        }
        if !on {
            chosen.retain(|t| t != tool);
        }
        if chosen.is_empty() {
            self.set_server(server, false, order);
            return;
        }
        // In the server's order.
        let names: Vec<String> = server.tools.iter().map(|t| t.name.clone()).filter(|t| chosen.contains(t)).collect();
        let all = names.len() == server.tools.len();
        if self.pick(server.id).is_none() {
            self.set_server(server, true, order);
        }
        if let Some(p) = self.picks.iter_mut().find(|p| p.server_id == server.id) {
            p.tools = if all { None } else { Some(names) };
        }
    }
}

/// The tool names offered from `server`.
pub fn offered(pick: &Pick, server: &Server) -> Vec<String> {
    match &pick.tools {
        None => server.tools.iter().map(|t| t.name.clone()).collect(),
        Some(list) => server.tools.iter().map(|t| t.name.clone()).filter(|t| list.contains(t)).collect(),
    }
}

/// The picks of servers that are listed and can be picked, with their servers.
fn live<'a>(form: &'a ToolsForm, servers: &'a [Server]) -> Vec<(&'a Pick, &'a Server)> {
    form.picks
        .iter()
        .filter_map(|p| servers.iter().find(|s| s.id == p.server_id).map(|s| (p, s)))
        .filter(|(_, s)| unpickable(s).is_none())
        .collect()
}

/// How many tools are offered, from how many servers.
pub fn totals(form: &ToolsForm, servers: &[Server]) -> (usize, usize) {
    let picked = live(form, servers);
    let tools = picked.iter().map(|(p, s)| offered(p, s).len()).sum();
    (tools, picked.iter().filter(|(p, s)| !offered(p, s).is_empty()).count())
}

/// "5 tools from 2 servers", "No tools picked".
pub fn summary(form: &ToolsForm, servers: &[Server]) -> String {
    match totals(form, servers) {
        (0, _) => "No tools picked".to_owned(),
        (tools, from) => format!("{} from {}", count(tools as i64, "tool", "tools"), count(from as i64, "server", "servers")),
    }
}

/// A tool name offered by two or more of the picked servers: each is sent as `<server>__<tool>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Clash {
    pub tool: String,
    pub servers: Vec<String>,
}

/// "a and b", "a, b and c".
pub fn and_list(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

/// The group's lines about shared names, one per set of servers that share them (backticks mark
/// code): "`lookup` is on docs and local: sent as `docs__lookup` and `local__lookup`.", or for
/// several names "`lookup`, `add` and 4 more are on docs and local: each sent as `docs__<name>` and
/// `local__<name>`."
pub fn clash_lines(found: &[Clash]) -> Vec<String> {
    let mut groups: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    for c in found {
        match groups.iter_mut().find(|(servers, _)| *servers == c.servers) {
            Some((_, tools)) => tools.push(c.tool.clone()),
            None => groups.push((c.servers.clone(), vec![c.tool.clone()])),
        }
    }
    groups
        .into_iter()
        .map(|(servers, tools)| {
            let on = and_list(&servers);
            if tools.len() == 1 {
                let sent: Vec<String> = servers.iter().map(|s| format!("`{s}__{}`", tools[0])).collect();
                return format!("`{}` is on {on}: sent as {}.", tools[0], and_list(&sent));
            }
            let named: Vec<String> = tools.iter().take(3).map(|t| format!("`{t}`")).collect();
            let names = if tools.len() > 3 {
                format!("{} and {} more", named.join(", "), int(tools.len() as i64 - 3))
            } else {
                and_list(&named)
            };
            let sent: Vec<String> = servers.iter().map(|s| format!("`{s}__<name>`")).collect();
            format!("{names} are on {on}: each sent as {}.", and_list(&sent))
        })
        .collect()
}

/// The names offered by more than one picked server, in the order they first appear.
pub fn clashes(form: &ToolsForm, servers: &[Server]) -> Vec<Clash> {
    let mut by_name: Vec<(String, Vec<String>)> = Vec::new();
    for (pick, server) in live(form, servers) {
        for tool in offered(pick, server) {
            match by_name.iter_mut().find(|(t, _)| *t == tool) {
                Some((_, list)) => list.push(server.name.clone()),
                None => by_name.push((tool, vec![server.name.clone()])),
            }
        }
    }
    by_name.into_iter().filter(|(_, s)| s.len() > 1).map(|(tool, servers)| Clash { tool, servers }).collect()
}

/// The run's `params.tools` for the picks and the numbers (already checked); `None` when tools
/// are off. A server whose every tool is checked is sent as "all", so tools it adds later come too.
pub fn to_run_tools(form: &ToolsForm, servers: &[Server], max_rounds: i64, tool_timeout_s: f64, max_result_chars: i64) -> Option<RunTools> {
    if !form.on {
        return None;
    }
    let picked = live(form, servers)
        .into_iter()
        .filter_map(|(pick, server)| {
            let names = offered(pick, server);
            if names.is_empty() {
                return None;
            }
            let all = names.len() == server.tools.len();
            Some(RunToolServer { server_id: server.id, tools: if all { None } else { Some(names) } })
        })
        .collect();
    Some(RunTools { servers: picked, max_rounds, tool_timeout_s, max_result_chars, tool_choice: form.choice, ..RunTools::default() })
}

/// A copied run's tools against the servers there are now: the picks kept, and a note for each
/// server or tool dropped (gone, or no longer usable). The snapshot names servers that are gone.
pub fn from_run_tools(tools: &RunTools, snapshot: Option<&ToolSnapshot>, servers: &[Server]) -> (ToolsForm, Vec<String>) {
    let mut picks = Vec::new();
    let mut notes = Vec::new();
    let old_name = |id: i64| {
        snapshot.and_then(|s| s.servers.iter().find(|x| x.server_id == id)).map(|x| x.name.clone()).unwrap_or_else(|| format!("server {}", int(id)))
    };
    for wanted in &tools.servers {
        let Some(server) = servers.iter().find(|s| s.id == wanted.server_id) else {
            notes.push(format!("{} (deleted)", old_name(wanted.server_id)));
            continue;
        };
        if unpickable(server).is_some() {
            notes.push(format!("{} ({})", server.name, short_reason(server)));
            continue;
        }
        let tools = match &wanted.tools {
            None => None,
            Some(list) => {
                let listed: Vec<String> = server.tools.iter().map(|t| t.name.clone()).filter(|t| list.contains(t)).collect();
                let gone: Vec<&String> = list.iter().filter(|t| !listed.contains(t)).collect();
                for t in &gone {
                    notes.push(format!("{} of {} (no longer listed)", t, server.name));
                }
                if listed.is_empty() {
                    continue;
                }
                if listed.len() == server.tools.len() { None } else { Some(listed) }
            }
        };
        picks.push(Pick { server_id: server.id, tools });
    }
    let order = |id: i64| servers.iter().position(|s| s.id == id).unwrap_or(usize::MAX);
    picks.sort_by_key(|p| order(p.server_id));
    (ToolsForm { on: true, picks, choice: tools.tool_choice }, notes)
}

/// The note under the group after Run again dropped something.
pub fn dropped_note(dropped: &[String]) -> Option<String> {
    (!dropped.is_empty()).then(|| format!("Not copied from the earlier run: {}.", dropped.join(", ")))
}

/// The Tool settings group's closed summary: "8 rounds, 60 s per call, 20,000 characters per
/// result, auto".
pub fn settings_summary(rounds: &str, timeout: &str, chars: &str, choice: ToolChoice) -> String {
    let n = |s: &str| s.trim().parse::<f64>().ok();
    let mut parts = Vec::new();
    if let Some(r) = n(rounds) {
        parts.push(count(r as i64, "round", "rounds"));
    }
    if let Some(t) = n(timeout) {
        parts.push(format!("{t} s per call"));
    }
    if let Some(c) = n(chars) {
        parts.push(format!("{} characters per result", int(c as i64)));
    }
    parts.push(match choice {
        ToolChoice::Auto => "auto".to_owned(),
        ToolChoice::Required => "a call required".to_owned(),
    });
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn server(id: i64, name: &str, status: &str, tools: &[&str]) -> Server {
        serde_json::from_value(json!({
            "id": id, "name": name, "transport": "http", "command": null, "args": [], "env": [],
            "url": "http://x/mcp", "headers": [], "enabled": true, "status": status,
            "error": if status == "error" { json!("Connection refused") } else { json!(null) },
            "checked_at": null, "server_info": null,
            "tools": tools.iter().map(|t| json!({"name": t, "description": null, "input_schema": {}})).collect::<Vec<_>>(),
            "blocked": null, "run_count": 0,
            "created_at": "2026-11-01T00:00:00Z", "updated_at": "2026-11-01T00:00:00Z"
        }))
        .unwrap()
    }

    fn servers() -> Vec<Server> {
        vec![server(1, "docs", "ok", &["search", "fetch"]), server(2, "github", "ok", &["search", "issues", "prs"]), server(3, "broken", "error", &[])]
    }

    #[test]
    fn picks_become_the_runs_tools() {
        let list = servers();
        let mut f = ToolsForm { on: true, ..Default::default() };
        f.set_server(&list[1], true, &list);
        f.set_server(&list[0], true, &list);
        // Kept in the servers' order.
        assert_eq!(f.picks.iter().map(|p| p.server_id).collect::<Vec<_>>(), [1, 2]);
        f.set_tool(&list[1], "prs", false, &list);
        assert_eq!(f.pick(2).unwrap().tools.as_deref(), Some(&["search".to_owned(), "issues".to_owned()][..]));
        assert_eq!(summary(&f, &list), "4 tools from 2 servers");
        let rt = to_run_tools(&f, &list, 8, 60.0, 20_000).unwrap();
        assert_eq!(rt.servers, vec![
            RunToolServer { server_id: 1, tools: None },
            RunToolServer { server_id: 2, tools: Some(vec!["search".into(), "issues".into()]) },
        ]);
        assert_eq!((rt.max_rounds, rt.tool_timeout_s, rt.max_result_chars, rt.tool_choice), (8, 60.0, 20_000, ToolChoice::Auto));
        // Checking the last tool again makes the server "all" again.
        f.set_tool(&list[1], "prs", true, &list);
        assert_eq!(f.pick(2).unwrap().tools, None);
        // The last tool unchecked turns the server off.
        f.set_tool(&list[0], "search", false, &list);
        f.set_tool(&list[0], "fetch", false, &list);
        assert!(f.pick(1).is_none());
        // Tools off: no tools section.
        f.on = false;
        assert_eq!(to_run_tools(&f, &list, 8, 60.0, 20_000), None);
    }

    #[test]
    fn servers_that_cannot_run_are_not_pickable() {
        let mut list = servers();
        assert_eq!(unpickable(&list[2]).as_deref(), Some("Its last check failed: Connection refused"));
        list[0].blocked = Some("Stdio servers are off here.".into());
        assert_eq!(unpickable(&list[0]).as_deref(), Some("Stdio servers are off here."));
        list[1].enabled = false;
        assert_eq!(unpickable(&list[1]).as_deref(), Some("Turned off in Tools."));
        let unchecked = server(4, "new", "unchecked", &[]);
        assert_eq!(unpickable(&unchecked).as_deref(), Some("Not checked yet."));
        // A pick of a server that went bad is not sent.
        let f = ToolsForm { on: true, picks: vec![Pick { server_id: 3, tools: None }], choice: ToolChoice::Auto };
        assert_eq!(to_run_tools(&f, &list, 8, 60.0, 20_000).unwrap().servers, vec![]);
        assert_eq!(summary(&f, &list), "No tools picked");
    }

    #[test]
    fn shared_names_are_found_among_the_offered_tools() {
        let list = servers();
        let mut f = ToolsForm { on: true, ..Default::default() };
        f.set_server(&list[0], true, &list);
        f.set_server(&list[1], true, &list);
        let found = clashes(&f, &list);
        assert_eq!(found, vec![Clash { tool: "search".into(), servers: vec!["docs".into(), "github".into()] }]);
        assert_eq!(clash_lines(&found), ["`search` is on docs and github: sent as `docs__search` and `github__search`."]);
        let many: Vec<Clash> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|t| Clash { tool: (*t).into(), servers: vec!["docs".into(), "local".into()] })
            .chain([Clash { tool: "x".into(), servers: vec!["a".into(), "b".into(), "c".into()] }])
            .collect();
        assert_eq!(
            clash_lines(&many),
            [
                "`a`, `b`, `c` and 2 more are on docs and local: each sent as `docs__<name>` and `local__<name>`.",
                "`x` is on a, b and c: sent as `a__x`, `b__x` and `c__x`.",
            ]
        );
        // Not offered from github: no clash.
        f.set_tool(&list[1], "search", false, &list);
        assert!(clashes(&f, &list).is_empty());
    }

    #[test]
    fn run_again_drops_what_is_gone() {
        let list = servers();
        let copied = RunTools {
            servers: vec![
                RunToolServer { server_id: 2, tools: Some(vec!["issues".into(), "wiki".into()]) },
                RunToolServer { server_id: 9, tools: None },
                RunToolServer { server_id: 3, tools: None },
                RunToolServer { server_id: 1, tools: None },
            ],
            tool_choice: ToolChoice::Required,
            ..RunTools::default()
        };
        let snapshot: ToolSnapshot = serde_json::from_value(json!({
            "servers": [{"server_id": 9, "name": "old-wiki", "transport": "stdio", "tools": []}],
            "definitions": []
        }))
        .unwrap();
        let (form, dropped) = from_run_tools(&copied, Some(&snapshot), &list);
        assert!(form.on);
        assert_eq!(form.choice, ToolChoice::Required);
        assert_eq!(form.picks, vec![Pick { server_id: 1, tools: None }, Pick { server_id: 2, tools: Some(vec!["issues".into()]) }]);
        assert_eq!(dropped, ["wiki of github (no longer listed)", "old-wiki (deleted)", "broken (last check failed)"]);
        assert_eq!(
            dropped_note(&dropped).as_deref(),
            Some("Not copied from the earlier run: wiki of github (no longer listed), old-wiki (deleted), broken (last check failed).")
        );
        assert_eq!(dropped_note(&[]), None);
    }

    #[test]
    fn the_settings_summary() {
        assert_eq!(settings_summary("8", "60", "20000", ToolChoice::Auto), "8 rounds, 60 s per call, 20,000 characters per result, auto");
        assert_eq!(settings_summary("1", "", "x", ToolChoice::Required), "1 round, a call required");
    }
}
