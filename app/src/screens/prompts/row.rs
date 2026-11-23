//! One prompt of a set (frontend/src/components/prompts/prompt-row.tsx): its number, the user text
//! (Rendered or Raw, clamped to about four lines with a fade), whether it has a system message, its
//! meta, and Show all, which opens every message of the prompt.

use nsh_core::color::{black, transparent_black};
use nsh_core::prelude::*;

use crate::api::types::PromptOut as Prompt;
use crate::format::{int, one_line, prompt_text};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, meta};
use crate::theme::{ink, muted_ink, radius};
use crate::ui::blocks::bar;
use crate::ui::model_text::model_text_sm as model_text;
use nsh_base::theme;

/// The clamped text is `max-h-24`: 96 px.
const CLAMP: f32 = 96.0;
/// The text overflows when it is taller than the clamp by more than this (`clientHeight + 6`).
const SLACK: f32 = 6.0;
/// The index column (`3rem`).
const INDEX: f32 = 48.0;

/// "key: value · key: value", each value on one line of at most 60 characters.
pub fn meta_line(meta: &serde_json::Value) -> String {
    let Some(map) = meta.as_object() else { return String::new() };
    map.iter()
        .map(|(k, v)| {
            let value = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{k}: {}", one_line(&value, 60))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn content_of(m: &serde_json::Value) -> String {
    m.get("content").and_then(|c| c.as_str()).unwrap_or_default().to_owned()
}

fn role_of(m: &serde_json::Value) -> &str {
    m.get("role").and_then(|r| r.as_str()).unwrap_or_default()
}

/// "assistant" becomes "Assistant".
fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// A muted 12 px caption.
fn caption(words: String) -> impl IntoElement {
    meta(text(words)).text_color(muted_ink)
}

/// The row's frame: the number in a 48 px column, the content beside it, a hairline under it.
fn frame(number: impl IntoElement, content: impl IntoElement, last: bool) -> AnyElement {
    view()
        .flex_row()
        .gap(px(8.0))
        .px(px(16.0))
        .py(px(12.0))
        .child(view().w(px(INDEX)).flex_none().pt(px(2.0)).child(number))
        .child(view().flex_1().min_w(px(0.0)).child(content))
        .child(
            view()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .h(px(1.0))
                .bg(move || theme().border)
                .opacity(if last { 0.0_f32 } else { 1.0 }),
        )
        .into_element()
}

/// The prompt, numbered `idx + 1`.
pub fn prompt_row(prompt: &Prompt, last: bool) -> AnyElement {
    let open = signal(false);
    let overflows = signal(false);
    let text_of = prompt_text(&prompt.messages);
    let system = prompt.messages.iter().find(|m| role_of(m) == "system").cloned();
    let meta_text = meta_line(&prompt.meta);
    let others: Vec<serde_json::Value> = prompt
        .messages
        .iter()
        .filter(|m| Some(*m) != system.as_ref() && content_of(m) != text_of)
        .cloned()
        .collect();
    let has_others = !others.is_empty();
    let has_system = system.is_some();

    let number = body(text(format!("#{}", int(prompt.idx + 1))))
        .font_family(ROBOTO_MONO.family)
        .tabular_nums()
        .text_color(muted_ink)
        .number_of_lines(1);

    // The messages above the user's, when the row is open.
    let earlier = {
        let (system, others) = (system.clone(), others.clone());
        dynamic(move || {
            if !open.get() {
                return None;
            }
            let mut parts: Vec<AnyElement> = Vec::new();
            if let Some(s) = &system {
                let content = content_of(s);
                parts.push(
                    view()
                        .mb(px(8.0))
                        .child(caption("System message".into()))
                        .child(
                            view()
                                .mt(px(4.0))
                                .rounded(px(radius::SM))
                                .bg(move || theme().muted)
                                .px(px(12.0))
                                .py(px(8.0))
                                .child(model_text(move || content.clone(), None)),
                        )
                        .into_element(),
                );
            }
            if !others.is_empty() {
                parts.push(
                    view()
                        .mb(px(8.0))
                        .gap(px(8.0))
                        .children(
                            others
                                .iter()
                                .map(|m| {
                                    let content = content_of(m);
                                    view()
                                        .child(caption(capitalized(role_of(m))))
                                        .child(model_text(move || content.clone(), None))
                                        .into_element()
                                })
                                .collect::<Vec<_>>(),
                        )
                        .into_element(),
                );
            }
            if has_system || has_others {
                parts.push(caption("User".into()).into_element());
            }
            Some(view().children(parts))
        })
    };

    // The user text: all of it when open, else clamped with a fade once it is taller than the clamp.
    let body_text = if text_of.is_empty() {
        body(text("This prompt has no user text.")).text_color(muted_ink).into_element()
    } else {
        let t_open = text_of.clone();
        let t_clamped = text_of.clone();
        dynamic(move || -> AnyElement {
            if open.get() {
                let t = t_open.clone();
                return model_text(move || t.clone(), None).into_element();
            }
            let t = t_clamped.clone();
            view()
                .max_h(px(CLAMP))
                .overflow_hidden()
                .mask_linear(
                    deg(180.0),
                    move || {
                        if overflows.get() {
                            vec![(black(), percentage(0.7)).into(), (transparent_black(), percentage(1.0)).into()]
                        } else {
                            Vec::new()
                        }
                    },
                )
                .child(
                    view()
                        .flex_shrink(0.0)
                        .on_layout(move |b: &Bounds<Points>| overflows.set(b.size.height.0 > CLAMP + SLACK))
                        .child(model_text(move || t.clone(), None)),
                )
                .into_element()
        })
        .into_element()
    };

    // The line under the text: the system note, the meta, Show all.
    let expandable = move || overflows.get() || open.get() || has_system || has_others;
    let note = has_system.then(|| dynamic(move || (!open.get()).then(|| meta(text("Has a system message")).text_color(muted_ink))));
    let meta_node = (!meta_text.is_empty()).then(|| meta(text(meta_text)).text_color(muted_ink).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)));
    let toggle = dynamic(move || {
        expandable().then(|| {
            meta(text(if open.get() { "Show less" } else { "Show all" }))
                .text_color(ink)
                .underline()
                .py(px(6.0))
                .my(px(-6.0))
                .role(AccessibilityRole::Button)
                .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
                .focusable(true)
                .cursor(Cursor::Pointer)
                .on_press(move || open.update(|o| *o = !*o))
        })
    });
    let footer = view()
        .mt(px(6.0))
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_x(px(12.0))
        .gap_y(px(4.0))
        .child(note)
        .child(meta_node)
        .child(toggle);

    frame(number, view().child(earlier).child(body_text).child(footer), last)
}

/// Loading rows at the real pitch (`PromptRowsSkeleton`).
pub fn rows_skeleton(rows: usize) -> impl IntoElement {
    let items = (0..rows).map(|i| {
        frame(
            bar(24.0, 16.0),
            view().gap(px(6.0)).child(bar(560.0, 16.0)).child(bar(448.0, 16.0)).child(bar(240.0, 16.0)),
            i + 1 == rows,
        )
    });
    view().role(AccessibilityRole::Progressbar).label("Loading prompts").children(items.collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_meta_line_lists_each_key_on_one_line() {
        assert_eq!(meta_line(&json!({})), "");
        // Keys keep the order the server sent them in.
        assert_eq!(meta_line(&json!({"source": "gsm8k", "n": 3})), "source: gsm8k · n: 3");
        let long = "x".repeat(80);
        let line = meta_line(&json!({ "a": format!("one\ntwo {long}") }));
        assert!(line.starts_with("a: one two xxx"), "{line}");
        assert!(line.ends_with('…'));
        assert_eq!(line.chars().count(), "a: ".len() + 60);
    }

    #[test]
    fn roles_are_capitalized() {
        assert_eq!(capitalized("assistant"), "Assistant");
        assert_eq!(capitalized(""), "");
    }
}
