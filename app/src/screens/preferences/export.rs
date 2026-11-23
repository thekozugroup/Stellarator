//! Export pairs (frontend/src/components/preferences/export-dialog.tsx), on the shared export
//! shell (`screens::export`). Filled in once the shell is on main.

use nsh_core::prelude::*;

use crate::api::types::PreferenceSetOut as Set;

/// The export dialog of `set`, shown while `open` holds.
pub fn export_dialog(_set: Set, _open: Signal<bool>) -> impl IntoElement {
    view()
}
