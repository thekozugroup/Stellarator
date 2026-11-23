//! Tools: the MCP servers whose tools a run can offer the model. The list (a table from 1024 px),
//! Add (Paste config, or a form), the details sheet a row opens, Check now, Turn off or on, and the
//! delete alert, which names the runs that used the server.
//!
//! With no servers, the page itself is the paste box: paste the `mcpServers` JSON of Claude
//! Desktop, Cursor or VS Code and Import adds and checks each server; the answer shows in the Add
//! dialog. The screen keeps the list in a `Signal<Load<…>>` and reads it again after each change.

mod details;
mod form;
mod list;
pub mod logic;

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use self::form::{DialogState, Mode, can_import, import, paste_box, server_dialog};
use self::list::{Actions, list_skeleton, server_list};
use self::logic::{delete_description, runs_using};
use crate::api::types::{McpServerOut as Server, McpServerPatch, McpStatus};
use crate::data::{Load, call};
use crate::format::count;
use crate::nav::{self, Destination};
use crate::theme::text::body;
use crate::theme::radius;
use crate::ui::blocks::{empty_block, error_block};
use crate::ui::button::{Button, Size, Variant, button, text_link};
use crate::ui::icon;
use crate::ui::overlay::{Alert, DialogWidth, alert, dialog, side_sheet, toast_error, toast_success};
use crate::ui::page::page;
use crate::ui::plasma::plasma;


/// Shows the Tools destination (New run's "Add MCP servers in Tools").
pub fn open() {
    nav::open(Destination::Tools);
}

