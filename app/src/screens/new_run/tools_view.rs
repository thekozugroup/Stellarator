//! New run's Tools group (after Sampling): off by default.
//!
//! - With no MCP servers saved, only a link: "Add MCP servers in Tools".
//! - "Use tools" (a switch). On: each server with its switch and how many of its tools are picked;
//!   the chevron lists its tools with a checkbox each and their descriptions. A server that cannot
//!   run (blocked here, off, its last check failed, not checked) has its switch dimmed and says why;
//!   one not checked yet has Check now. Under the servers, "5 tools from 2 servers", and a line for
//!   each tool name two picked servers share: it is sent as `server__tool`.
//! - Tool settings (a group that opens): max rounds (a stepper), the tool timeout, the most
//!   characters of a result, and Auto | Required.
//! - The footnote: the endpoint must support tool calling, with a link to docs/tools.md.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::logic::Field;
use super::state::{Data, State};
use super::tools::{clash_lines, clashes, offered, settings_summary, summary, unpickable};
use crate::api::types::{McpStatus, ToolChoice};
use crate::data::{Load, call};
use crate::theme::text::{body, label, meta, mono};
use crate::theme::{muted_ink, token};
use crate::ui::blocks::error_block;
use crate::ui::button::{Button, Size, Variant, external_link, text_link};
use crate::ui::choice::{checkbox_content, segmented_with_get, switch_with};
use crate::ui::field::{TextField, form_disclosure, form_group, form_item, form_row, text_field};
use crate::ui::icon::{self, icon};
use crate::ui::overlay::toast_error;
use crate::ui::stepper::number_stepper;
use crate::ui::trace::code_spans;

/// The guide to tools, on GitHub.
const DOCS: &str = "https://github.com/thekozugroup/Stellarator/blob/main/docs/tools.md";

/// The group, what stands in its place, and the settings that open under it.
pub fn tools_section(state: State, data: Data, submit: &Rc<dyn Fn()>) -> impl IntoElement {
    let submit = submit.clone();
    // What decides the section's structure: whether the servers loaded, and whether there are any.
    let shape = computed(move || {
        data.servers.with(|l| match l {
            Load::Loading => 0,
            Load::Failed(_) => 1,
            Load::Loaded(v) if v.is_empty() => 2,
            Load::Loaded(_) => 3,
        })
    });
    let note = dynamic(move || state.tools_note.get().map(|n| meta(text(n)).text_color(token("coral-ink")).px(px(12.0))));
    let body_el = dynamic(move || -> AnyElement {
        match shape.get() {
            0 => view().into_element(),
            1 => {
                let message = data.servers.with_untracked(|l| l.error().unwrap_or_default().to_owned());
                view()
                    .gap(px(6.0))
                    .child(group_title())
                    .child(error_block(
                        format!("Couldn’t read the MCP servers. {message}"),
                        Some(Rc::new(move || {
                            data.servers.set(Load::Loading);
                            call(|api| async move { api.mcp_servers().await }, move |r| data.servers.set(Load::from(r)));
                        })),
                    ))
                    .into_element()
            }
            2 => view()
                .px(px(12.0))
                .child(text_link("Add MCP servers in Tools", None, crate::screens::tools::open))
                .into_element(),
            _ => group(state, data, submit.clone()).into_element(),
        }
    });
    view().gap(px(8.0)).element_ref(&state.element(Field::Tools)).child(body_el).child(note)
}

fn group_title() -> impl IntoElement {
    body(text("Tools")).px(px(12.0)).font_weight(FontWeight::SEMIBOLD).text_color(muted_ink).role(AccessibilityRole::Header)
}

/// The group with servers to pick from.
fn group(state: State, data: Data, submit: Rc<dyn Fn()>) -> impl IntoElement {
    let on = computed(move || state.tools.with(|t| t.on));
    // In a row as tall as a control, so it lines up with its label.
    let use_row = form_row(
        "Use tools",
        view().h(px(32.0)).flex_row().items_center().child(switch_with(
            "",
            move || on.get(),
            move |next| {
                state.tools.update(|t| t.on = next);
                state.error(Field::Tools).set(None);
            },
            || false,
            "Use tools",
        )),
    )
    .align_end();
    // The servers' rows come and go with the servers (their ids and whether each can be picked),
    // not with the picks: the switches and boxes follow the picks in their own bindings.
    let rows_shape = computed(move || data.servers.with(|l| l.loaded().map(|v| v.iter().map(|s| (s.id, unpickable(s).is_some())).collect::<Vec<_>>()).unwrap_or_default()));
    let rows = move || -> Vec<AnyElement> {
        if !on.get() {
            return Vec::new();
        }
        let mut out: Vec<AnyElement> = rows_shape.get().into_iter().map(|(id, _)| server_row(state, data, id).into_element()).collect();
        out.push(totals_row(state, data).into_element());
        out
    };
    let footnote = view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_x(px(6.0))
        .child(meta(text("The endpoint must support tool calling.")))
        .child(external_link("About tools", DOCS));
    view()
        .gap(px(8.0))
        .child(form_group(vec![use_row.into_element()]).title("Tools").dynamic_rows(rows).footnote(footnote))
        .child(when(move || on.get(), move || settings(state, submit.clone())))
}

