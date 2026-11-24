//! The run's tools in its chip row: a chip "5 tools" whose popover lists the servers the run sends
//! tools from (as they were when the run was made) and each tool's name as the model sees it, with
//! the server's own name after a renamed one (`docs__search`, search).
//!
//! In code mode the chip says "Code mode · 5 tools". Its popover names the code tool and lists the
//! namespaces the code sees, each with its methods (`lookup.search`, and the tool's own name after
//! one that was renamed: JavaScript names cannot hold a `-`), and has a Types button: the
//! TypeScript declarations the model was given, in a mono block with a copy button.

use nsh_base::popover;
use nsh_core::prelude::*;

use crate::api::types::{CodeNamespace, CodeToolSnapshot, ToolMode, ToolSnapshot};
use crate::format::count;
use crate::screens::tools::logic::transport_label;
use crate::theme::radius;
use crate::theme::text::{body, label, meta, mono};
use crate::ui::button::{Size, Variant, button};
use crate::ui::chip::chip;
use crate::ui::material::{dialog_well, material_float, material_thick};
use crate::ui::page::window_width;
use crate::ui::tooltip::copy_button;

/// How many tools the run sends.
pub fn tool_total(snapshot: &ToolSnapshot) -> usize {
    snapshot.servers.iter().map(|s| s.tools.len()).sum()
}

/// The chip's words: "5 tools", or in code mode "Code mode · 5 tools".
pub fn chip_label(snapshot: &ToolSnapshot) -> String {
    let tools = count(tool_total(snapshot) as i64, "tool", "tools");
    if snapshot.mode == ToolMode::Code { format!("Code mode · {tools}") } else { tools }
}

/// "3 methods", for a namespace.
pub fn methods_words(ns: &CodeNamespace) -> String {
    count(ns.methods.len() as i64, "method", "methods")
}

/// Whether the methods of `ns` come from more than one server (all tools under `codemode`), so each
/// method says which.
pub fn mixed_servers(ns: &CodeNamespace) -> bool {
    ns.methods.iter().any(|m| m.server_id != ns.methods[0].server_id)
}

/// The chip and its popover.
pub fn tools_chip(snapshot: ToolSnapshot) -> impl IntoElement {
    let trigger = chip(chip_label(&snapshot)).popover_trigger();
    let code = snapshot.code.clone().filter(|_| snapshot.mode == ToolMode::Code);
    let types = signal(false);
    let content: AnyElement = match code {
        Some(code) => code_content(code, types).into_element(),
        None => direct_content(&snapshot).into_element(),
    };
    // 400 px in code mode (the declarations are wide), never wider than a phone with its margins.
    let width = (if snapshot.mode == ToolMode::Code { 400.0 } else { 320.0_f32 }).min(window_width() - 32.0);
    let panel = material_float(material_thick(view().w(px(width)).max_h(px(420.0)).rounded(px(radius::LG)).overflow_hidden()))
        .role(AccessibilityRole::Dialog)
        .label("Tools this run sends")
        .child(scroll().child(view().p(px(14.0)).gap(px(16.0)).child(content)));
    popover().placement(Placement::Bottom).align(Align::Start).offset(px(6.0)).margin(px(16.0)).trigger(trigger).child(panel)
}

/// Direct mode: each server and its tools.
fn direct_content(snapshot: &ToolSnapshot) -> impl IntoElement {
    let servers: Vec<AnyElement> = snapshot
        .servers
        .iter()
        .map(|s| {
            let tools: Vec<AnyElement> = s
                .tools
                .iter()
                .map(|t| {
                    view()
                        .flex_row()
                        .items_baseline()
                        .gap(px(8.0))
                        .min_w(px(0.0))
                        .child(mono(text(t.name.clone())).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
                        .child((t.name != t.server_tool).then(|| meta(text(t.server_tool.clone())).number_of_lines(1)))
                        .into_element()
                })
                .collect();
            view()
                .gap(px(6.0))
                .child(
                    view()
                        .flex_row()
                        .items_baseline()
                        .gap(px(8.0))
                        .child(label(text(s.name.clone())).number_of_lines(1).flex_shrink(1.0))
                        .child(meta(text(format!("{} · {}", transport_label(s.transport), count(s.tools.len() as i64, "tool", "tools"))))),
                )
                .child(view().gap(px(4.0)).children(tools))
                .into_element()
        })
        .collect();
    view().gap(px(16.0)).children(servers)
}

/// Code mode: the code tool, its namespaces and methods; the Types button swaps them for the
/// generated declarations.
fn code_content(code: CodeToolSnapshot, types: Signal<bool>) -> impl IntoElement {
    let names = code.tool_name.clone();
    let methods = code.namespaces.clone();
    let declarations = code.types.clone();
    let format = code.format.clone();
    let head = view()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            view()
                .flex_row()
                .items_baseline()
                .gap(px(8.0))
                .min_w(px(0.0))
                .child(mono(text(names)).font_weight(FontWeight::MEDIUM).number_of_lines(1).flex_shrink(1.0))
                .child(meta(text("runs JavaScript")).number_of_lines(1)),
        )
        .child(dynamic(move || {
            let showing = types.get();
            button(if showing { "Methods" } else { "Types" })
                .variant(Variant::Secondary)
                .size(Size::Sm)
                .on_press(move || types.update(|t| *t = !*t))
        }));
    let body_el = dynamic(move || -> AnyElement {
        if types.get() { types_block(declarations.clone(), format.clone()).into_element() } else { namespaces_list(&methods).into_element() }
    });
    view().gap(px(12.0)).child(head).child(body_el)
}

