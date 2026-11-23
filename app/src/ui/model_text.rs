//! Model text (frontend/src/components/shared/model-text.tsx, markdown.tsx, lib/math.ts): prompts,
//! thinking traces and responses, shown rendered as Markdown with formulas, or raw, exactly as the
//! model wrote it. The choice is one per viewer, kept in the app's storage, and shared by every
//! panel so they stay in step.
//!
//! Rendered text is nsh's `TextView` with the `math` feature (formulas typeset natively, KaTeX's
//! fonts) and the reading styles of index.css's `.md`: 14 px text on 1.6 lines, headings at most
//! 15 px at 600, code in Roboto Mono on `muted` with 8 px corners, a 2 px rule beside quotes.
//! Raw text keeps its line breaks and wraps anywhere. Links report their presses and open nothing,
//! and images show as their alt text, as model output is untrusted.

use nsh_base::{TextViewClasses, TextViewPart, TextViewParts, text_view, theme};
use nsh_core::color::transparent_black;
use nsh_core::element::StyleRefinement;
use nsh_core::geometry::relative;
use nsh_core::prelude::*;
use nsh_core::storage::Storage;

use super::choice::segmented_small;
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::{ink, muted_ink, radius};

/// Rendered as Markdown, or raw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextMode {
    #[default]
    Rendered,
    Raw,
}

/// The storage key, the web UI's.
const KEY: &str = "stellarator:text-mode";

/// The viewer's choice (reactive; read from storage the first time).
fn mode_signal() -> Signal<Option<TextMode>> {
    global::<Option<TextMode>>()
}

/// The viewer's choice (reactive).
pub fn text_mode() -> TextMode {
    let s = mode_signal();
    if let Some(m) = s.get() {
        return m;
    }
    let stored = if Storage::get_item(KEY).as_deref() == Some("raw") { TextMode::Raw } else { TextMode::Rendered };
    s.set(Some(stored));
    stored
}

/// Sets and keeps the viewer's choice.
pub fn set_text_mode(mode: TextMode) {
    mode_signal().set(Some(mode));
    let _ = Storage::set_item(KEY, if mode == TextMode::Raw { "raw" } else { "rendered" });
}

/// "Rendered | Raw": put it in the header of the panel that shows model text.
pub fn text_mode_toggle() -> impl IntoElement {
    segmented_small(
        vec![(TextMode::Rendered, "Rendered"), (TextMode::Raw, "Raw")],
        text_mode,
        set_text_mode,
        "Show model text",
    )
}

/// The size and color of model text: 14 px on 1.6 lines in ink by default ([`model_text`]); a
/// card's `text-sm` text is [`ModelTextStyle::SMALL`]; `muted` dims it (a thinking trace, a
/// rejected response).
#[derive(Clone)]
pub struct ModelTextStyle {
    /// The text size in points.
    pub size: f32,
    /// The line height in points.
    pub line_height: f32,
    /// Muted ink while it holds, ink otherwise.
    pub muted: std::rc::Rc<dyn Fn() -> bool>,
}

impl ModelTextStyle {
    /// 14 px on 1.6 lines, ink (index.css's `.md` with no size of its own).
    pub fn reading() -> Self {
        ModelTextStyle { size: 14.0, line_height: 22.4, muted: std::rc::Rc::new(|| false) }
    }

    /// `text-sm` on the `.md` block: 13 px on 18 px lines (the utility's line height wins over
    /// `.md`'s 1.6), ink.
    pub fn small() -> Self {
        ModelTextStyle { size: 13.0, line_height: 18.0, muted: std::rc::Rc::new(|| false) }
    }

    /// Muted ink while `muted` holds.
    pub fn muted_when(mut self, muted: impl Fn() -> bool + 'static) -> Self {
        self.muted = std::rc::Rc::new(muted);
        self
    }

    fn color(&self) -> impl Fn() -> AdaptiveColor + 'static {
        let muted = self.muted.clone();
        move || if muted() { muted_ink() } else { ink() }
    }
}

/// The reading styles of rendered model text (index.css's `.md`).
fn reading_parts() -> TextViewParts {
    reading_parts_with(&ModelTextStyle::reading())
}