/// A server: its switch, its name, what it offers or why it cannot run, and the chevron that lists
/// its tools.
fn server_row(state: State, data: Data, id: i64) -> impl IntoElement {
    let server = move || data.servers.with(|l| l.loaded().and_then(|v| v.iter().find(|s| s.id == id).cloned()));
    let all = move || data.server_list();
    let Some(first) = untracked(server) else { return view().into_element() };
    let blocked = unpickable(&first);
    let can_pick = blocked.is_none();
    let expanded = move || state.expanded.with(|e| e.contains(&id));
    let picked = move || state.tools.with(|t| t.pick(id).is_some());
    let name = first.name.clone();
    let switch = switch_with(
        "",
        picked,
        move |next| {
            if let Some(s) = untracked(server) {
                let order = untracked(all);
                state.tools.update(|t| t.set_server(&s, next, &order));
                state.error(Field::Tools).set(None);
            }
        },
        move || !can_pick,
        format!("Use {name}"),
    );
    // "4 of 12 tools", or why it cannot run.
    let detail = dynamic(move || -> AnyElement {
        let Some(s) = server() else { return view().into_element() };
        if let Some(why) = unpickable(&s) {
            let negative = s.status == McpStatus::Error;
            return meta(text(why))
                .text_color(move || if negative { token("negative-ink")() } else { muted_ink() })
                .number_of_lines(2)
                .into_element();
        }
        let n = state.tools.with(|t| t.pick(id).map(|p| offered(p, &s).len()));
        let total = s.tools.len();
        let words = match n {
            Some(k) if k == total => format!("All {total} tools"),
            Some(k) => format!("{k} of {total} tools"),
            None => if total == 1 { "1 tool".to_owned() } else { format!("{total} tools") },
        };
        meta(text(words)).tabular_nums().into_element()
    });
    let unchecked = first.status == McpStatus::Unchecked && first.tools.is_empty() && first.blocked.is_none() && first.enabled;
    let check = unchecked.then(|| {
        let busy = move || state.checking.with(|c| c.contains(&id));
        crate::ui::button::button("Check now")
            .variant(Variant::Secondary)
            .on_card()
            .size(Size::Sm)
            .busy(busy, "Checking…")
            .on_press(move || check_server(state, data, id))
    });
    let chevron = can_pick.then(|| {
        Button::icon(icon::CHEVRON_DOWN, format!("Tools of {}", first.name))
            .variant(Variant::Ghost)
            .size(Size::IconSm)
            .on_press(move || {
                state.expanded.update(|e| {
                    if e.contains(&id) {
                        e.retain(|x| *x != id);
                    } else {
                        e.push(id);
                    }
                })
            })
    });
    let head = view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .child(switch)
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .gap(px(2.0))
                .opacity(if can_pick { 1.0_f32 } else { 0.7 })
                .child(label(text(first.name.clone())).number_of_lines(1))
                .child(detail),
        )
        .child(check)
        .child(chevron.map(|c| view().rotate(move || if expanded() { 180.0_f32.to_radians() } else { 0.0 }).child(c)));
    form_item(
        view()
            .gap(px(8.0))
            .child(head)
            .child(when(expanded, move || tool_list(state, data, id))),
    )
    .into_element()
}

/// A server's tools: a checkbox each with its name in mono and its description under it (two
/// lines), indented to the server's name.
fn tool_list(state: State, data: Data, id: i64) -> impl IntoElement {
    let Some(server) = untracked(|| data.servers.with(|l| l.loaded().and_then(|v| v.iter().find(|s| s.id == id).cloned()))) else {
        return view().into_element();
    };
    let order = untracked(|| data.server_list());
    let s = Rc::new(server.clone());
    let order = Rc::new(order);
    let rows: Vec<AnyElement> = server
        .tools
        .iter()
        .map(|tool| {
            let (s1, s2, o) = (s.clone(), s.clone(), order.clone());
            let (n1, n2) = (tool.name.clone(), tool.name.clone());
            let words = view()
                .gap(px(2.0))
                .child(mono(text(tool.name.clone())).text_color(move || theme().foreground))
                .child(tool.description.clone().filter(|d| !d.trim().is_empty()).map(|d| meta(text(d)).number_of_lines(2)));
            checkbox_content(
                move || state.tools.with(|t| t.has_tool(&s1, &n1)),
                move |on| {
                    state.tools.update(|t| t.set_tool(&s2, &n2, on, &o));
                    state.error(Field::Tools).set(None);
                },
                words,
                tool.name.clone(),
            )
            .into_element()
        })
        .collect();
    view()
        .ml(px(44.0))
        .gap(px(8.0))
        .pb(px(4.0))
        .role(AccessibilityRole::Group)
        .label(format!("Tools of {}", server.name))
        .children(rows)
        .into_element()
}

