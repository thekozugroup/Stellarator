//! The fonts the app ships (assets/fonts, DESIGN.md "Typography"): static Inter at the weights the
//! UI uses (400 body, 500 labels and buttons, 600 titles), Inter Display 600 for display titles and
//! figures, and Roboto Mono 400 for data. `registry.assets(assets!())` (lib.rs) registers every
//! file before the first frame; theme.json names the families (`font-sans`, `font-display`,
//! `font-mono`). Both are under the SIL Open Font License (the LICENSE files next to them).

use nsh_core::element::{FontAsset, asset};

/// Inter, 400. Its family, "Inter", is also the theme's `font-sans`, which every text without a
/// family of its own is drawn with.
pub const INTER: FontAsset = asset!("fonts/Inter-Regular.ttf");

/// Inter, 500.
pub const INTER_MEDIUM: FontAsset = asset!("fonts/Inter-Medium.ttf");

/// Inter, 600.
pub const INTER_SEMIBOLD: FontAsset = asset!("fonts/Inter-SemiBold.ttf");

/// Inter Display, 600: the display title (32 px) and the answer figure (28 px).
pub const INTER_DISPLAY: FontAsset = asset!("fonts/InterDisplay-SemiBold.ttf");

/// Roboto Mono, 400: IDs, model names, timestamps, axis labels.
pub const ROBOTO_MONO: FontAsset = asset!("fonts/RobotoMono-Regular.ttf");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_families_are_the_ones_theme_json_names() {
        assert_eq!(INTER.family, "Inter");
        assert_eq!(INTER_MEDIUM.family, "Inter");
        assert_eq!(INTER_SEMIBOLD.family, "Inter");
        assert_eq!(INTER_DISPLAY.family, "Inter Display");
        assert_eq!(ROBOTO_MONO.family, "Roboto Mono");
    }
}
