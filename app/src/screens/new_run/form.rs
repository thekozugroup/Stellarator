//! The form of New run (frontend/src/pages/run-new.tsx, NewRunForm): the grouped rows, 640 px at
//! most. Source (endpoint, model, prompt set), Requests, Sampling, then the two groups that open
//! (Prompt options, Advanced) and Output.

use std::rc::Rc;

use nsh_base::{SelectEntry, SelectItem};
use nsh_core::prelude::*;

use super::logic::{Field, advanced_summary, default_name, model_after_endpoint, prompt_set_note, prompt_summary, shared_names};
use super::state::{Data, State};
use crate::api::types::{MaxTokensField, PromptSetOut as PromptSet, PromptSetStatus};
use crate::data::{Load, call};
use crate::format::count;
use crate::nav::{self, Destination};
use crate::screens::endpoints::open_edit;
use crate::screens::endpoints::status::{Trouble, endpoint_trouble};
use crate::theme::text::{body, mono};
use crate::theme::{muted_ink, radius, token};
use crate::ui::blocks::error_block;
use crate::ui::button::{Button, Size, Variant, text_link};
use crate::ui::choice::{checkbox_with, segmented_mono};
use crate::ui::field::{TextField, form_disclosure, form_group, form_row, text_area, text_field};
use crate::ui::icon;
use crate::ui::page::is_phone;
use crate::ui::pickers::{RichSelect, TypedCombobox};
use crate::ui::plasma::plasma;
use crate::ui::stepper::{number_stepper, seqs_choices};

/// The widest the form (and its bar) goes: `max-w-[40rem]`.
pub const FORM_WIDTH: f32 = 640.0;

/// The value of the "use what I typed" item of the model list, which a listed model's id cannot be.
const TYPED: &str = "\u{1}typed:";

/// A number field's width (`w-28`).
const NUMBER_WIDTH: f32 = 112.0;

/// The form.
pub fn form_view(state: State, data: Data, submit: Rc<dyn Fn()>, source_error: Option<String>) -> impl IntoElement {
    let prompt_rows = {
        move || {
            vec![
                form_row(
                    "System prompt",
                    area(state, Field::SystemPrompt, false).placeholder("You are a careful assistant. Think step by step."),
                )
                .stacked()
                .optional()
                .hint("Replaces the set's system message.")
                .error(state.error(Field::SystemPrompt))
                .into_element(),
                form_row(
                    "Prompt template",
                    area(state, Field::UserTemplate, true)
                        .placeholder("Solve this problem. Put the final answer on its own line.\n\n{prompt}"),
                )
                .stacked()
                .optional()
                .hint("Must contain {prompt}.")
                .error(state.error(Field::UserTemplate))
                .into_element(),
            ]
        }
    };
    let advanced_rows = {
        let submit = submit.clone();
        move || advanced_rows(state, &submit)
    };
    view()
        .max_w(px(FORM_WIDTH))
        .gap(px(24.0))
        .child(source_error.map(|m| error_block(m, None)))
        .child(
            form_group(vec![
                endpoint_row(state, data, submit.clone()).into_element(),
                model_row(state, data).into_element(),
                prompt_set_row(state, data).into_element(),
            ])
            .title("Source"),
        )
        .child(form_group(requests_rows(state, data, &submit)).title("Requests"))
        .child(form_group(sampling_rows(state, &submit)).title("Sampling"))
        .child(super::tools_view::tools_section(state, data, &submit))
        .child(form_disclosure("Prompt options", move || prompt_summary(&state.form()), state.prompt_open, prompt_rows, false))
        .child(form_disclosure("Advanced", move || advanced_summary(&state.form()), state.advanced_open, advanced_rows, false))
        .child(
            form_group(vec![
                form_row(
                    "Run name",
                    field_for(state, Field::Name, &submit, false).placeholder_with(move || {
                        let name = default_name(&state.get(Field::Model), data.selected_set(&state).as_ref());
                        if name.is_empty() { "Qwen3 distill, round 2".to_owned() } else { name }
                    }),
                )
                .optional()
                .error(state.error(Field::Name))
                .into_element(),
            ])
            .title("Output"),
        )
        .child(dynamic(move || state.form_error.get().map(|m| error_block(m, None))))
}

