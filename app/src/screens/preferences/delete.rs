//! Delete a preference set (frontend/src/components/preferences/delete-set-dialog.tsx): the alert
//! names the set and its pairs; runs and responses are kept. The alert stays open while the request
//! runs and on failure (its error above the buttons); once deleted, the list shows.

use std::rc::Rc;

use nsh_core::prelude::*;

use crate::api::types::PreferenceSetOut as Set;
use crate::data::call;
use crate::format::count;
use crate::nav;
use crate::ui::overlay::{Alert, alert, toast_success};

/// The alert for the set `set` gives (read as it opens), shown while `open` holds.
pub fn delete_alert(set: Rc<dyn Fn() -> Option<Set>>, open: Signal<bool>) -> impl IntoElement {
    let busy = signal(false);
    let error = signal(None::<String>);
    // A new opening starts without the last error.
    effect(move || {
        if open.get() {
            error.set(None);
        }
    });
    let spec_set = set.clone();
    alert(
        open,
        move || {
            let s = untracked(|| spec_set());
            let name = s.as_ref().map(|s| s.name.clone()).unwrap_or_default();
            let decided = s.as_ref().map(|s| s.decided).unwrap_or(0);
            Alert {
                title: format!("Delete ‘{name}’?").into(),
                description: format!("Its {} are deleted. Runs and responses are kept.", count(decided, "pair", "pairs")).into(),
                action: "Delete preference set".into(),
                busy_label: "Deleting…".into(),
                destructive: true,
            }
        },
        move || busy.get(),
        move || error.get(),
        move || {
            let Some(s) = untracked(|| set()) else { return };
            let (id, name) = (s.id, s.name);
            busy.set(true);
            call(
                move |api| async move { api.delete_preference_set(id).await },
                move |result| {
                    busy.try_set(false);
                    match result {
                        Ok(()) => {
                            toast_success(format!("Deleted ‘{name}’"));
                            open.try_set(false);
                            nav::back_to(0);
                        }
                        Err(e) => {
                            error.try_set(Some(e.message));
                        }
                    }
                },
            );
        },
    )
}
