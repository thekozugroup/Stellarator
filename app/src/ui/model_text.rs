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

use nsh_base::{TextViewPart, TextViewParts, text_view, theme};
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
    TextViewParts::new()
        .part(TextViewPart::Root, move |s| {
            let color = color.clone();
            s.text_size(px(size)).line_height(px(line)).text_color(move || color()).max_w(px(640.0)).gap(px(size * 0.75))
        })
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
        .part(TextViewPart::Table, |s| s.text_size(px(14.0)))
        .part(TextViewPart::TableCell, |s| s.border_b_1().border_color(move || theme().border).py(px(4.9)).pr(px(10.5)))
        .part(TextViewPart::TableHeaderCell, |s| s.font_weight(FontWeight::SEMIBOLD))
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
        let style = style.clone();
        match mode {
            TextMode::Raw => text(move || source())
                .text_size(px(style.size))
                .line_height(px(style.size * 1.6))
                .text_color(style.color())
                .max_w(px(640.0))
                .selectable(true)
                .into_element(),
            TextMode::Rendered => {
                provide_context(reading_parts_with(&style));
                text_view()
                    .markdown(move || Some(SharedString::from(normalize_math(&source()))))
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

/// Models write math as `\( … \)`, `\[ … \]`, `$$ … $$` and sometimes `$ … $`. As the web UI
/// (lib/math.ts): `\( x \)` becomes inline `$$x$$`, `\[ x \]` a display block, and a line that
/// holds only `$$ x $$` a display block too. Code spans and fenced code are left alone. Single
/// dollars stay text (prompts are full of prices).
pub fn normalize_math(text: &str) -> String {
    if !text.contains("\\(") && !text.contains("\\[") && !text.contains("$$") {
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
            out.push_str(&c);
        }
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
    use super::normalize_math;

    #[test]
    fn math_delimiters_become_dollars() {
        assert_eq!(normalize_math("a \\( x^2 \\) b"), "a $$x^2$$ b");
        assert_eq!(normalize_math("\\[ x \\]"), "\n$$\nx\n$$\n");
        assert_eq!(normalize_math("  $$ e = mc^2 $$"), "  $$\n  e = mc^2\n  $$");
        // Prices and code stay as written.
        assert_eq!(normalize_math("costs $12 and $15"), "costs $12 and $15");
        assert_eq!(normalize_math("`\\( x \\)` and $$y$$"), "`\\( x \\)` and $$y$$");
        assert_eq!(normalize_math("```\n\\[ x \\]\n```"), "```\n\\[ x \\]\n```");
    }
}
