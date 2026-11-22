//! The screens and which one is open. Only Runs is a real screen in this first slice; the other
//! destinations show a placeholder that names what will come.

mod placeholder;
mod runs;

use nsh_core::prelude::*;

/// A screen of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Runs,
    NewRun,
    PromptSets,
    Endpoints,
    Preferences,
    Settings,
}

impl Screen {
    /// The sidebar destination the screen belongs to (New run is under Runs).
    pub fn destination(self) -> Screen {
        match self {
            Screen::NewRun => Screen::Runs,
            other => other,
        }
    }
}

/// The open screen (reactive).
pub fn current() -> Screen {
    global::<Screen>().get()
}

/// Opens `screen`.
pub fn open(screen: Screen) {
    global::<Screen>().set(screen);
}

/// The view of `screen`.
pub fn view(screen: Screen) -> AnyElement {
    match screen {
        Screen::Runs => runs::screen().into_element(),
        Screen::NewRun => placeholder::screen("New run", "Starting a run").into_element(),
        Screen::PromptSets => placeholder::screen("Prompt sets", "Prompt sets").into_element(),
        Screen::Endpoints => placeholder::screen("Endpoints", "Endpoints").into_element(),
        Screen::Preferences => placeholder::screen("Preferences", "Preference sets").into_element(),
        Screen::Settings => placeholder::screen("Settings", "Settings").into_element(),
    }
}
