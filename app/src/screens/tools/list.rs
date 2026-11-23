//! The server list. From 1024 px a table: the name in ink over what it runs (the command or the
//! URL, in mono); the transport; the tool count; the status as a dot and a word with the reason
//! under it (a failed check, or why a stdio server is blocked here); when it was checked; Check now
//! and the "…" menu (Edit…, Check now, Turn off or on, Delete…). Below 1024 px one stacked row per
//! server with its facts on a second line. A press on a row opens the details sheet.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::logic::{server_state, target, tool_count, transport_label};
use crate::api::types::McpServerOut as Server;
use crate::data::now_utc;
use crate::format::{DASH, middle, relative};
use crate::theme::text::{body, label, meta, mono};
use crate::theme::{muted_ink, radius};
use crate::ui::blocks::bar;
use crate::ui::button::{Size, Variant, button};
use crate::ui::icon;
use crate::ui::page::window_width;
use crate::ui::select::{MenuEntry, menu_button, menu_destructive, menu_entry};
use crate::ui::status::{pill, pill_small};
use crate::ui::table::{Column, bound, cell, header, row};

/// What a row's controls do.
#[derive(Clone)]
pub struct Actions {
    pub open: Rc<dyn Fn(Server)>,
    pub edit: Rc<dyn Fn(Server)>,
    pub delete: Rc<dyn Fn(Server)>,
    pub check: Rc<dyn Fn(Server)>,
    pub toggle: Rc<dyn Fn(Server)>,
    /// The servers being checked now.
    pub checking: Signal<Vec<i64>>,
}

const SERVER: Column = Column::flex("Server");
const TRANSPORT: Column = Column::fixed("Transport", 96.0);
const TOOLS: Column = Column::fixed("Tools", 96.0);
const STATUS: Column = Column::fixed("Status", 224.0);
const CHECKED: Column = Column::fixed("Checked", 112.0);
const ACTIONS: Column = Column::fixed("", 144.0);
static COLUMNS: [Column; 6] = [SERVER, TRANSPORT, TOOLS, STATUS, CHECKED, ACTIONS];

/// The width from which the list is a table (Tailwind's `lg`).
const TABLE_FROM: f32 = 1024.0;

/// The list of `servers` (reactive), as a table or stacked rows by the window's width. A row is
/// made once per server; what changes in it (the status after Check now, the tools) is bound
/// inside its cells, so a reload never closes its open "…" menu.
pub fn server_list(servers: impl Fn() -> Vec<Server> + Copy + 'static, actions: Actions) -> impl IntoElement {
    let count_now = move || servers().len();
    let table = move || window_width() >= TABLE_FROM;
    let a1 = actions.clone();
    when(table, move || {
        let actions = a1.clone();
        view().child(header(&COLUMNS)).child(each(servers, |s: &Server| s.id, move |s, index| {
            let last = move || index.get() + 1 == count_now();
            table_row(s, actions.clone(), last)
        }))
    })
    .otherwise(move || {
        let actions = actions.clone();
        view().child(each(servers, |s: &Server| s.id, move |s, index| {
            let last = move || index.get() + 1 == count_now();
            stacked_row(s, actions.clone(), last)
        }))
    })
}

/// The status as a dot and a word, and the reason under it in 12 px muted text (two lines).
pub fn server_status(server: &Server) -> impl IntoElement {
    let state = server_state(server);
    view()
        .gap(px(2.0))
        .items_start()
        .min_w(px(0.0))
        .child(pill(state.tone, state.word))
        .child(state.reason.map(|r| meta(text(r)).number_of_lines(2)))
}

/// When it was last checked ("5 minutes ago"), or the dash.
pub fn checked_at(server: &Server) -> String {
    server.checked_at.map(|t| relative(t, now_utc())).unwrap_or_else(|| DASH.to_owned())
}

/// Check now (from 1024 px) and the "…" menu. Made once per row, and again on a rename or when the
/// server is turned off or on (the menu's words).
fn row_actions(s: ReadSignal<Server>, actions: Actions, with_check: bool) -> impl IntoElement {
    let id = s.with_untracked(|s| s.id);
    let key = computed(move || s.with(|s| (s.name.clone(), s.enabled, s.blocked.is_some())));
    dynamic(move || {
        let (name, enabled, blocked) = key.get();
        let checking = actions.checking;
        let busy = move || checking.with(|c| c.contains(&id));
        let a = actions.edit.clone();
        let mut entries: Vec<MenuEntry> = vec![menu_entry(Some(icon::PENCIL), "Edit…", move || a(s.get_untracked()))];
        let a = actions.check.clone();
        entries.push(menu_entry(Some(icon::REFRESH), "Check now", move || a(s.get_untracked())).disabled(move || busy() || blocked));
        let a = actions.toggle.clone();
        entries.push(menu_entry(None, if enabled { "Turn off" } else { "Turn on" }, move || a(s.get_untracked())));
        entries.push(MenuEntry::Separator);
        let a = actions.delete.clone();
        entries.push(menu_destructive(Some(icon::TRASH), "Delete…", move || a(s.get_untracked())));
        let check = with_check.then(|| {
            let a = actions.check.clone();
            button("Check now")
                .variant(Variant::Ghost)
                .size(Size::Sm)
                .accessibility_label(format!("Check {name} now"))
                .busy(busy, "Checking…")
                .disabled(move || blocked)
                .on_press(move || a(s.get_untracked()))
        });
        view()
            .flex_row()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(check)
            .child(menu_button(format!("More actions for {name}"), entries, Some(176.0)))
    })
}

