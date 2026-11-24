//! Everything about one MCP server, in a side sheet: the name and what it runs (with its copy
//! button); the status badge, when it was checked and what the server said it is; why it cannot
//! run here or why its check failed; the env variables or headers by name and hint; the tools,
//! each with its description and its input schema as a mono JSON block that opens (searchable from
//! 21 tools); Edit, Check now.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::list::checked_at;
use super::logic::{server_state, target, transport_label};
use crate::api::types::{McpServerOut as Server, McpStatus, McpTool, McpTransport};
use crate::format::int;
use crate::theme::text::{body, meta, mono, title};
use crate::theme::{muted_ink, radius, token};
use crate::ui::button::{Variant, button};
use crate::ui::field::text_field;
use crate::ui::icon::{self, icon};
use crate::ui::material::{dialog_muted, dialog_well};
use crate::ui::overlay::sheet_close;
use crate::ui::status::status_badge;
use crate::ui::tooltip::copy_button;

/// The sheet's content for `server`. `open` closes it; `on_edit` and `on_check` are the row
/// actions; `checking` holds while a check runs.
pub fn details(
    server: Server,
    open: Signal<bool>,
    on_edit: Rc<dyn Fn(Server)>,
    on_check: Rc<dyn Fn(Server)>,
    checking: impl Fn() -> bool + Clone + 'static,
) -> impl IntoElement {
    let state = server_state(&server);
    let line = target(&server);
    let header = view()
        .px(px(20.0))
        .pt(px(20.0))
        .pr(px(56.0))
        .pb(px(16.0))
        .gap(px(2.0))
        .child(title(text(server.name.clone())).role(AccessibilityRole::Header))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(2.0))
                .min_w(px(0.0))
                .child(mono(text(line.clone())).text_color(muted_ink).number_of_lines(2).flex_shrink(1.0).min_w(px(0.0)))
                .child(view().my(px(-6.0)).child(copy_button(
                    line,
                    if server.transport == McpTransport::Stdio { "Copy command" } else { "Copy URL" },
                ))),
        );

    let mut facts_line = vec![transport_label(server.transport).to_owned(), format!("Checked {}", checked_at(&server))];
    if let Some(info) = server.server_info.clone().filter(|i| !i.is_empty()) {
        facts_line.push(info);
    }
    let runs = server.run_count;
    if runs > 0 {
        facts_line.push(if runs == 1 { "Used by 1 run".to_owned() } else { format!("Used by {} runs", int(runs)) });
    }
    let reason_color = if server.blocked.is_some() { token("coral-ink") } else { token("negative-ink") };
    let secrets = if server.transport == McpTransport::Stdio { &server.env } else { &server.headers };
    let secrets_line = (!secrets.is_empty()).then(|| {
        let words = if server.transport == McpTransport::Stdio { "Environment " } else { "Headers " };
        let list = secrets
            .iter()
            .map(|s| match &s.hint {
                Some(h) => format!("{} {h}", s.name),
                None => s.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        body(text(words)).text_color(muted_ink).child(mono(text(list)).text_color(muted_ink))
    });
    let facts = view()
        .px(px(20.0))
        .gap(px(8.0))
        // The badge on its own line, the facts under it (they wrap in the sheet's width).
        .child(view().flex_row().child(status_badge(state.tone, None, state.word)))
        .child(body(text(facts_line.join(" · "))).text_color(muted_ink).tabular_nums())
        .child(state.reason.clone().map(|r| body(text(r)).text_color(reason_color).selectable(true)))
        .child(secrets_line);

    let tools_section = tools_section(&server);

    let s1 = server.clone();
    let s2 = server.clone();
    let blocked = server.blocked.is_some();
    let failing = server.status == McpStatus::Error;
    let c1 = checking.clone();
    let c2 = checking.clone();
    let footer = view()
        .flex_row()
        .flex_wrap()
        .justify_end()
        .gap(px(8.0))
        .px(px(20.0))
        .pt(px(16.0))
        .pb(px(20.0))
        .child(button("Edit").variant(Variant::Secondary).on_press(move || on_edit(s1.clone())))
        .child(
            button("Check now")
                .variant(if failing || server.status == McpStatus::Unchecked { Variant::Primary } else { Variant::Secondary })
                .busy(move || c1(), "Checking…")
                .disabled(move || c2() || blocked)
                .on_press(move || on_check(s2.clone())),
        );

    view()
        .flex_1()
        .min_h(px(0.0))
        .child(header)
        .child(facts)
        .child(tools_section)
        .child(footer)
        .child(sheet_close(open))
}

/// The tools: a box of rows (searchable from 21), or why there are none.
fn tools_section(server: &Server) -> impl IntoElement {
    let heading = body(text(if server.tools.is_empty() { "Tools".to_owned() } else { format!("Tools  {}", int(server.tools.len() as i64)) }))
        .font_weight(FontWeight::SEMIBOLD)
        .px(px(4.0))
        .role(AccessibilityRole::Header);
    let content: AnyElement = if server.tools.is_empty() {
        let words = match (server.blocked.is_some(), server.status) {
            (true, _) => "This server can’t run here, so its tools are not listed.",
            (false, McpStatus::Error) => "The last check failed, so the tool list is empty.",
            (false, McpStatus::Unchecked) => "Not checked yet. Check now lists its tools.",
            (false, McpStatus::Ok) => "The server lists no tools.",
        };
        body(text(words)).text_color(muted_ink).px(px(16.0)).py(px(12.0)).rounded(px(radius::XL)).bg(dialog_muted).into_element()
    } else {
        tool_list(Rc::new(server.tools.clone())).into_element()
    };
    view()
        .flex_1()
        .min_h(px(0.0))
        .gap(px(8.0))
        .px(px(20.0))
        .pt(px(24.0))
        .child(heading)
        .child(content)
}

/// The tools in a scrolling box, filtered by a search field when there are more than 20.
fn tool_list(tools: Rc<Vec<McpTool>>) -> impl IntoElement {
    let query = signal(String::new());
    let searchable = tools.len() > 20;
    let all = tools.clone();
    let shown = move || {
        let needle = query.get().trim().to_lowercase();
        all.iter()
            .filter(|t| {
                needle.is_empty()
                    || t.name.to_lowercase().contains(&needle)
                    || t.description.as_deref().is_some_and(|d| d.to_lowercase().contains(&needle))
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    view()
        .flex_1()
        .min_h(px(0.0))
        .gap(px(8.0))
        .child(searchable.then(|| text_field(query).search().placeholder("Search tools…").accessibility_label("Search tools")))
        .child(
            scroll().flex_shrink(1.0).min_h(px(0.0)).rounded(px(radius::XL)).bg(dialog_muted).child(view().p(px(8.0)).child(dynamic(move || {
                let list = shown();
                if list.is_empty() {
                    return body(text(format!("No tool matches “{}”.", query.get()))).text_color(muted_ink).px(px(8.0)).py(px(6.0)).into_element();
                }
                view()
                    .children(list.into_iter().enumerate().map(|(i, t)| tool_row(t, i).into_element()).collect::<Vec<_>>())
                    .into_element()
            }))),
        )
}

/// The line that opens and closes a schema: a chevron and its words ("Input · query, limit?").
fn schema_trigger(open: Signal<bool>, words: String, label: String) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .self_start()
        .mx(px(-4.0))
        .px(px(4.0))
        .min_h(px(24.0))
        .rounded(px(radius::XS))
        .role(AccessibilityRole::Button)
        .label(label)
        .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(move || open.update(|o| *o = !*o))
        .child(icon(icon::CHEVRON_RIGHT, 14.0, move || theme().muted_foreground).rotate(move || if open.get() { 90.0_f32.to_radians() } else { 0.0 }))
        .child(meta(text(words)).number_of_lines(1))
}

/// A tool: its name in mono at 500, its description (three lines, all once opened), and the input
/// schema as pretty mono JSON that opens and closes; the output schema the same way when the
/// server gave one (code mode types the result with it).
pub fn tool_row(tool: McpTool, index: usize) -> impl IntoElement {
    let open = signal(false);
    let open_output = signal(false);
    let schema = serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default();
    let fields = schema_fields(&tool.input_schema);
    let trigger = schema_trigger(
        open,
        if fields.is_empty() { "Input schema".to_owned() } else { format!("Input · {}", fields.join(", ")) },
        format!("Input schema of {}", tool.name),
    );
    let output = tool.output_schema.as_ref().map(|o| {
        let out_fields = schema_fields(o);
        let words = if out_fields.is_empty() { "Output schema".to_owned() } else { format!("Output · {}", out_fields.join(", ")) };
        (serde_json::to_string_pretty(o).unwrap_or_default(), schema_trigger(open_output, words, format!("Output schema of {}", tool.name)))
    });
    let (output_json, output_trigger) = match output {
        Some((json, trigger)) => (Some(json), Some(trigger)),
        None => (None, None),
    };
    let description = tool.description.clone().filter(|d| !d.trim().is_empty());
    let name = tool.name.clone();
    view()
        .px(px(8.0))
        .py(px(8.0))
        .gap(px(4.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(mono(text(tool.name.clone())).font_weight(FontWeight::MEDIUM).text_color(move || theme().foreground).selectable(true))
                .child(view().my(px(-6.0)).child(copy_button(name.clone(), format!("Copy {name}")))),
        )
        .child(description.map(|d| {
            body(text(d)).text_color(muted_ink).number_of_lines(move || if open.get() { 0_u32 } else { 3 }).selectable(true)
        }))
        .child(trigger)
        .child(when(move || open.get(), move || {
            json_well(schema.clone())
        }))
        .child(output_trigger)
        .child(output_json.map(|json| when(move || open_output.get(), move || json_well(json.clone()))))
        .child((index > 0).then(|| view().absolute().top(px(0.0)).left(px(8.0)).right(px(8.0)).h(px(1.0)).bg(move || theme().border)))
}

/// The argument names of a JSON schema (`properties`), the required ones first.
pub fn schema_fields(schema: &serde_json::Value) -> Vec<String> {
    let Some(props) = schema.get("properties").and_then(|p| p.as_object()) else { return Vec::new() };
    let required: Vec<&str> = schema.get("required").and_then(|r| r.as_array()).map(|a| a.iter().filter_map(|v| v.as_str()).collect()).unwrap_or_default();
    let mut names: Vec<String> = props.keys().filter(|k| required.contains(&k.as_str())).cloned().collect();
    names.extend(props.keys().filter(|k| !required.contains(&k.as_str())).map(|k| format!("{k}?")));
    names
}

/// JSON in mono in a ground well (8 px corners), selectable, at most 320 px tall.
pub fn json_well(json: String) -> impl IntoElement {
    scroll()
        .max_h(px(320.0))
        .rounded(px(radius::SM))
        .bg(dialog_well)
        .child(mono(text(json)).line_height(px(19.5)).selectable(true).px(px(12.0)).py(px(10.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_schemas_fields_put_the_required_first() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"limit": {"type": "integer"}, "query": {"type": "string"}},
            "required": ["query"]
        });
        assert_eq!(schema_fields(&schema), ["query", "limit?"]);
        assert!(schema_fields(&serde_json::json!({})).is_empty());
    }
}
