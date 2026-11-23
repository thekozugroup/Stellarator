//! The run's actions (frontend/src/components/runs/run-actions.tsx): pause or resume, retry the
//! failures, export, and the rarer ones in a "…" menu (Run again…, Rename…, Delete run…). The one
//! ink button follows the state: Resume on a paused run (which also queues its failed samples
//! again), Export once a finished run has records, Retry failed when it has none. A running run
//! has no primary, and neither does a finished one whose primary is Edit endpoint.

use std::rc::Rc;

use nsh_core::prelude::*;

use super::data::{ActionFacts, Primary};
use crate::api::types::{RunOut, RunStatus};
use crate::data::call;
use crate::format::{count, int};
use crate::nav::{self, Destination};
use crate::screens::new_run;
use crate::ui::button::{Button, Size, Variant, button};
use crate::ui::field::{field, text_field};
use crate::ui::icon;
use crate::ui::overlay::{Alert, DialogWidth, alert, dialog, dialog_footer, dialog_header, toast_error, toast_success};
use crate::ui::page::window_width;
use crate::ui::select::{MenuEntry, menu_destructive, menu_entry, menu_with};

/// What the actions change, and the dialogs they open.
#[derive(Clone, Copy)]
pub struct RunActions {
    /// The action on its way: "pause", "resume" or "retry-failed".
    busy: Signal<Option<&'static str>>,
    rename_open: Signal<bool>,
    rename_value: Signal<String>,
    rename_error: Signal<Option<String>>,
    renaming: Signal<bool>,
    delete_open: Signal<bool>,
    deleting: Signal<bool>,
    delete_error: Signal<Option<String>>,
}

impl RunActions {
    pub fn new() -> Self {
        RunActions {
            busy: signal(None),
            rename_open: signal(false),
            rename_value: signal(String::new()),
            rename_error: signal(None),
            renaming: signal(false),
            delete_open: signal(false),
            deleting: signal(false),
            delete_error: signal(None),
        }
    }

    /// Pause, resume or retry; `updated` gets the run the server sent back.
    fn act(self, run: &ActionFacts, what: &'static str, updated: Rc<dyn Fn(RunOut)>) {
        if self.busy.get_untracked().is_some() {
            return;
        }
        self.busy.set(Some(what));
        let (id, failed, paused) = (run.id, run.failed, run.status == RunStatus::Paused);
        let busy = self.busy;
        call(
            move |api| async move { api.run_action(id, what).await },
            move |result| {
                busy.set(None);
                match result {
                    Ok(r) => {
                        updated(r);
                        if what == "retry-failed" {
                            toast_success(format!("Retrying {}", count(failed, "failed sample", "failed samples")));
                        }
                    }
                    Err(e) => {
                        let verb = if what == "pause" {
                            "pause"
                        } else if paused {
                            "resume"
                        } else {
                            "retry the failed samples of"
                        };
                        toast_error(format!("Couldn’t {verb} the run. {}", e.message));
                    }
                }
            },
        );
    }

    /// The buttons for `run` (its primary action already decided: none while Edit endpoint is, or
    /// may become, the primary). `on_export` opens the export dialog.
    pub fn buttons(self, run: &ActionFacts, on_export: Rc<dyn Fn()>, updated: Rc<dyn Fn(RunOut)>) -> AnyElement {
        let completed = run.status == RunStatus::Completed;
        let exportable = run.exportable;
        let can_retry = run.failed > 0 && completed;
        let primary = run.primary;
        let phone = window_width() < 640.0;
        let busy = self.busy;
        let variant = |p: Primary| if primary == Some(p) { Variant::Primary } else { Variant::Secondary };
        // Retrying the failed samples of a paused run also resumes it, so Resume does both.
        let resume: &'static str = if run.failed > 0 { "retry-failed" } else { "resume" };

        let mut out: Vec<AnyElement> = Vec::new();
        if can_retry {
            let (r, u) = (run.clone(), updated.clone());
            out.push(
                button(format!("Retry failed ({})", int(run.failed)))
                    .variant(variant(Primary::Retry))
                    .leading(icon::ROTATE_CCW)
                    .busy(move || busy.get() == Some("retry-failed"), "Retrying…")
                    .on_press(move || self.act(&r, "retry-failed", u.clone()))
                    .into_element(),
            );
        }
        if exportable {
            // Without its icon on phones, so a paused run's actions and menu fit one row.
            let mut b = button("Export dataset…").variant(variant(Primary::Export));
            if !phone {
                b = b.leading(icon::DOWNLOAD);
            }
            out.push(b.on_press(move || on_export()).into_element());
        }
        match run.status {
            RunStatus::Running => {
                let (r, u) = (run.clone(), updated.clone());
                out.push(
                    button("Pause run")
                        .variant(Variant::Secondary)
                        .leading(icon::PAUSE)
                        .busy(move || busy.get() == Some("pause"), "Pausing…")
                        .on_press(move || self.act(&r, "pause", u.clone()))
                        .into_element(),
                );
            }
            RunStatus::Pausing => {
                out.push(button("Pausing…").variant(Variant::Secondary).leading(icon::PAUSE).disabled(|| true).into_element());
            }
            RunStatus::Paused => {
                let (r, u) = (run.clone(), updated.clone());
                let mut b = button("Resume run")
                    .variant(variant(Primary::Resume))
                    .busy(move || busy.get() == Some(resume), "Resuming…")
                    .on_press(move || self.act(&r, resume, u.clone()));
                if !phone {
                    b = b.leading(icon::PLAY);
                }
                out.push(b.into_element());
            }
            RunStatus::Completed => {}
        }
        let (id, name) = (run.id, run.name.clone());
        out.push(
            menu_with(
                Button::icon(icon::ELLIPSIS, "More actions for this run").variant(Variant::Secondary).size(Size::Icon).without_tooltip(),
                vec![
                    menu_entry(None, "Run again…", move || new_run::open(new_run::Prefill { from: Some(id), endpoint: None, prompt_set: None })),
                    menu_entry(None, "Rename…", move || {
                        self.rename_value.set(name.clone());
                        self.rename_error.set(None);
                        self.rename_open.set(true);
                    }),
                    MenuEntry::Separator,
                    menu_destructive(None, "Delete run…", move || {
                        self.delete_error.set(None);
                        self.delete_open.set(true);
                    }),
                ],
                Some(176.0),
            )
            .into_element(),
        );
        view().flex_row().flex_wrap().items_center().gap(px(8.0)).children(out).into_element()
    }