/// The reading styles at `style`'s size and color.
fn reading_parts_with(style: &ModelTextStyle) -> TextViewParts {
    let (size, line) = (style.size, style.line_height);
    let color = std::rc::Rc::new(style.color());
    // The blocks take the color as well as the root: the text view's own ink on its root can win
    // over the root part's color, and the blocks' text inherits from the nearest color.
    let tinted = |color: std::rc::Rc<dyn Fn() -> AdaptiveColor>| {
        move |s: StyleRefinement| {
            let color = color.clone();
            s.text_color(move || color())
        }
    };
    let c: std::rc::Rc<dyn Fn() -> AdaptiveColor> = color.clone();
    let head = color.clone();
    TextViewParts::new()
        .part(TextViewPart::Root, move |s| {
            let color = color.clone();
            s.text_size(px(size)).line_height(px(line)).text_color(move || color()).max_w(px(if size < 14.0 { 664.0 } else { 640.0 })).gap(px(size * 0.75))
        })
        .part(TextViewPart::Paragraph, tinted(c.clone()))
        .part(TextViewPart::Heading, tinted(c.clone()))
        .part(TextViewPart::List, tinted(c))
        .part(TextViewPart::Heading, |s| s.text_size(px(15.0)).line_height(px(21.0)).font_weight(FontWeight::SEMIBOLD).mt(px(8.0)))
        .part(TextViewPart::Heading4, |s| s.text_size(px(14.0)))
        .part(TextViewPart::Heading5, |s| s.text_size(px(14.0)))
        .part(TextViewPart::Heading6, |s| s.text_size(px(14.0)))
        .part(TextViewPart::Strong, |s| s.font_weight(FontWeight::SEMIBOLD))
        .part(TextViewPart::ListMarker, |s| s.text_color(muted_ink))
        .part(TextViewPart::Blockquote, |s| s.border_l(px(2.0)).border_color(move || theme().border).pl(px(12.6)).text_color(muted_ink))
        .part(TextViewPart::Code, |s| {
            s.font_family(ROBOTO_MONO.family).text_size(px(12.6)).bg(move || theme().muted).rounded(px(4.0))
        })
        .part(TextViewPart::CodeBlock, |s| s.bg(move || theme().muted).rounded(px(radius::SM)).border(px(0.0)))
        .part(TextViewPart::CodeBlockCode, |s| {
            s.font_family(ROBOTO_MONO.family).text_size(px(12.0)).line_height(px(19.2)).px(px(12.6)).py(px(10.5))
        })
        .part(TextViewPart::Link, |s| s.text_color(ink).underline())
        // Tables as `.md table`: as wide as their text (up to the column, past it they scroll), a
        // hairline under each row, no column lines or box, the head in the text's own size and ink.
        // The grid draws its top and left edges itself: the table clips them, its scroll view
        // moved up and left by their width.
        .part(TextViewPart::Table, |s| s.text_size(px(14.0)).self_start().max_w(relative(1.0)).overflow_hidden())
        .part(TextViewPart::TableScroll, |s| s.mt(px(-1.0)).ml(px(-1.0)))
        // Bound, as the head's own fill is: a bound style wins over a fixed one.
        .part(TextViewPart::TableHead, |s| s.bg(|| -> AdaptiveColor { transparent_black().into() }))
        .part(TextViewPart::TableCell, |s| {
            s.border_r(px(0.0)).border_b_1().border_color(move || theme().border).pt(px(4.9)).pb(px(4.9)).pl(px(0.0)).pr(px(10.5))
        })
        .part(TextViewPart::TableHeaderCell, move |s| {
            let color = head.clone();
            s.font_weight(FontWeight::SEMIBOLD).text_size(px(14.0)).text_color(move || color())
        })
        .part(TextViewPart::Rule, |s| s.border_color(move || theme().border))
}

/// Model text: rendered as Markdown with formulas, or raw, as the viewer chose (or as `forced`
/// says: `Some(TextMode::Raw)` for a one-line preview).
pub fn model_text(source: impl Fn() -> String + 'static, forced: Option<TextMode>) -> impl IntoElement {
    model_text_with(source, forced, ModelTextStyle::reading())
}