// ------------------------------------------------------------------ wiring

/// A one-line field for `field`: invalid while it has an error, the error cleared by typing, its
/// element kept to focus it, Return starting the run, and (with `blur`) checked when it loses the
/// focus.
fn field_for(state: State, field: Field, submit: &Rc<dyn Fn()>, blur: bool) -> TextField {
    let submit = submit.clone();
    let mut f = text_field(state.value(field))
        .invalid(move || state.has_error(field))
        .on_change(move |_| state.error(field).set(None))
        .on_submit(move || submit())
        .element(&state.element(field));
    if blur {
        f = f.on_blur(move || state.blur(field));
    }
    f
}

/// A text area for `field` (80 px, three lines), wired as [`field_for`] (Return is a new line).
fn area(state: State, field: Field, blur: bool) -> TextField {
    let mut f = text_area(state.value(field))
        .height(80.0)
        .invalid(move || state.has_error(field))
        .on_change(move |_| state.error(field).set(None))
        .element(&state.element(field));
    if blur {
        f = f.on_blur(move || state.blur(field));
    }
    f
}

/// A number's field: 112 px, left-aligned figures.
fn number_field(state: State, field: Field, submit: &Rc<dyn Fn()>) -> TextField {
    field_for(state, field, submit, true).width(NUMBER_WIDTH).tabular()
}

// ------------------------------------------------------------------ Source

/// "No endpoints yet. Add an endpoint": what stands in the place of a picker with nothing to pick.
fn nothing_yet(sentence: &'static str, action: &'static str, on_press: impl Fn() + 'static) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(4.0))
        .min_h(px(32.0))
        .child(body(text(sentence)).text_color(muted_ink))
        .child(text_link(action, None, on_press))
}

/// A pixel tile of 20 px, as the web UI's `Plasma cells={5} className="size-5 rounded-xs"`.
fn tile(seed: String) -> AnyElement {
    plasma(&seed, 5, 20.0, radius::XS, false).into_element()
}

