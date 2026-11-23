//! Keyboard shortcuts. `mod` is Command on macOS (and in a browser on a Mac), Control elsewhere.
//!
//! | Keys | Where | What |
//! | --- | --- | --- |
//! | `mod+[` | everywhere | back one screen ([`crate::nav::back`]) |
//! | `j`, `k` | sample viewer | next and previous sample (the web UI's) |
//! | `j`, `k`, arrows, `u`, `?` | preference review | next and previous pair, choose, undo, help (the web UI's) |
//!
//! A screen binds its own keys with [`bind`] in its body: they hold while the screen is shown and
//! go with it. Plain letters do not fire while a text field has the focus (nsh's default,
//! `When::Auto`). Show a key in the UI with [`label`] and `ui::tooltip::kbd`.

use nsh_core::prelude::*;

use crate::nav;

/// Back one screen.
pub const BACK: &str = "mod+[";

/// Binds the app-wide shortcuts. Call once from the root component.
pub fn install() {
    on_key(BACK, |_| {
        nav::back();
    });
}

/// Runs `handler` when `keys` ("j", "shift+mod+l", "arrowdown") is pressed, while the calling
/// component lives.
pub fn bind(keys: &'static str, handler: impl Fn() + 'static) {
    on_key(keys, move |_| handler());
}

/// `keys` as the keyboard labels them: "⌘[" on a Mac, "Ctrl+[" elsewhere.
pub fn label(keys: &str) -> String {
    key_label(keys)
}