/// The Tools screen.
pub fn screen() -> impl IntoElement {
    let load = signal(Load::<Vec<Server>>::Loading);
    let reload = move || call(|api| async move { api.mcp_servers().await }, move |r| load.set(Load::from(r)));
    reload();
    // Whether stdio servers can be added here; until the settings answer, as on the desktop.
    let stdio_allowed = signal(true);
    call(|api| async move { api.settings().await }, move |r| {
        if let Ok(s) = r {
            stdio_allowed.set(s.stdio_mcp_allowed);
        }
    });

    let servers = move || load.with(|l| l.loaded().cloned().unwrap_or_default());
    let has_servers = move || load.with(|l| l.loaded().is_some_and(|v| !v.is_empty()));

    let dialog_state = DialogState::new();
    let details_id = signal(None::<i64>);
    let sheet_open = signal(false);
    let deleting = signal(None::<Server>);
    let delete_runs = signal(Vec::<String>::new());
    let delete_open = signal(false);
    let delete_busy = signal(false);
    let delete_error = signal(None::<String>);
    let checking = signal(Vec::<i64>::new());

    // A server the server sent back: shown in place at once.
    let replace = move |server: Server| {
        load.update(|l| {
            if let Load::Loaded(list) = l {
                for x in list.iter_mut().filter(|x| x.id == server.id) {
                    *x = server.clone();
                }
            }
        });
    };

    // Check now: connect, list the tools, keep them; say how it went.
    let check: Rc<dyn Fn(Server)> = Rc::new(move |s: Server| {
        if checking.with_untracked(|c| c.contains(&s.id)) {
            return;
        }
        checking.update(|c| c.push(s.id));
        let id = s.id;
        call(move |api| async move { api.check_mcp_server(id).await }, move |result| {
            checking.update(|c| c.retain(|x| *x != id));
            match result {
                Ok(server) => {
                    match server.status {
                        McpStatus::Ok => toast_success(format!("{}: {}.", server.name, count(server.tools.len() as i64, "tool", "tools"))),
                        _ => toast_error(format!("{}: {}", server.name, server.error.clone().unwrap_or_else(|| "the check failed.".into()))),
                    }
                    replace(server);
                }
                Err(e) => toast_error(e.message),
            }
        });
    });
    // Turn off or on: a server that is off is never offered to a run.
    let toggle: Rc<dyn Fn(Server)> = Rc::new(move |s: Server| {
        let id = s.id;
        let body = McpServerPatch { enabled: Some(!s.enabled), ..Default::default() };
        call(move |api| async move { api.update_mcp_server(id, &body).await }, move |result| match result {
            Ok(server) => {
                toast_success(if server.enabled { format!("{} is on.", server.name) } else { format!("{} is off. New runs can’t use it.", server.name) });
                replace(server);
            }
            Err(e) => toast_error(e.message),
        });
    });
    let actions = Actions {
        open: Rc::new(move |s: Server| {
            details_id.set(Some(s.id));
            sheet_open.set(true);
        }),
        edit: Rc::new(move |s: Server| dialog_state.edit(s)),
        delete: Rc::new(move |s: Server| {
            delete_error.set(None);
            delete_runs.set(Vec::new());
            let id = s.id;
            // The runs it was used by, named in the alert once they are read.
            if s.run_count > 0 {
                call(|api| async move { api.runs().await }, move |r| {
                    if let Ok(runs) = r {
                        delete_runs.set(runs_using(&runs, id));
                    }
                });
            }
            deleting.set(Some(s));
            delete_open.set(true);
        }),
        check: check.clone(),
        toggle,
        checking,
    };

    let imported: Rc<dyn Fn()> = Rc::new(reload);
    let saved: Rc<dyn Fn(Server)> = Rc::new(move |_| reload());

    // What decides the page's structure, and only that: a reload or Check now changes the rows in
    // place, so an open row menu stays open.
    let state = computed(move || {
        load.with(|l| match l {
            Load::Loading => 0,
            Load::Failed(_) => 1,
            Load::Loaded(v) if v.is_empty() => 2,
            Load::Loaded(_) => 3,
        })
    });
    let content = dynamic({
        let actions = actions.clone();
        let imported = imported.clone();
        move || -> AnyElement {
            match state.get() {
                0 => list_skeleton().into_element(),
                1 => {
                    let message = load.with_untracked(|l| l.error().unwrap_or_default().to_owned());
                    error_block(format!("Couldn’t load MCP servers. {message}"), Some(Rc::new(move || {
                        load.set(Load::Loading);
                        reload();
                    })))
                    .into_element()
                }
                2 => empty_page(dialog_state, stdio_allowed, imported.clone()).into_element(),
                _ => server_list(servers, actions.clone()).into_element(),
            }
        }
    });

    let form = dialog(dialog_state.open, "MCP server", move || {
        server_dialog(dialog_state, stdio_allowed.get_untracked(), imported.clone(), saved.clone()).into_element()
    })
    .width(DialogWidth::Xl)
    .gap(0.0)
    .busy(move || dialog_state.busy.get());

    let details_actions = actions.clone();
    let sheet = side_sheet(sheet_open, "MCP server details", move || {
        let Some(id) = details_id.get() else { return view().into_element() };
        let Some(server) = load.with(|l| l.loaded().and_then(|v| v.iter().find(|s| s.id == id).cloned())) else {
            return view().into_element();
        };
        let edit = details_actions.edit.clone();
        let on_edit: Rc<dyn Fn(Server)> = Rc::new(move |s: Server| {
            sheet_open.set(false);
            edit(s);
        });
        details::details(server, sheet_open, on_edit, details_actions.check.clone(), move || checking.with(|c| c.contains(&id)))
            .into_element()
    });

    let confirm = alert(
        delete_open,
        move || {
            let s = deleting.get_untracked();
            let runs = s.as_ref().map(|s| s.run_count).unwrap_or(0);
            Alert {
                title: format!("Delete “{}”?", s.as_ref().map(|s| s.name.as_str()).unwrap_or("server")).into(),
                description: delete_runs.with(|names| delete_description(names, runs)).into(),
                action: "Delete server".into(),
                busy_label: "Deleting…".into(),
                destructive: true,
            }
        },
        move || delete_busy.get(),
        move || delete_error.get(),
        move || {
            let Some(s) = deleting.get_untracked() else { return };
            delete_busy.set(true);
            let id = s.id;
            call(move |api| async move { api.delete_mcp_server(id).await }, move |result| {
                delete_busy.set(false);
                match result {
                    Ok(()) => {
                        toast_success(format!("Deleted {}.", s.name));
                        delete_open.set(false);
                        sheet_open.set(false);
                        reload();
                    }
                    Err(e) if e.status == Some(409) => delete_error.set(Some(format!(
                        "A run that is going uses {}. Pause it or let it finish, then delete the server. {}",
                        s.name, e.message
                    ))),
                    Err(e) => delete_error.set(Some(e.message)),
                }
            });
        },
    );

    let add = move || dialog_state.add(Mode::Paste);
    let mut frame = page("Tools");
    frame = frame.desktop_actions(move || {
        let empty = load.with(|l| l.loaded().is_some_and(Vec::is_empty));
        (!empty)
            .then(|| button("Add servers").leading(icon::PLUS).on_press(add).into_element())
            .unwrap_or_else(|| view().into_element())
    });
    frame
        .bar_actions(move || {
            dynamic(move || {
                has_servers().then(|| Button::icon(icon::PLUS, "Add servers").variant(Variant::Secondary).size(Size::Icon).round().on_press(add))
            })
            .into_element()
        })
        .child(content)
        .child(form)
        .child(sheet)
        .child(confirm)
}

/// No servers: the paste box itself, Import, and the form for a server typed by hand.
fn empty_page(state: DialogState, stdio_allowed: Signal<bool>, imported: Rc<dyn Fn()>) -> impl IntoElement {
    let go: Rc<dyn Fn()> = Rc::new(move || import(state, imported.clone()));
    let g = go.clone();
    let actions = view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(16.0))
        .child(
            button("Import servers")
                .busy(move || state.busy.get(), "Adding and checking…")
                .disabled(move || !can_import(state))
                .on_press(move || g()),
        )
        .child(text_link("Fill in a form instead", None, move || state.add(Mode::Form)));
    empty_block(
        vec![
            plasma("tools-empty", 8, 56.0, radius::MD, false).into_element(),
            body(text("No MCP servers yet. Paste the config from Claude Desktop, Cursor or VS Code to give runs your tools."))
                .text_color(move || theme().muted_foreground)
                .mt(px(16.0))
                .into_element(),
            view().mt(px(12.0)).child(dynamic(move || paste_box(state, stdio_allowed.get(), None, go.clone()).into_element())).into_element(),
        ],
        Some(actions.into_element()),
    )
    .gap(px(16.0))
    .p(px(24.0))
    .rounded(px(radius::XL))
    .bg(move || theme().muted)
}