/// [`model_text`] at `style`'s size and color (a card's 13 px text, a muted thinking trace).
pub fn model_text_with(source: impl Fn() -> String + 'static, forced: Option<TextMode>, style: ModelTextStyle) -> impl IntoElement {
    let source = std::rc::Rc::new(source);
    dynamic(move || {
        let mode = forced.unwrap_or_else(text_mode);
        let source = source.clone();
        // The text view's part styles are read once: a change of the color makes it again.
        let muted = (style.muted)();
        let style = ModelTextStyle { muted: std::rc::Rc::new(move || muted), ..style.clone() };
        match mode {
            TextMode::Raw => text(move || source())
                .text_size(px(style.size))
                .line_height(px(style.size * 1.6))
                .text_color(style.color())
                .max_w(px(if style.size < 14.0 { 664.0 } else { 640.0 }))
                .selectable(true)
                .into_element(),
            TextMode::Rendered => {
                provide_context(reading_parts_with(&style));
                // The size, line height and color also as the root's classes: the parts given as a
                // context do not reach a text view made in a `dynamic` (its component runs under
                // the dynamic's own owner), and the classes win over the view's defaults.
                let muted = (style.muted)();
                let classes = TextViewClasses {
                    root: Some(root_classes(style.size, style.line_height, muted).into()),
                    ..TextViewClasses::default()
                };
                text_view()
                    .classes(classes)
                    .markdown(move || Some(SharedString::from(hard_breaks(&normalize_math(&source())))))
                    .selectable(true)
                    .render_image(|image: &nsh_base::text_view::Image| {
                        let alt = if image.alt.is_empty() { "Image".to_owned() } else { image.alt.to_string() };
                        text(format!("{alt} (image link)")).underline()
                    })
                    .into_element()
            }
        }
    })
}

/// The root's classes at `size` on `line` points, in muted ink or ink.
fn root_classes(size: f32, line: f32, muted: bool) -> String {
    format!("text-[{size}px] leading-[{line}px] {}", if muted { "text-muted-foreground" } else { "text-foreground" })
}