/// A picker's row: the tile, the name (cut at the end) and a muted detail after it.
fn picked(seed: String, name: String, detail: Option<(String, bool)>) -> AnyElement {
    let detail = detail.map(|(text_detail, negative)| {
        body(text(format!(" · {text_detail}")))
            .text_color(move || if negative { token("negative-ink")() } else { muted_ink() })
            .tabular_nums()
            .flex_none()
    });
    view()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .min_w(px(0.0))
        .child(tile(seed))
        .child(body(text(name)).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
        .child(detail)
        .into_element()
}

fn endpoint_row(state: State, data: Data, submit: Rc<dyn Fn()>) -> impl IntoElement {
    let _ = submit;
    let control = dynamic(move || -> AnyElement {
        if data.endpoint_list().is_empty() {
            return nothing_yet("No endpoints yet.", "Add an endpoint", || nav::open(Destination::Endpoints)).into_element();
        }
        RichSelect {
            items: Rc::new(move || {
                data.endpoint_list().iter().map(|e| SelectItem::new(e.id.to_string(), e.name.clone())).collect()
            }),
            value: Rc::new(move || Some(state.get(Field::Endpoint)).filter(|v| !v.is_empty()).map(SharedString::from)),
            on_change: Rc::new(move |id: SharedString| choose_endpoint(state, data, &id)),
            // The field shows the name alone; a failed check is said once, in the line below.
            shown: Rc::new(move || data.selected_endpoint(&state).map(|e| picked(format!("endpoint-{}", e.id), e.name, None))),
            render_item: Rc::new(move |entry: &SelectEntry| {
                let id = entry.value.to_string();
                match data.endpoints.with_untracked(|l| l.loaded().and_then(|v| v.iter().find(|e| e.id.to_string() == id).cloned())) {
                    Some(e) => {
                        let trouble = endpoint_trouble(Some(&e));
                        let detail = trouble
                            .map(|t| (t.word().to_owned(), true))
                            .unwrap_or_else(|| (if e.location == "cloud" { "Cloud" } else { "Local" }.to_owned(), false));
                        picked(format!("endpoint-{}", e.id), e.name, Some(detail))
                    }
                    None => body(text(entry.title.clone())).into_element(),
                }
            }),
            placeholder: "Choose an endpoint",
            accessibility: "Endpoint",
            invalid: Rc::new(move || state.has_error(Field::Endpoint)),
            element: Some(state.element(Field::Endpoint)),
        }
        .into_element()
    });
    form_row("Endpoint", control)
        .error(state.error(Field::Endpoint))
        .hint_element(move || {
            let endpoint = data.selected_endpoint(&state)?;
            Some(match endpoint_trouble(Some(&endpoint)) {
                Some(trouble) => {
                    let id = endpoint.id;
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.0))
                        .child(body(text(format!("{} at last check.", trouble.word()))).text_color(token("negative-ink")))
                        .child(inline_link("Edit endpoint", move || edit_endpoint(state, id, trouble)))
                        .into_element()
                }
                None => mono(text(endpoint.base_url)).text_color(muted_ink).into_element(),
            })
        })
}

/// A link inside a line of text: the text underlined in ink, no taller than the line.
fn inline_link(label: &'static str, on_press: impl Fn() + 'static) -> impl IntoElement {
    view()
        .role(AccessibilityRole::Link)
        .label(label)
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(on_press)
        .child(body(text(label)).underline())
}

/// Edit endpoint, in the hint under a failing endpoint: its edit dialog with the key (or URL)
/// focused; closing it returns here, with the endpoint and the prompt set kept (the web UI's
/// `returnTo`).
fn edit_endpoint(state: State, id: i64, trouble: Trouble) {
    let prompt_set = state.peek(Field::PromptSet).parse::<i64>().ok();
    super::keep_for_return(super::logic::Prefill { from: None, endpoint: Some(id), prompt_set });
    open_edit(id, trouble.field());
}

/// Chooses an endpoint: its default seqs, and the model kept when the endpoint also lists it, else
/// the one to suggest. The errors of what it changed go.
fn choose_endpoint(state: State, data: Data, id: &str) {
    let endpoint = data.endpoints.with_untracked(|l| l.loaded().and_then(|v| v.iter().find(|e| e.id.to_string() == id).cloned()));
    state.models_note.set(None);
    state.value(Field::Endpoint).set(id.to_owned());
    if let Some(e) = &endpoint {
        state.value(Field::Concurrency).set(e.default_concurrency.to_string());
    }
    let runs = data.runs.with_untracked(|l| l.loaded().cloned());
    let model = model_after_endpoint(&state.peek(Field::Model), endpoint.as_ref(), runs.as_deref());
    state.value(Field::Model).set(model);
    for field in [Field::Endpoint, Field::Model, Field::Concurrency] {
        state.error(field).set(None);
    }
}

