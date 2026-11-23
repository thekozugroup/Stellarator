//! Light, dark or the system's appearance (frontend/src/lib/theme.tsx): the choice in Settings,
//! kept in the app's storage under the web UI's key, and applied with nsh's `set_color_scheme`.

use nsh_core::prelude::*;
use nsh_core::storage::Storage;

/// The storage key, the web UI's.
const KEY: &str = "theme";

/// The appearance the user chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    Light,
    Dark,
    #[default]
    System,
}

impl Choice {
    fn stored() -> Choice {
        match Storage::get_item(KEY).as_deref() {
            Some("light") => Choice::Light,
            Some("dark") => Choice::Dark,
            _ => Choice::System,
        }
    }

    fn appearance(self) -> Appearance {
        match self {
            Choice::Light => Appearance::Light,
            Choice::Dark => Appearance::Dark,
            Choice::System => Appearance::System,
        }
    }
}

/// The choice (reactive).
pub fn choice() -> Choice {
    global::<Choice>().get()
}

/// Applies the stored choice. Call once from the root component.
pub fn apply_saved() {
    let saved = Choice::stored();
    global::<Choice>().set(saved);
    if saved != Choice::System {
        set_color_scheme(saved.appearance());
    }
}

/// Shows `choice` and keeps it.
pub fn set_choice(choice: Choice) {
    global::<Choice>().set(choice);
    set_color_scheme(choice.appearance());
    match choice {
        Choice::System => Storage::remove_item(KEY),
        Choice::Light => {
            let _ = Storage::set_item(KEY, "light");
        }
        Choice::Dark => {
            let _ = Storage::set_item(KEY, "dark");
        }
    }
}