/// Checks a server from the group (one not checked yet), then shows its tools.
fn check_server(state: State, data: Data, id: i64) {
    if state.checking.with_untracked(|c| c.contains(&id)) {
        return;
    }
    state.checking.update(|c| c.push(id));
    call(move |api| async move { api.check_mcp_server(id).await }, move |result| {
        state.checking.update(|c| c.retain(|x| *x != id));
        match result {
            Ok(server) => {
                if server.status == McpStatus::Error {
                    toast_error(format!("{}: {}", server.name, server.error.clone().unwrap_or_else(|| "the check failed.".into())));
                }
                data.servers.update(|l| {
                    if let Load::Loaded(list) = l {
                        for s in list.iter_mut().filter(|s| s.id == server.id) {
                            *s = server.clone();
                        }
                    }
                });
            }
            Err(e) => toast_error(e.message),
        }
    });
}

/// "5 tools from 2 servers" (or the group's error), and a line for each name two servers share.
fn totals_row(state: State, data: Data) -> impl IntoElement {
    form_item(dynamic(move || {
        let servers = data.server_list();
        let form = state.tools.get();
        let error = state.error(Field::Tools).get();
        let found = clashes(&form, &servers);
        let head: AnyElement = match error {
            Some(e) => body(text(e)).text_color(token("negative-ink")).role(AccessibilityRole::Alert).into_element(),
            None => body(text(summary(&form, &servers))).text_color(muted_ink).tabular_nums().into_element(),
        };
        let lines: Vec<AnyElement> = clash_lines(&found)
            .into_iter()
            .map(|words| {
                view()
                    .flex_row()
                    .items_start()
                    .gap(px(8.0))
                    .child(icon(icon::INFO, 14.0, token("coral-ink")).mt(px(2.0)))
                    .child(code_spans(&words, 13.0).text_size(px(13.0)).line_height(px(18.0)).text_color(token("coral-ink")).flex_1().min_w(px(0.0)))
                    .into_element()
            })
            .collect();
        view().gap(px(6.0)).child(head).children(lines).into_element()
    }))
}

/// The Tool settings group: max rounds, the timeout, a result's length, the tool choice.
fn settings(state: State, submit: Rc<dyn Fn()>) -> impl IntoElement {
    let rows = move || {
        let s = submit.clone();
        let number = move |field: Field| -> TextField {
            let s = s.clone();
            text_field(state.value(field))
                .width(112.0)
                .tabular()
                .invalid(move || state.has_error(field))
                .on_change(move |_| state.error(field).set(None))
                .on_blur(move || state.blur(field))
                .on_submit(move || s())
                .element(&state.element(field))
        };
        let s2 = submit.clone();
        vec![
            form_row(
                "Max rounds",
                number_stepper(state.value(Field::MaxRounds), "rounds", 1, 32)
                    .on_card()
                    .invalid(move || state.has_error(Field::MaxRounds))
                    .on_change(move |_| state.error(Field::MaxRounds).set(None))
                    .on_blur(move || state.blur(Field::MaxRounds))
                    .on_submit(move || s2())
                    .element(&state.element(Field::MaxRounds)),
            )
            .align_end()
            .hint("Rounds of tool calls per sample, 1–32.")
            .error(state.error(Field::MaxRounds))
            .into_element(),
            form_row("Tool timeout (s)", number(Field::ToolTimeout))
                .align_end()
                .hint("Per call. A late call returns an error to the model.")
                .error(state.error(Field::ToolTimeout))
                .into_element(),
            form_row("Max result characters", number(Field::MaxResultChars))
                .align_end()
                .hint("Longer results are cut, with a marker.")
                .error(state.error(Field::MaxResultChars))
                .into_element(),
            form_row(
                "Tool choice",
                segmented_with_get(
                    vec![(ToolChoice::Auto, "Auto"), (ToolChoice::Required, "Required")],
                    move || state.tools.with(|t| t.choice),
                    move |v| state.tools.update(|t| t.choice = v),
                    "Tool choice",
                    false,
                ),
            )
            .align_end()
            .hint_when(move || {
                Some(
                    match state.tools.with(|t| t.choice) {
                        ToolChoice::Auto => "The model decides.",
                        ToolChoice::Required => "The first request of each sample must call a tool.",
                    }
                    .into(),
                )
            })
            .into_element(),
        ]
    };
    form_disclosure(
        "Tool settings",
        move || {
            settings_summary(
                &state.get(Field::MaxRounds),
                &state.get(Field::ToolTimeout),
                &state.get(Field::MaxResultChars),
                state.tools.with(|t| t.choice),
            )
        },
        state.tools_open,
        rows,
        false,
    )
}