fn model_row(state: State, data: Data) -> impl IntoElement {
    let no_models = move || data.selected_endpoint(&state).is_some_and(|e| e.models.is_empty());
    // The model shows in mono once there is one; the placeholder is plain text, so the field is
    // made again when the model appears or goes.
    let combo = move |shown_in_mono: bool| TypedCombobox {
        items: Rc::new(move || model_items(state, data)),
        value: Rc::new(move || {
            let model = state.get(Field::Model);
            if model.is_empty() { Vec::new() } else { vec![SharedString::from(model)] }
        }),
        on_change: Rc::new(move |value: SharedString| {
            let value = value.to_string();
            state.set(Field::Model, value.strip_prefix(TYPED).unwrap_or(&value));
            state.query.set(String::new());
        }),
        on_query: Rc::new(move |query: &str| state.query.set(query.to_owned())),
        placeholder: "Choose or type a model id",
        accessibility: "Model id",
        empty: Rc::new(move || {
            if no_models() { "No models listed. Type a model id." } else { "No listed model matches." }.to_owned()
        }),
        mono: shown_in_mono,
        disabled: Rc::new(move || data.selected_endpoint(&state).is_none()),
        invalid: Rc::new(move || state.has_error(Field::Model)),
        element: Some(state.element(Field::Model)),
        render_item: Some(Rc::new(|entry: &SelectEntry| {
            // Model ids in mono 12 px; what was typed as "Use <id>".
            match entry.value.strip_prefix(TYPED) {
                Some(typed) => body(text("Use ")).child(mono(text(typed.to_owned()))).number_of_lines(1).into_element(),
                None => mono(text(entry.title.clone())).number_of_lines(1).into_element(),
            }
        })),
    }
    .into_element();
    let combo = dynamic(move || combo(!state.get(Field::Model).is_empty()));
    let refresh = dynamic(move || {
        let endpoint = data.selected_endpoint(&state)?;
        Some(
            Button::icon(icon::RETRY, "Refresh models")
                .variant(Variant::Secondary)
                .on_card()
                .size(Size::Icon)
                .busy(move || state.refreshing.get(), "")
                .on_press(move || refresh_models(state, data, endpoint.id)),
        )
    });
    let control = view().flex_row().items_center().gap(px(8.0)).child(view().flex_1().min_w(px(0.0)).child(combo)).child(refresh);
    form_row("Model", control).error(state.error(Field::Model)).hint_when(move || {
        state.models_note.get().or_else(|| no_models().then(|| "No models listed. Type a model id.".to_owned())).map(SharedString::from)
    })
}

/// The items of the model list: what was typed (when it is not a listed model), the model the
/// form has when the endpoint does not list it, then the listed ones.
fn model_items(state: State, data: Data) -> Vec<SelectItem> {
    let models = data.selected_endpoint(&state).map(|e| e.models).unwrap_or_default();
    let typed = state.query.get().trim().to_owned();
    let current = state.get(Field::Model);
    let mut items = Vec::new();
    if !typed.is_empty() && typed != current && !models.contains(&typed) {
        items.push(SelectItem::new(format!("{TYPED}{typed}"), format!("Use {typed}")));
    }
    if !current.is_empty() && !models.contains(&current) {
        items.push(SelectItem::new(current.clone(), current));
    }
    items.extend(models.into_iter().map(|m| SelectItem::new(m.clone(), m).group("Listed by this endpoint")));
    items
}

/// Whether Refresh models' answer for endpoint `asked` still concerns the form: the Endpoint field
/// (`field`, the endpoint's id as text) still holds it.
pub fn refresh_applies(field: &str, asked: i64) -> bool {
    field == asked.to_string()
}

/// Reads the endpoint's model list again and says how it went under the Model field.
fn refresh_models(state: State, data: Data, id: i64) {
    if state.refreshing.get_untracked() {
        return;
    }
    state.refreshing.set(true);
    call(
        move |api| async move { api.check_endpoint(id).await },
        move |result| {
            state.refreshing.set(false);
            // The endpoint was changed while it read: the list is kept for that endpoint, but the
            // note and a model picked from it would be about another one.
            let applies = refresh_applies(&state.peek(Field::Endpoint), id);
            let note = match result {
                Ok(endpoint) => {
                    data.endpoints.update(|l| {
                        if let Load::Loaded(list) = l {
                            for e in list.iter_mut().filter(|e| e.id == endpoint.id) {
                                *e = endpoint.clone();
                            }
                        }
                    });
                    if !applies {
                        return;
                    }
                    if endpoint.last_ok == Some(false) {
                        format!("Couldn't list models. {}", endpoint.last_error.as_deref().unwrap_or("The endpoint did not answer."))
                    } else {
                        if endpoint.models.len() == 1 && state.peek(Field::Model).is_empty() {
                            state.value(Field::Model).set(endpoint.models[0].clone());
                        }
                        format!("Found {}.", count(endpoint.models.len() as i64, "model", "models"))
                    }
                }
                Err(_) if !applies => return,
                Err(e) => format!("Couldn't list models. {}", e.message),
            };
            state.models_note.set(Some(note));
        },
    );
}

