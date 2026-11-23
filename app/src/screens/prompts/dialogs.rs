//! The prompt set's dialogs (frontend/src/components/prompts/rename-prompt-set-dialog.tsx,
//! delete-prompt-set-dialog.tsx, stop-import-dialog.tsx): Rename, the Delete alert and the Stop
//! import alert. Deleting and stopping both delete the set and go back to the list.

use nsh_core::prelude::*;

use crate::api::types::PromptSetOut as PromptSet;
use crate::data::{Load, call};
use crate::format::count;
use crate::nav::{self, Destination};
use crate::ui::button::{Variant, button};
use crate::ui::field::{field, text_field};
use crate::ui::overlay::{
    Alert, DialogWidth, alert, alert_with_cancel, dialog, dialog_footer, dialog_header, toast_info, toast_success,
};

/// Which dialogs are open.
#[derive(Clone, Copy)]
pub struct Open {
    pub rename: Signal<bool>,
    pub delete: Signal<bool>,
    pub stop: Signal<bool>,
}

/// The three dialogs of the set `set` (read when one opens).
pub fn dialogs(set: Signal<Load<PromptSet>>, open: Open) -> impl IntoElement {
    let current = move || set.with_untracked(|l| l.loaded().cloned());
    // A rename on its way: the dialog stays open until its answer comes (beside `open`, so the
    // answer finds it).
    let renaming = signal(false);
    let rename = dialog(open.rename, "Rename prompt set", move || match current() {
        Some(s) => rename_form(s, set, open.rename, renaming).into_element(),
        None => view().into_element(),
    })
    .width(DialogWidth::Sm)
    .busy(move || renaming.get());
    view().child(rename).child(delete_alert(current, open.delete)).child(stop_alert(current, open.stop))
}

/// Rename: the name field with its error under it, Cancel and Save name. `busy` is the dialog's.
fn rename_form(prompt_set: PromptSet, set: Signal<Load<PromptSet>>, open: Signal<bool>, busy: Signal<bool>) -> impl IntoElement {
    let id = prompt_set.id;
    let name = signal(prompt_set.name.clone());
    let error = signal(None::<String>);
    let name_ref = element_ref();
    let len = prompt_set.name.encode_utf16().count() as u32;
    // Opens with the name selected, as the web UI's field takes focus and selects.
    let _ = request_animation_frame(move |_| {
        name_ref.focus();
        name_ref.set_selection(0, len);
    });
    let submit = move || {
        if busy.get_untracked() {
            return;
        }
        let n = name.get_untracked().trim().to_owned();
        if n.is_empty() {
            error.set(Some("Enter a name for this prompt set.".into()));
            name_ref.focus();
            return;
        }
        error.set(None);
        busy.set(true);
        call(
            move |api| async move { api.rename_prompt_set(id, &n).await },
            move |result| {
                busy.set(false);
                match result {
                    Ok(updated) => {
                        nav::set_label(updated.name.clone());
                        set.set(Load::Loaded(updated));
                        toast_success("Prompt set renamed.");
                        open.set(false);
                    }
                    Err(e) => {
                        error.try_set(Some(e.fields.get("name").cloned().unwrap_or(e.message)));
                        name_ref.focus();
                    }
                }
            },
        );
    };
    view()
        .gap(px(16.0))
        .child(dialog_header("Rename prompt set", None))
        .child(
            field(
                "Name",
                text_field(name)
                    .element(&name_ref)
                    .invalid(move || error.with(Option::is_some))
                    .on_change(move |_| error.set(None))
                    .on_submit(submit),
            )
            .error(error),
        )
        .child(dialog_footer(vec![
            button("Cancel").variant(Variant::Outline).disabled(move || busy.get()).on_press(move || open.set(false)),
            button("Save name").busy(move || busy.get(), "Saving…").on_press(submit),
        ]))
}

/// What deleting costs, in the alert's words.
pub fn delete_description(prompts: i64, runs: i64) -> String {
    if runs > 0 {
        format!(
            "Its {} and {} (with their samples) are deleted too.",
            count(prompts, "prompt", "prompts"),
            count(runs, "run", "runs")
        )
    } else {
        format!("Its {} are deleted too.", count(prompts, "prompt", "prompts"))
    }
}