/// A single line break in the text stays a line break when rendered (the web UI shows model text
/// with its lines): a trailing double space makes Markdown's soft break a hard one. Fenced code is
/// left alone.
pub fn hard_breaks(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = String::with_capacity(text.len() + 16);
    let mut fenced = false;
    for (i, line) in lines.iter().enumerate() {
        let fence = line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~");
        if fence {
            fenced = !fenced;
        }
        out.push_str(line);
        let next_filled = lines.get(i + 1).is_some_and(|n| !n.trim().is_empty());
        if !fenced && !fence && next_filled && !line.trim().is_empty() && !line.ends_with("  ") {
            out.push_str("  ");
        }
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

/// Models write math as `\( … \)`, `\[ … \]`, `$$ … $$` and sometimes `$ … $`. As the web UI
/// (lib/math.ts): `\( x \)` becomes inline `$$x$$`, `\[ x \]` a display block, and a line that
/// holds only `$$ x $$` a display block too. Code spans and fenced code are left alone. Single
/// dollars stay text (prompts are full of prices): they are escaped, as the text view would read
/// `$x$` as a formula.
pub fn normalize_math(text: &str) -> String {
    if !text.contains('$') && !text.contains("\\(") && !text.contains("\\[") {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 16);
    for (code, part) in split_code(text) {
        if code {
            out.push_str(part);
        } else {
            let a = replace_delimited(part, "\\[", "\\]", |body| format!("\n$$\n{}\n$$\n", body.trim()));
            let b = display_lines(&a);
            let c = replace_delimited(&b, "\\(", "\\)", |body| format!("$${}$$", body.trim()));
            out.push_str(&escape_single_dollars(&c));
        }
    }
    out
}

/// Each `$` that is not half of `$$` (and not escaped already) written `\$`, so it stays text.
fn escape_single_dollars(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        if c == '$' && prev != Some('$') && next != Some('$') && prev != Some('\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `text` cut into (is code, part): fenced blocks (``` or ~~~, to their closing fence or the end)
/// and inline code spans (one line) are code.
fn split_code(text: &str) -> Vec<(bool, &str)> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let rest = &text[i..];
        let fence = if rest.starts_with("```") {
            Some("```")
        } else if rest.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(f) = fence {
            let end = rest[3..].find(f).map(|e| i + 3 + e + 3).unwrap_or(text.len());
            parts.push((false, &text[start..i]));
            parts.push((true, &text[i..end]));
            start = end;
            i = end;
            continue;
        }
        if bytes[i] == b'`' {
            if let Some(e) = rest[1..].find(['`', '\n']) {
                if rest.as_bytes()[1 + e] == b'`' {
                    let end = i + 1 + e + 1;
                    parts.push((false, &text[start..i]));
                    parts.push((true, &text[i..end]));
                    start = end;
                    i = end;
                    continue;
                }
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    parts.push((false, &text[start..]));
    parts.into_iter().filter(|(_, p)| !p.is_empty()).collect()
}

/// Each `open … close` (shortest, non-empty) in `text` replaced by `f(body)`.
fn replace_delimited(text: &str, open: &str, close: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(a) = rest.find(open) {
        let after = &rest[a + open.len()..];
        match after.find(close) {
            Some(b) if b > 0 => {
                out.push_str(&rest[..a]);
                out.push_str(&f(&after[..b]));
                rest = &after[b + close.len()..];
            }
            _ => {
                out.push_str(&rest[..a + open.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A whole line of `$$ … $$` (optionally indented, no other `$$` inside, not blank) becomes a
/// display block at the same indent.
fn display_lines(text: &str) -> String {
    let mut out = Vec::new();
    for line in text.split('\n') {
        let trimmed_end = line.trim_end_matches([' ', '\t']);
        let indent_len = trimmed_end.len() - trimmed_end.trim_start_matches([' ', '\t']).len();
        let (indent, body) = trimmed_end.split_at(indent_len);
        if body.len() > 4 && body.starts_with("$$") && body.ends_with("$$") {
            let inner = &body[2..body.len() - 2];
            if !inner.contains("$$") && inner.chars().any(|c| !c.is_whitespace() && c != '$') {
                out.push(format!("{indent}$$\n{indent}{}\n{indent}$$", inner.trim()));
                continue;
            }
        }
        out.push(line.to_owned());
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{hard_breaks, normalize_math};

    #[test]
    fn the_root_classes_carry_the_size_and_color() {
        assert_eq!(super::root_classes(13.0, 20.8, true), "text-[13px] leading-[20.8px] text-muted-foreground");
        assert_eq!(super::root_classes(14.0, 22.4, false), "text-[14px] leading-[22.4px] text-foreground");
    }

    #[test]
    fn single_line_breaks_stay_breaks_outside_code() {
        assert_eq!(hard_breaks("a\nb\n\nc"), "a  \nb\n\nc");
        assert_eq!(hard_breaks("one line"), "one line");
        assert_eq!(hard_breaks("```\nx\ny\n```\nz"), "```\nx\ny\n```\nz");
    }

    #[test]
    fn math_delimiters_become_dollars() {
        assert_eq!(normalize_math("a \\( x^2 \\) b"), "a $$x^2$$ b");
        assert_eq!(normalize_math("\\[ x \\]"), "\n$$\nx\n$$\n");
        assert_eq!(normalize_math("  $$ e = mc^2 $$"), "  $$\n  e = mc^2\n  $$");
        // Prices and single-dollar formulas stay text (escaped for the text view); code stays as
        // written.
        assert_eq!(normalize_math("costs $12 and $15"), "costs \\$12 and \\$15");
        assert_eq!(normalize_math("Suppose $p$ and $$q$$"), "Suppose \\$p\\$ and $$q$$");
        assert_eq!(normalize_math("already \\$5"), "already \\$5");
        assert_eq!(normalize_math("`\\( x \\)` and $$y$$"), "`\\( x \\)` and $$y$$");
        assert_eq!(normalize_math("```\n\\[ x \\]\n```"), "```\n\\[ x \\]\n```");
    }
}