fn prompt_set_row(state: State, data: Data) -> impl IntoElement {
    let control = dynamic(move || -> AnyElement {
        if data.set_list().is_empty() {
            return nothing_yet("No prompt sets yet.", "Import prompts", || nav::open(Destination::PromptSets)).into_element();
        }
        let find = move |id: &str| -> Option<PromptSet> {
            data.sets.with_untracked(|l| l.loaded().and_then(|v| v.iter().find(|s| s.id.to_string() == id).cloned()))
        };
        RichSelect {
            items: Rc::new(move || {
                data.set_list()
                    .iter()
                    .map(|s| SelectItem::new(s.id.to_string(), s.name.clone()).disabled(s.status != PromptSetStatus::Ready))
                    .collect()
            }),
            value: Rc::new(move || Some(state.get(Field::PromptSet)).filter(|v| !v.is_empty()).map(SharedString::from)),
            on_change: Rc::new(move |id: SharedString| state.set(Field::PromptSet, id.to_string())),
            // The field shows the name and size; the import date only helps tell same-named sets
            // apart in the list.
            shown: Rc::new(move || {
                data.selected_set(&state).map(|s| {
                    let size = count(s.prompt_count, "prompt", "prompts");
                    picked(format!("prompt-set-{}", s.id), s.name, Some((size, false)))
                })
            }),
            render_item: Rc::new(move |entry: &SelectEntry| match find(&entry.value) {
                Some(s) => {
                    let shared = shared_names(&data.set_list()).get(&s.name).copied().unwrap_or(1);
                    let note = prompt_set_note(&s, shared);
                    picked(format!("prompt-set-{}", s.id), s.name, Some((note, false)))
                }
                None => body(text(entry.title.clone())).into_element(),
            }),
            placeholder: "Choose a prompt set",
            accessibility: "Prompt set",
            invalid: Rc::new(move || state.has_error(Field::PromptSet)),
            element: Some(state.element(Field::PromptSet)),
        }
        .into_element()
    });
    form_row("Prompt set", control).error(state.error(Field::PromptSet))
}

// ------------------------------------------------------------------ Requests and Sampling

fn requests_rows(state: State, data: Data, submit: &Rc<dyn Fn()>) -> Vec<AnyElement> {
    let stepper = |field: Field, name: &'static str, min: i64, max: i64| {
        let submit = submit.clone();
        number_stepper(state.value(field), name, min, max)
            .on_card()
            .invalid(move || state.has_error(field))
            .on_change(move |_| state.error(field).set(None))
            .on_blur(move || state.blur(field))
            .on_submit(move || submit())
            .element(&state.element(field))
    };
    // From 768 px the controls sit at the end of their column; a phone's start at its edge.
    let column = |children: Vec<AnyElement>| {
        view().gap(px(6.0)).style_when(move || !is_phone(), |s| s.items_end()).children(children)
    };
    let seqs = column(vec![
        stepper(Field::Concurrency, "seqs", 1, 4096).into_element(),
        seqs_choices(
            move || state.get(Field::Concurrency).trim().parse::<i64>().unwrap_or(-1),
            move |n| state.set(Field::Concurrency, n.to_string()),
        )
        .into_element(),
    ]);
    vec![
        form_row("Seqs", seqs)
            .align_end()
            .error(state.error(Field::Concurrency))
            .hint_when(move || {
                let cloud = data.selected_endpoint(&state).is_some_and(|e| e.location == "cloud");
                Some(
                    format!(
                        "{} Changeable while running.",
                        if cloud { "Stay under your provider's rate limit." } else { "Match your server's --max-num-seqs." }
                    )
                    .into(),
                )
            })
            .into_element(),
        form_row("Samples per prompt", stepper(Field::Samples, "samples per prompt", 1, 64))
            .align_end()
            .hint("2 or more for preference pairs.")
            .error(state.error(Field::Samples))
            .into_element(),
    ]
}

