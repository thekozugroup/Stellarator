//! The screens. Each route of [`crate::nav::Route`] has its view here; a screen is a function that
//! returns a [`crate::ui::page::Page`] (its frame) with its content. Screens not yet ported show a
//! placeholder that names what will come.
//!
//! To add a screen: add its route to `nav::Route` (with its destination and label there), write
//! `screens/<name>.rs` with `pub fn screen(...) -> impl IntoElement`, and add its arm below.

pub mod endpoints;
/// The export dialogs' shared layout and the run's export (the preference export uses the same).
#[allow(dead_code, reason = "Run detail and the preference export open these; they land next")]
pub mod export;
pub mod new_run;
mod gallery;
mod placeholder;
mod preferences;
mod runs;
mod settings;

use nsh_core::prelude::*;

use crate::nav::Route;

/// The view of `route`.
pub fn view(route: Route) -> AnyElement {
    // API calls made anywhere in the screen (its menus and dialogs too) belong to the screen.
    crate::data::own_calls();
    match route {
        Route::Runs => runs::screen().into_element(),
        Route::Endpoints => endpoints::screen().into_element(),
        Route::Settings => settings::screen().into_element(),
        Route::NewRun => new_run::screen().into_element(),
        Route::PromptSets => placeholder::screen("Prompt sets", "Prompt sets").into_element(),
        Route::Preferences => preferences::list_screen().into_element(),
        Route::Run { .. } => placeholder::screen("Run", "The run's samples").into_element(),
        Route::PromptSet { .. } => placeholder::screen("Prompt set", "The prompt set").into_element(),
        Route::Import => placeholder::screen("Import prompts", "Importing prompts").into_element(),
        Route::PreferenceSet { id } => preferences::review_screen(id).into_element(),
        Route::Gallery => gallery::screen().into_element(),
    }
}
