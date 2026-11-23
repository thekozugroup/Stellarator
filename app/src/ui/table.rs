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
    /// For a column without a width: its share of the free space (1 for [`Column::flex`]).
    pub grow: f32,
}

impl Column {
    pub const fn flex(title: &'static str) -> Self {
        Column { title, width: None, numeric: false, grow: 1.0 }
    }

    /// A column without a width that takes `grow` shares of the free space (two flex columns with
    /// 1.1 and 1 share it 11 to 10).
    pub const fn share(title: &'static str, grow: f32) -> Self {
        Column { title, width: None, numeric: false, grow }
    }

    pub const fn fixed(title: &'static str, width: f32) -> Self {
        Column { title, width: Some(width), numeric: false, grow: 1.0 }
    }

    pub const fn numeric(title: &'static str, width: f32) -> Self {
        Column { title, width: Some(width), numeric: true, grow: 1.0 }
    }
}

/// A cell of `column` holding `content`: its width, padding and alignment.
pub fn cell(column: Column, content: impl IntoElement) -> impl IntoElement {
    let base = view().flex_row().items_center().px(px(CELL_PADDING)).py(px(4.0)).min_w(px(0.0)).overflow_hidden();
    let base = match column.width {
        Some(width) => base.w(px(width)).flex_none(),
        None => base.flex_grow(column.grow).basis(px(0.0)),
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

/// A row of cells, at least 44 px, with the hairline under it (none under the last row, while
/// `last` holds) and the highlight under the pointer. Add `.on_press` to open the row's item.
pub fn row(cells: Vec<AnyElement>, last: impl Fn() -> bool + 'static) -> View {
    view()
        .flex_row()
        .items_center()
        .min_h(px(TABLE_ROW))
        .rounded(px(radius::MD))
        .hover(|s| s.bg(move || theme().accent))
        .children(cells)
        .child(
            view()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .h(px(1.0))
                .bg(move || theme().border)
                .opacity(move || if last() { 0.0_f32 } else { 1.0 }),
        )
}

/// A part of a keyed row (`each`) made from its item: made again when `part` of the item changes,
/// and only then. A list whose items are read again every second (a run that goes, an importing
/// set) keeps its rows, their open menus and their focus; only the cells whose fields changed are
/// made again.
pub fn bound<T: 'static, K: PartialEq + 'static>(
    item: ReadSignal<T>,
    part: impl Fn(&T) -> K + 'static,
    make: impl Fn(&T) -> AnyElement + 'static,
) -> AnyElement {
    let key = computed(move || item.with(&part));
    dynamic(move || {
        key.with(|_| ());
        item.with_untracked(&make)
    })
}
