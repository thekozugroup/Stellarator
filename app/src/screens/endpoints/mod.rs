//! Endpoints (frontend/src/pages/endpoints.tsx): the saved model servers. The list (a table from
//! 1024 px), Add endpoint (the form dialog, from a preset or from nothing), the details sheet a row
//! opens, Check now, and the delete alert. With none saved, the page offers the common presets.
//!
//! The screen keeps the list in a `Signal<Load<…>>` and reads it again after each change, as the
//! web UI's query is invalidated.

mod details;
mod form;
mod list;
mod status;

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use self::form::{Target, endpoint_form};
use self::list::{Actions, endpoint_list, list_skeleton};
use self::status::is_key_rejected;
use crate::api::EndpointPreset;
use crate::api::types::EndpointOut as Endpoint;
use crate::data::{Load, call};
use crate::format::count;
use crate::theme::radius;
use crate::theme::text::body;
use crate::ui::blocks::{empty_block, error_block};
use crate::ui::button::{Button, Size, Variant, button};
use crate::ui::icon;
use crate::ui::overlay::{Alert, DialogWidth, alert, dialog, side_sheet, toast_error, toast_success};
use crate::ui::page::{is_phone, page};
use crate::ui::plasma::plasma;

/// The presets the empty page offers, in this order.
const EMPTY_PRESETS: [&str; 3] = ["openrouter", "lmstudio", "ollama"];

/// The Endpoints screen.
pub fn screen() -> impl IntoElement {
    let load = signal(Load::<Vec<Endpoint>>::Loading);
    let presets = signal(Vec::<EndpointPreset>::new());
    let reload = move || call(|api| async move { api.endpoints().await }, move |r| load.set(Load::from(r)));
    reload();
    // Presets rarely change; a failed read only hides the preset chips.
    call(|api| async move { api.endpoint_presets().await }, move |r| presets.set(r.unwrap_or_default()));

    let endpoints = move || load.with(|l| l.loaded().cloned().unwrap_or_default());
    let has_endpoints = move || load.with(|l| l.loaded().is_some_and(|v| !v.is_empty()));

    // The form dialog, the details sheet and the delete alert.
    let form_open = signal(false);
    let form_target = signal(Target::default());
    let open_form = move |target: Target| {
        form_target.set(target);
        form_open.set(true);
    };
    let details_id = signal(None::<i64>);
    let sheet_open = signal(false);
    let deleting = signal(None::<Endpoint>);
    let delete_open = signal(false);
    let delete_busy = signal(false);
    let delete_error = signal(None::<String>);
    let checking = signal(Vec::<i64>::new());

    // Check now: read the model list again, keep the answer, say how it went.
    let check: Rc<dyn Fn(Endpoint)> = Rc::new(move |e: Endpoint| {
        if checking.with_untracked(|c| c.contains(&e.id)) {
            return;
        }
        checking.update(|c| c.push(e.id));
        let id = e.id;
        call(
            move |api| async move { api.check_endpoint(id).await },
            move |result| {
                checking.update(|c| c.retain(|x| *x != id));
                match result {
                    Ok(ep) => {
                        if ep.last_ok == Some(true) {
                            toast_success(format!("{}: {}.", ep.name, count(ep.models.len() as i64, "model", "models")));
                        } else {
                            let what = if is_key_rejected(ep.last_error.as_deref()) { "key rejected" } else { "unreachable" };
                            toast_error(format!("{}: {what}.", ep.name));
                        }
                        load.update(|l| {
                            if let Load::Loaded(list) = l {
                                for x in list.iter_mut().filter(|x| x.id == ep.id) {
                                    *x = ep.clone();
                                }
                            }
                        });
                    }
                    Err(e) => toast_error(e.message),
                }
            },
        );
    });
    let actions = Actions {
        open: Rc::new(move |e: Endpoint| {
            details_id.set(Some(e.id));
            sheet_open.set(true);
        }),
        edit: Rc::new(move |e: Endpoint| open_form(Target { endpoint: Some(e), preset: None })),
        delete: Rc::new(move |e: Endpoint| {
            delete_error.set(None);
            deleting.set(Some(e));
            delete_open.set(true);
        }),
        check: check.clone(),
        checking,
    };

    let add = move || open_form(Target::default());
    let content = dynamic({
        let actions = actions.clone();
        move || -> AnyElement {
            let state = load.with(|l| match l {
                Load::Loading => 0,
                Load::Failed(_) => 1,
                Load::Loaded(v) if v.is_empty() => 2,
                Load::Loaded(_) => 3,
            });
            match state {
                0 => list_skeleton().into_element(),
                1 => {
                    let message = load.with(|l| l.error().unwrap_or_default().to_owned());
                    error_block(format!("Couldn’t load endpoints. {message}"), Some(Rc::new(move || {
                        load.set(Load::Loading);
                        reload();
                    })))
                    .into_element()
                }
                2 => empty_page(presets, open_form).into_element(),
                _ => endpoint_list(endpoints, actions.clone()).into_element(),
            }
        }
    });

    let form = dialog(form_open, "Endpoint", move || {
        let close: Rc<dyn Fn()> = Rc::new(move || form_open.set(false));
        let saved: Rc<dyn Fn(Endpoint)> = Rc::new(move |_| reload());
        endpoint_form(form_target.get_untracked(), presets.get_untracked(), close, saved).into_element()
    })
    .width(DialogWidth::Xl)
    .gap(0.0);

    let details_actions = actions.clone();
    let sheet = side_sheet(sheet_open, "Endpoint details", move || {
        let Some(id) = details_id.get() else { return view().into_element() };
        let Some(endpoint) = load.with(|l| l.loaded().and_then(|v| v.iter().find(|e| e.id == id).cloned())) else {
            return view().into_element();
        };
        let edit = details_actions.edit.clone();
        let on_edit: Rc<dyn Fn(Endpoint)> = Rc::new(move |e: Endpoint| {
            sheet_open.set(false);
            edit(e);
        });
        details::details(endpoint, sheet_open, on_edit, details_actions.check.clone(), move || {
            checking.with(|c| c.contains(&id))
        })
        .into_element()
    });

    let confirm = alert(
        delete_open,
        move || {
            let e = deleting.get_untracked();
            let runs = e.as_ref().map(|e| e.run_count).unwrap_or(0);
            Alert {
                title: format!("Delete “{}”?", e.as_ref().map(|e| e.name.as_str()).unwrap_or("endpoint")).into(),
                description: if runs > 0 {
                    format!("{} used this endpoint. They keep their data but can’t resume.", count(runs, "run", "runs")).into()
                } else {
                    "No runs use this endpoint.".into()
                },
                action: "Delete endpoint".into(),
                busy_label: "Deleting…".into(),
                destructive: true,
            }
        },
        move || delete_busy.get(),
        move || delete_error.get(),
        move || {
            let Some(e) = deleting.get_untracked() else { return };
            delete_busy.set(true);
            let id = e.id;
            call(
                move |api| async move { api.delete_endpoint(id).await },
                move |result| {
                    delete_busy.set(false);
                    match result {
                        Ok(()) => {
                            toast_success(format!("Deleted {}.", e.name));
                            delete_open.set(false);
                            reload();
                        }
                        Err(err) => delete_error.set(Some(err.message)),
                    }
                },
            );
        },
    );

    let mut frame = page("Endpoints");
    // The toolbar's Add endpoint: while loading and with endpoints (the empty page has its own
    // buttons); on a phone the "+" in the bar instead.
    if !untracked(is_phone) {
        frame = frame.actions(move || {
            let empty = load.with(|l| l.loaded().is_some_and(Vec::is_empty));
            (!empty).then(|| button("Add endpoint").leading(icon::PLUS).on_press(add).into_element()).unwrap_or_else(|| view().into_element())
        });
    }
    frame
        .bar_actions(move || {
            dynamic(move || {
                has_endpoints().then(|| {
                    Button::icon(icon::PLUS, "Add endpoint").variant(Variant::Secondary).size(Size::Icon).round().on_press(add)
                })
            })
            .into_element()
        })
        .child(content)
        .child(form)
        .child(sheet)
        .child(confirm)
}

