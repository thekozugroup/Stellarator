//! A destination not yet built in the native app: its frame and one quiet line naming what will
//! come (DESIGN.md: keep an empty section's frame and say what will appear).

use nsh_core::prelude::*;

use crate::theme::muted_ink;
use crate::theme::text::body;
use crate::nav;
use crate::ui::page::page_with;

/// The placeholder for a screen; `what` names its content. Its title is the screen's label in the
/// router (a run's name when the list opened it).
pub fn screen(_screen_title: &'static str, what: &'static str) -> impl IntoElement {
    page_with(|| Some(nav::label().into())).child(body(text(format!("{what}: coming in the nsh build."))).text_color(muted_ink))
}
