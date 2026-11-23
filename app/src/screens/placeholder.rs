//! A destination not yet built in the native app: its frame and one quiet line naming what will
//! come (DESIGN.md: keep an empty section's frame and say what will appear).

use nsh_core::prelude::*;

use crate::theme::muted_ink;
use crate::theme::text::body;
use crate::ui::page::page;

/// The placeholder for the screen titled `screen_title`; `what` names its content.
pub fn screen(screen_title: &'static str, what: &'static str) -> impl IntoElement {
    page(screen_title).child(body(text(format!("{what}: coming in the nsh build."))).text_color(muted_ink))
}