/// The delete label: "Delete prompt set", or "Delete set and 3 runs".
pub fn delete_label(runs: i64) -> String {
    if runs > 0 { format!("Delete set and {}", count(runs, "run", "runs")) } else { "Delete prompt set".to_owned() }
}

/// Deletes the set, then goes back to the list with a toast.
fn delete_and_leave(
    set: PromptSet,
    done_message: String,
    busy: Signal<bool>,
    error: Signal<Option<String>>,
) {
    busy.set(true);
    let id = set.id;
    call(
        move |api| async move { api.delete_prompt_set(id).await },
        move |result| {
            busy.set(false);
            match result {
                Ok(()) => {
                    toast_success(done_message);
                    nav::open(Destination::PromptSets);
                }
                Err(e) => error.set(Some(e.message)),
            }
        },
    );
}

fn delete_alert(current: impl Fn() -> Option<PromptSet> + Copy + 'static, open: Signal<bool>) -> impl IntoElement {
    let busy = signal(false);
    let error = signal(None::<String>);
    alert(
        open,
        move || {
            error.set(None);
            let (title, description, action) = match current() {
                Some(s) => (format!("Delete “{}”?", s.name), delete_description(s.prompt_count, s.run_count), delete_label(s.run_count)),
                None => ("Delete prompt set?".to_owned(), String::new(), delete_label(0)),
            };
            Alert { title: title.into(), description: description.into(), action: action.into(), busy_label: "Deleting…".into(), destructive: true }
        },
        move || busy.get(),
        move || error.get(),
        move || {
            if let Some(s) = current() {
                let message = format!("Deleted “{}”.", s.name);
                delete_and_leave(s, message, busy, error);
            }
        },
    )
}

/// Stops the import of `set` (`POST …/stop`), which deletes the partial set; then goes back to the
/// list. The server never deletes a set whose import finished, even one that finishes while the
/// request runs (409): that set is kept, the alert closes, and the screen, which reads an
/// importing set every second, shows it ready and reads its prompts again.
fn stop_and_leave(set: PromptSet, open: Signal<bool>, busy: Signal<bool>, error: Signal<Option<String>>) {
    busy.set(true);
    let (id, name) = (set.id, set.name);
    call(
        move |api| async move { api.stop_import(id).await },
        move |result| {
            busy.set(false);
            match result {
                Ok(()) => {
                    toast_success(format!("Stopped the import of “{name}”."));
                    nav::open(Destination::PromptSets);
                }
                Err(e) if e.status == Some(409) => {
                    toast_info(format!("“{name}” finished importing before it could be stopped, so it was kept."));
                    open.set(false);
                }
                Err(e) => error.set(Some(e.message)),
            }
        },
    );
}

/// Stopping an import deletes the partial set (unless it finished meanwhile).
fn stop_alert(current: impl Fn() -> Option<PromptSet> + Copy + 'static, open: Signal<bool>) -> impl IntoElement {
    let busy = signal(false);
    let error = signal(None::<String>);
    alert_with_cancel(
        open,
        "Keep importing",
        move || {
            error.set(None);
            let name = current().map(|s| s.name).unwrap_or_default();
            Alert {
                title: format!("Stop importing “{name}”?").into(),
                description: "The prompts read so far are deleted with it.".into(),
                action: "Stop and delete".into(),
                busy_label: "Stopping…".into(),
                destructive: true,
            }
        },
        move || busy.get(),
        move || error.get(),
        move || {
            if let Some(s) = current() {
                stop_and_leave(s, open, busy, error);
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_says_what_goes_with_the_set() {
        assert_eq!(delete_description(7473, 2), "Its 7,473 prompts and 2 runs (with their samples) are deleted too.");
        assert_eq!(delete_description(1, 1), "Its 1 prompt and 1 run (with their samples) are deleted too.");
        assert_eq!(delete_description(12, 0), "Its 12 prompts are deleted too.");
        assert_eq!(delete_label(0), "Delete prompt set");
        assert_eq!(delete_label(3), "Delete set and 3 runs");
    }
}
