//! Delete a preference set (frontend/src/components/preferences/delete-set-dialog.tsx): the alert
//! names the set and its pairs; runs and responses are kept. The alert stays open while the request
//! runs and on failure (its error above the buttons); once deleted, the list shows.

use nsh_core::prelude::*;

use crate::api::types::PreferenceSetOut as Set;
use crate::data::call;
use crate::format::count;
use crate::nav;
use crate::ui::overlay::{Alert, alert, toast_success};

/// The alert for `set`, shown while `open` holds.
pub fn delete_alert(set: Set, open: Signal<bool>) -> impl IntoElement {
    let busy = signal(false);
    let error = signal(None::<String>);
    let (id, name, decided) = (set.id, set.name.clone(), set.decided);
    // A new opening starts without the last error.
    effect(move || {
        if open.get() {
            error.set(None);
        }
    });
    let spec_name = name.clone();
    alert(
        open,
        move || Alert {
            title: format!("Delete ‘{spec_name}’?").into(),
            description: format!("Its {} are deleted. Runs and responses are kept.", count(decided, "pair", "pairs")).into(),
            action: "Delete preference set".into(),
            busy_label: "Deleting…".into(),
            destructive: true,
        },
        move || busy.get(),
        move || error.get(),
        move || {
            busy.set(true);
            let name = name.clone();
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
