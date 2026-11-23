//! The floating action bar (frontend/src/components/shared/action-bar.ts): the bottom bar of New
//! run, Import and the preference review. A panel in the bar material with the material's edge and
//! shadow and the panel radius (as the sidebar), 16 px by 12 px padding. Give it to a page with
//! `Page::bottom_bar`: it stays 8 px over the window's bottom edge (over the tab bar on phones)
//! while the content scrolls under it.
//!
//! ```ignore
//! page("New run").bottom_bar(|| action_bar(vec![summary, button("Start run").into_element()]).into_element())
//! ```

use nsh_core::prelude::*;

use super::material::{material_bar, material_float};
use crate::theme::radius;
use crate::theme::shape::panel_radius;

/// The bar holding `children` in a row: the first at the start, the rest at the end.
pub fn action_bar(children: Vec<AnyElement>) -> impl IntoElement {
    material_float(material_bar(view()))
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(16.0))
        .py(px(12.0))
        // The panel radius; where the window has square corners, a card's 16 px.
        .rounded(move || px(if panel_radius() > 0.0 { panel_radius() } else { radius::XL }))
        .children(children)
}