    /// The rename dialog and the delete alert, for `run` (reactive).
    pub fn dialogs(self, run: impl Fn() -> Option<RunOut> + Clone + 'static, updated: Rc<dyn Fn(RunOut)>) -> impl IntoElement {
        let name_ref = element_ref();
        let r1 = run.clone();
        let submit: Rc<dyn Fn()> = Rc::new(move || {
            if self.renaming.get_untracked() {
                return;
            }
            let Some(id) = untracked(&r1).map(|r| r.id) else { return };
            let name = self.rename_value.get_untracked().trim().to_owned();
            if name.is_empty() {
                self.rename_error.set(Some("Enter a name.".into()));
                name_ref.focus();
                return;
            }
            self.renaming.set(true);
            let updated = updated.clone();
            call(
                move |api| async move { api.update_run(id, &serde_json::json!({ "name": name })).await },
                move |result| {
                    self.renaming.set(false);
                    match result {
                        Ok(r) => {
                            updated(r);
                            self.rename_open.set(false);
                            toast_success("Run renamed");
                        }
                        Err(e) => self.rename_error.set(Some(e.fields.get("name").cloned().unwrap_or(e.message))),
                    }
                },
            );
        });
        let rename = dialog(self.rename_open, "Rename run", move || {
            let (s1, s2) = (submit.clone(), submit.clone());
            let len = self.rename_value.with_untracked(|v| v.encode_utf16().count()) as u32;
            let _ = request_animation_frame(move |_| {
                name_ref.focus();
                name_ref.set_selection(0, len);
            });
            view()
                .gap(px(16.0))
                .child(dialog_header("Rename run", None))
                .child(
                    field(
                        "Name",
                        text_field(self.rename_value)
                            .element(&name_ref)
                            .invalid(move || self.rename_error.with(Option::is_some))
                            .accessibility_label("Name")
                            .on_change(move |_| self.rename_error.set(None))
                            .on_submit(move || s1()),
                    )
                    .error(self.rename_error),
                )
                .child(dialog_footer(vec![
                    button("Cancel").variant(Variant::Secondary).on_press(move || self.rename_open.set(false)),
                    button("Save name").busy(move || self.renaming.get(), "Saving…").on_press(move || s2()),
                ]))
                .into_element()
        })
        .width(DialogWidth::Md);

        let r2 = run.clone();
        let r3 = run.clone();
        let confirm = alert(
            self.delete_open,
            move || {
                let r = untracked(&r2);
                let (name, total) = r.map(|r| (r.name, r.total)).unwrap_or_default();
                Alert {
                    title: format!("Delete run “{name}” and its {}?", count(total, "sample", "samples")).into(),
                    description: "Responses and thinking traces are deleted for good. Exported files are not affected.".into(),
                    action: "Delete run".into(),
                    busy_label: "Deleting…".into(),
                    destructive: true,
                }
            },
            move || self.deleting.get(),
            move || self.delete_error.get(),
            move || {
                let Some(r) = untracked(&r3) else { return };
                self.deleting.set(true);
                let (id, name) = (r.id, r.name);
                call(
                    move |api| async move { api.delete_run(id).await },
                    move |result| {
                        self.deleting.set(false);
                        match result {
                            Ok(()) => {
                                toast_success(format!("Deleted “{name}”"));
                                self.delete_open.set(false);
                                if nav::destination() == Destination::Runs {
                                    nav::open(Destination::Runs);
                                }
                            }
                            Err(e) => self.delete_error.set(Some(format!("Couldn’t delete the run. {}", e.message))),
                        }
                    },
                );
            },
        );
        view().child(rename).child(confirm)
    }
}