fn sampling_rows(state: State, submit: &Rc<dyn Fn()>) -> Vec<AnyElement> {
    vec![
        form_row("Temperature", number_field(state, Field::Temperature, submit))
            .optional()
            .align_end()
            .hint("0–2. Empty: server default.")
            .error(state.error(Field::Temperature))
            .into_element(),
        form_row("Top p", number_field(state, Field::TopP, submit))
            .optional()
            .align_end()
            .hint("0–1. Empty: server default.")
            .error(state.error(Field::TopP))
            .into_element(),
        form_row("Max tokens", number_field(state, Field::MaxTokens, submit))
            .align_end()
            .hint("Includes the thinking trace.")
            .error(state.error(Field::MaxTokens))
            .into_element(),
        form_row(
            "Sent as",
            segmented_mono(
                vec![(MaxTokensField::MaxTokens, "max_tokens"), (MaxTokensField::MaxCompletionTokens, "max_completion_tokens")],
                move || state.token_field.get(),
                move |v| state.token_field.set(v),
                "Token limit field name",
            ),
        )
        .align_end()
        .into_element(),
        form_row("Seed", number_field(state, Field::Seed, submit))
            .optional()
            .align_end()
            .error(state.error(Field::Seed))
            .into_element(),
    ]
}

// ------------------------------------------------------------------ Advanced

fn advanced_rows(state: State, submit: &Rc<dyn Fn()>) -> Vec<AnyElement> {
    vec![
        form_row("Extra request body", area(state, Field::ExtraBody, true).mono().placeholder("{\"chat_template_kwargs\": {\"enable_thinking\": true}}"))
            .stacked()
            .optional()
            .hint("Merged into every request.")
            .error(state.error(Field::ExtraBody))
            .into_element(),
        form_row("Stream responses", view().child(checkbox_with("", move || state.stream.get(), move |on| state.stream.set(on))))
            .align_end()
            .hint("Needed for live progress and time to first token.")
            .into_element(),
        form_row("Thinking opens with", field_for(state, Field::ThinkOpen, submit, true).mono())
            .error(state.error(Field::ThinkOpen))
            .into_element(),
        form_row("Thinking closes with", field_for(state, Field::ThinkClose, submit, true).mono())
            .hint("Only for inline <think> traces.")
            .error(state.error(Field::ThinkClose))
            .into_element(),
        form_row("Attempts per sample", number_field(state, Field::MaxAttempts, submit))
            .align_end()
            .error(state.error(Field::MaxAttempts))
            .into_element(),
        form_row("Request timeout (s)", number_field(state, Field::Timeout, submit))
            .align_end()
            .error(state.error(Field::Timeout))
            .into_element(),
        form_row("Pause after failures in a row", number_field(state, Field::PauseAfter, submit))
            .align_end()
            .hint("0 never pauses.")
            .error(state.error(Field::PauseAfter))
            .into_element(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_list_answer_counts_only_for_the_endpoint_still_chosen() {
        assert!(refresh_applies("4", 4));
        assert!(!refresh_applies("7", 4));
        assert!(!refresh_applies("", 4));
    }
}