/// The name (500, two lines at most) over its target in mono.
fn name_block(server: &Server) -> impl IntoElement {
    view()
        .flex_1()
        .min_w(px(0.0))
        .gap(px(2.0))
        .py(px(6.0))
        .child(label(text(server.name.clone())).number_of_lines(2))
        .child(mono(text(middle(&target(server), 56))).text_color(muted_ink).number_of_lines(1))
}

fn table_row(s: ReadSignal<Server>, actions: Actions, last: impl Fn() -> bool + 'static) -> AnyElement {
    let cells = vec![
        cell(SERVER, bound(s, |s| (s.name.clone(), target(s)), |s| name_block(s).into_element())).into_element(),
        cell(TRANSPORT, body(text(move || s.with(|s| transport_label(s.transport)))).text_color(muted_ink)).into_element(),
        cell(
            TOOLS,
            body(text(move || s.with(|s| tool_count(s).unwrap_or_else(|| DASH.to_owned()))))
                .tabular_nums()
                .text_color(move || if s.with(|s| tool_count(s).is_some()) { theme().foreground } else { muted_ink() }),
        )
        .into_element(),
        cell(
            STATUS,
            view().py(px(4.0)).min_w(px(0.0)).child(bound(
                s,
                |s| (s.status, s.error.clone(), s.blocked.clone(), s.enabled),
                |s| server_status(s).into_element(),
            )),
        )
        .into_element(),
        cell(CHECKED, body(text(move || s.with(checked_at))).text_color(muted_ink).number_of_lines(1)).into_element(),
        cell(ACTIONS, view().flex_1().child(row_actions(s, actions.clone(), true))).into_element(),
    ];
    let open = actions.open.clone();
    row(cells, last)
        .cursor(Cursor::Pointer)
        .role(AccessibilityRole::Button)
        .label(move || SharedString::from(s.with(|s| s.name.clone())))
        .opacity(move || if s.with(|s| s.enabled) { 1.0_f32 } else { 0.6 })
        .on_press(move || open(s.get_untracked()))
        .into_element()
}

/// The line of facts under a stacked row's name: the status, the transport, the tool count.
fn stacked_facts(server: &Server) -> impl IntoElement {
    let state = server_state(server);
    let mut more = vec![transport_label(server.transport).to_owned()];
    if let Some(n) = tool_count(server) {
        more.push(n);
    }
    view()
        .gap(px(2.0))
        .min_w(px(0.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .min_w(px(0.0))
                .child(pill_small(state.tone, state.word))
                .child(meta(text(format!(" · {}", more.join(" · ")))).tabular_nums().number_of_lines(1)),
        )
        .child(state.reason.map(|r| meta(text(r)).number_of_lines(2)))
}

/// Below 1024 px: the name, the target and a line of facts; the menu at the end.
fn stacked_row(s: ReadSignal<Server>, actions: Actions, last: impl Fn() -> bool + 'static) -> AnyElement {
    let open = actions.open.clone();
    view()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .min_h(px(56.0))
                .gap(px(2.0))
                .mx(px(-8.0))
                .px(px(8.0))
                .py(px(10.0))
                .rounded(px(radius::MD))
                .role(AccessibilityRole::Button)
                .label(move || SharedString::from(s.with(|s| s.name.clone())))
                .cursor(Cursor::Pointer)
                .opacity(move || if s.with(|s| s.enabled) { 1.0_f32 } else { 0.6 })
                .active(|st| st.bg(move || theme().accent))
                .on_press(move || open(s.get_untracked()))
                .child(label(text(move || s.with(|s| s.name.clone()))).number_of_lines(2))
                .child(mono(text(move || s.with(|s| middle(&target(s), 40)))).text_color(muted_ink).number_of_lines(1))
                .child(bound(
                    s,
                    |s| (s.status, s.error.clone(), s.blocked.clone(), s.enabled, s.tools.len(), s.transport),
                    |s| stacked_facts(s).into_element(),
                )),
        )
        .child(row_actions(s, actions, false))
        .child(
            view()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .h(px(1.0))
                .bg(move || theme().border)
                .opacity(move || if last() { 0.0_f32 } else { 1.0 }),
        )
        .into_element()
}

/// Loading rows at the real pitch and column positions.
pub fn list_skeleton() -> impl IntoElement {
    let table = window_width() >= TABLE_FROM;
    let rows = (0..3).map(move |_| {
        view()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .h(px(if table { 56.0 } else { 64.0 }))
            .px(px(if table { 12.0 } else { 0.0 }))
            .border_b_1()
            .border_color(move || theme().border)
            .child(view().flex_1().gap(px(6.0)).child(bar(128.0, 16.0)).child(bar(224.0, 12.0)))
            .child(table.then(|| bar(64.0, 16.0)))
            .child(table.then(|| bar(96.0, 16.0)))
            .into_element()
    });
    view()
        .role(AccessibilityRole::Progressbar)
        .label("Loading MCP servers")
        .child(table.then(|| view().h(px(32.0)).rounded(px(radius::MD)).bg(move || theme().muted)))
        .children(rows.collect::<Vec<_>>())
}
