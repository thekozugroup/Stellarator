//! Facts as tiles (frontend/src/components/shared/tiles.tsx; DESIGN.md, "Layout"): `muted` tiles
//! with 16 px corners and 16 px padding, each a figure (Inter Display 28 px, tabular) over its label
//! (12 px muted). At most four, sharing the row equally; a tile with `span` takes two shares and
//! can hold a progress bar under its figure. Never repeat a count shown elsewhere on the screen.
//!
//! ```ignore
//! tiles(vec![
//!     tile("of 7,473 samples", "3,759").span().child(progress_bar_full(3759, 0, 7473)).into_element(),
//!     tile("Output tokens/s", "320").into_element(),
//! ])
//! ```

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::theme::fonts::INTER_DISPLAY;
use crate::theme::radius;
use crate::theme::text::meta;

/// A tile, built with the methods below.
pub struct Tile {
    label: SharedString,
    value: Box<dyn Fn() -> String>,
    unit: Option<SharedString>,
    span: bool,
    child: Option<AnyElement>,
    background: Option<fn() -> AdaptiveColor>,
}

/// A figure over its label. `value` may change (a closure re-reads it).
pub fn tile(label: impl Into<SharedString>, value: impl Fn() -> String + 'static) -> Tile {
    Tile { label: label.into(), value: Box::new(value), unit: None, span: false, child: None, background: None }
}

impl Tile {
    /// Smaller muted text after the figure: "tok/s", "of 7,473".
    pub fn unit(mut self, unit: impl Into<SharedString>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    /// Two shares of the row.
    pub fn span(mut self) -> Self {
        self.span = true;
        self
    }

    /// Under the figure (a progress bar).
    pub fn child(mut self, element: impl IntoElement) -> Self {
        self.child = Some(element.into_element());
        self
    }

    /// Inside dialogs and sheets `muted` is the ground: pass `material::dialog_muted`.
    pub fn background(mut self, color: fn() -> AdaptiveColor) -> Self {
        self.background = Some(color);
        self
    }
}

impl IntoElement for Tile {
    fn into_element(self) -> AnyElement {
        let value = self.value;
        let bg = self.background.unwrap_or(|| theme().muted);
        view()
            .flex_grow(if self.span { 2.0 } else { 1.0 })
            .basis(px(0.0))
            .min_w(px(0.0))
            .gap(px(4.0))
            .p(px(16.0))
            .rounded(px(radius::XL))
            .bg(bg)
            .child(
                view()
                    .flex_row()
                    .items_baseline()
                    .gap(px(6.0))
                    .min_w(px(0.0))
                    .child(
                        text(move || value())
                            .font_family(INTER_DISPLAY.family)
                            .text_size(px(28.0))
                            .line_height(px(31.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .letter_spacing(px(-0.84))
                            .tabular_nums()
                            .number_of_lines(1)
                            .text_color(move || theme().foreground),
                    )
                    .child(self.unit.map(|u| crate::theme::text::body(text(u)).tabular_nums().text_color(crate::theme::muted_ink))),
            )
            .child(meta(text(self.label)).number_of_lines(1))
            .child(self.child.map(|c| view().mt(px(8.0)).child(c)))
            .into_element()
    }
}

/// A row of tiles, 8 px apart.
pub fn tiles(children: Vec<AnyElement>) -> impl IntoElement {
    view().flex_row().gap(px(8.0)).children(children)
}
