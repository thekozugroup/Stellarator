use nsh_core::prelude::*;

pub fn screen() -> impl IntoElement {
    super::placeholder::screen("Settings", "Settings")
}
