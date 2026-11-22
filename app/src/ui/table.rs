//! A data list (DESIGN.md, "Components", Tables): no box around it. The column header is a `muted`
//! band with 10 px corners and 12 px muted labels; rows are 44 px, divided by straight hairlines;
//! the row under the pointer gets an `accent` layer with 10 px corners. Numbers right-align in
//! tabular figures.

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::theme::metrics::{TABLE_HEADER, TABLE_ROW};
use crate::theme::radius;
use crate::theme::text::meta;

/// Cell padding on each side (`px-3`).
const CELL_PADDING: f32 = 12.0;

/// A column: its header, its width (`None` takes the rest), and whether it holds numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    pub title: &'static str,
    pub width: Option<f32>,
    pub numeric: bool,
}

impl Column {
    pub const fn flex(title: &'static str) -> Self {
        Column { title, width: None, numeric: false }
    }

    pub const fn fixed(title: &'static str, width: f32) -> Self {
        Column { title, width: Some(width), numeric: false }
    }

    pub const fn numeric(title: &'static str, width: f32) -> Self {
        Column { title, width: Some(width), numeric: true }
    }
}

/// A cell of `column` holding `content`: its width, padding and alignment.
pub fn cell(column: Column, content: impl IntoElement) -> impl IntoElement {
    let base = view().flex_row().items_center().px(px(CELL_PADDING)).min_w(px(0.0)).overflow_hidden();
    let base = match column.width {
        Some(width) => base.w(px(width)).flex_none(),
        None => base.flex_1(),
    };
    let base = if column.numeric { base.justify_end() } else { base };
    base.child(content)
}

/// The header band.
pub fn header(columns: &'static [Column]) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .h(px(TABLE_HEADER))
        .rounded(px(radius::MD))
        .bg(move || theme().muted)
        .children(columns.iter().map(|&c| cell(c, meta(text(c.title)).number_of_lines(1))))
}

/// A row of cells, with the hairline under it and the highlight under the pointer.
pub fn row(cells: Vec<AnyElement>) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .h(px(TABLE_ROW))
        .rounded(px(radius::MD))
        .hover(|s| s.bg(move || theme().accent))
        .children(cells)
        .child(view().absolute().left(px(0.0)).right(px(0.0)).bottom(px(0.0)).h(px(1.0)).bg(move || theme().border))
}