/// No endpoints: the tile, what to do, and the common presets as buttons.
fn empty_page(presets: Signal<Vec<EndpointPreset>>, open_form: impl Fn(Target) + Copy + 'static) -> impl IntoElement {
    let buttons = dynamic(move || {
        let list = presets.get();
        let mut row: Vec<AnyElement> = EMPTY_PRESETS
            .iter()
            .filter_map(|id| list.iter().find(|p| p.id == *id).cloned())
            .map(|p| {
                button(format!("Add {}", p.name))
                    .variant(Variant::Secondary)
                    .on_card()
                    .on_press(move || open_form(Target { endpoint: None, preset: Some(p.clone()) }))
                    .into_element()
            })
            .collect();
        let other = if list.is_empty() {
            button("Add endpoint")
        } else {
            button("Other…").variant(Variant::Secondary).on_card()
        };
        row.push(other.on_press(move || open_form(Target::default())).into_element());
        view().flex_row().flex_wrap().gap(px(8.0)).children(row)
    });
    empty_block(
        vec![
            plasma("endpoint-empty", 8, 56.0, radius::MD, false).into_element(),
            body(text("No endpoints yet. Add one to run prompts against a model."))
                .text_color(move || theme().muted_foreground)
                .mt(px(16.0))
                .into_element(),
        ],
        Some(buttons.into_element()),
    )
    .gap(px(16.0))
    .p(px(24.0))
    .rounded(px(radius::XL))
    .bg(move || theme().muted)
}