/// Each namespace with its methods as the code calls them.
fn namespaces_list(namespaces: &[CodeNamespace]) -> impl IntoElement {
    let rows: Vec<AnyElement> = namespaces
        .iter()
        .map(|ns| {
            let mixed = mixed_servers(ns);
            let methods: Vec<AnyElement> = ns
                .methods
                .iter()
                .map(|m| {
                    // After the method: the server when the namespace mixes several, and the
                    // tool's own name when it differs (`list_issues`, from `list-issues`).
                    let mut notes: Vec<String> = Vec::new();
                    if mixed {
                        notes.push(m.server.clone());
                    }
                    if m.name != m.tool {
                        notes.push(m.tool.clone());
                    }
                    view()
                        .flex_row()
                        .items_baseline()
                        .gap(px(8.0))
                        .min_w(px(0.0))
                        .child(mono(text(format!("{}.{}", ns.name, m.name))).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
                        .child((!notes.is_empty()).then(|| meta(text(notes.join(" · "))).number_of_lines(1).flex_shrink(2.0).min_w(px(0.0))))
                        .into_element()
                })
                .collect();
            view()
                .gap(px(6.0))
                .child(
                    view()
                        .flex_row()
                        .items_baseline()
                        .gap(px(8.0))
                        .child(label(text(ns.name.clone())).font_family(crate::theme::fonts::ROBOTO_MONO.family).number_of_lines(1).flex_shrink(1.0))
                        .child(meta(text(methods_words(ns)))),
                )
                .child(view().gap(px(4.0)).children(methods))
                .into_element()
        })
        .collect();
    view().gap(px(16.0)).children(rows)
}

/// The declarations in a mono block (a well, 8 px corners) with a copy button, and the release of
/// Cloudflare's Code Mode they follow.
fn types_block(declarations: String, format: String) -> impl IntoElement {
    view()
        .gap(px(8.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .child(body(text("Types the model was given")).font_weight(FontWeight::SEMIBOLD))
                .child(copy_button(declarations.clone(), "Copy types")),
        )
        .child(
            view()
                .rounded(px(radius::SM))
                .bg(dialog_well)
                .px(px(12.0))
                .py(px(10.0))
                .child(mono(text(declarations)).line_height(px(19.5)).selectable(true)),
        )
        .child(meta(text(format)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(mode: &str) -> ToolSnapshot {
        let mut v = json!({
            "servers": [
                {"server_id": 1, "name": "lookup", "transport": "http", "tools": [{"name": "lookup.search", "server_tool": "search"}, {"name": "lookup.fetch", "server_tool": "fetch"}]},
                {"server_id": 2, "name": "gh", "transport": "stdio", "tools": [{"name": "gh.list_issues", "server_tool": "list-issues"}]}
            ],
            "definitions": [], "mode": mode
        });
        if mode == "code" {
            v["code"] = json!({
                "tool_name": "codemode", "types": "declare const lookup: {};", "format": "@cloudflare/codemode 0.5.6",
                "namespaces": [
                    {"name": "lookup", "methods": [{"name": "search", "server_id": 1, "server": "lookup", "tool": "search"}, {"name": "fetch", "server_id": 1, "server": "lookup", "tool": "fetch"}]},
                    {"name": "gh", "methods": [{"name": "list_issues", "server_id": 2, "server": "gh", "tool": "list-issues"}]}
                ]
            });
        }
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn the_chip_says_the_mode_and_counts_the_tools() {
        assert_eq!(chip_label(&snapshot("direct")), "3 tools");
        assert_eq!(chip_label(&snapshot("code")), "Code mode · 3 tools");
    }

    #[test]
    fn a_namespace_says_how_many_methods_and_whether_servers_are_mixed() {
        let code = snapshot("code").code.unwrap();
        assert_eq!(methods_words(&code.namespaces[0]), "2 methods");
        assert_eq!(methods_words(&code.namespaces[1]), "1 method");
        assert!(!mixed_servers(&code.namespaces[0]));
        let mut all = code.namespaces[0].clone();
        all.methods.extend(code.namespaces[1].methods.clone());
        assert!(mixed_servers(&all));
    }
}
