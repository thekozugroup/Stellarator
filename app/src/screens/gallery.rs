//! Every component of `ui` on one screen, for checking them by eye and in screenshots. Not in
//! the navigation: start the desktop app with `STELLARATOR_GALLERY=1` to open it (Settings /
//! Components).

use nsh_core::prelude::*;

use crate::theme::radius;
use crate::theme::text::{body, meta};
use crate::ui::action_bar::{action_bar, action_bar_tinted};
use crate::ui::blocks::{bar, empty_block, empty_text, error_block};
use crate::ui::button::{Button, Size, Variant, button, external_link};
use crate::ui::chip::{chip, chip_row};
use crate::ui::choice::{checkbox, line_tabs, radio_group, segmented, switch, toggle};
use crate::ui::field::{field, form_disclosure, form_group, form_row, text_area, text_field};
use crate::ui::icon;
use crate::ui::info_tip::info_tip;
use crate::ui::model_text::{ModelTextStyle, model_text, model_text_with, text_mode_toggle};
use crate::ui::overlay::{
    Alert, DialogWidth, alert, dialog, dialog_description, dialog_footer, dialog_header, side_sheet, side_sheet_width, toast_error_with,
    toast_info,
};
use crate::ui::page::{page, panel, panel_tight};
use crate::ui::pickers::{RichSelect, TypedCombobox};
use crate::ui::stepper::{number_stepper, seqs_choices};
use crate::ui::plasma::plasma;
use crate::ui::progress::progress_bar_full;
use crate::ui::select::{MenuEntry, combobox_field, menu_button, menu_destructive, menu_entry, menu_with_open, option, select_button, select_field, select_row};
use crate::ui::status::{Tone, pill, run_status_badge, status_badge};
use crate::ui::tiles::{tile, tiles};
use crate::ui::tooltip::{copy_button, kbd};

const SAMPLE: &str = "## Answer\n\nThe sum is $$\\frac{n(n+1)}{2}$$ for the first *n* integers, so:\n\n\\[ \\sum_{k=1}^{100} k = 5050 \\]\n\n- `code` in text\n- a [link](https://example.com)\n\n```python\nprint(sum(range(101)))\n```\n\n> A quote.";

/// The gallery screen.
pub fn screen() -> impl IntoElement {
    let name = signal(String::from("DGX Spark"));
    let notes = signal(String::new());
    let pick = signal(Some(SharedString::from("vllm")));
    let model = signal(None::<SharedString>);
    let tab = signal(0_u8);
    let seg = signal(1_u8);
    let on = signal(true);
    let checked = signal(true);
    let radio = signal(0_u8);
    let open_dialog = signal(false);
    let open_alert = signal(false);
    let open_sheet = signal(false);
    let open_wide_sheet = signal(false);
    let open_wide_dialog = signal(false);
    let disclosure = signal(false);
    let steps = signal(String::from("16"));
    let rich = signal(Some(SharedString::from("1")));
    let typed_model = signal(String::from("qwen3-32b"));
    let typed_query = signal(String::new());
    let run_pick = signal(Some(SharedString::from("1")));
    let menu_shown = signal(false);

    let buttons = view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(button("Primary").leading(icon::PLUS))
        .child(button("Secondary").variant(Variant::Secondary))
        .child(button("Outline").variant(Variant::Outline))
        .child(button("Ghost").variant(Variant::Ghost))
        .child(button("Destructive").variant(Variant::Destructive))
        .child(button("Link").variant(Variant::Link))
        .child(button("Small").variant(Variant::Secondary).size(Size::Sm))
        .child(button("Extra small").variant(Variant::Secondary).size(Size::Xs))
        .child(button("Large").size(Size::Lg))
        .child(button("Saving").busy(|| true, "Saving…"))
        .child(button("Disabled").disabled(|| true))
        .child(Button::icon(icon::ELLIPSIS, "More").size(Size::IconSm))
        .child(Button::icon(icon::PLUS, "Add").variant(Variant::Secondary).size(Size::IconRound).round());

    let menu = menu_button(
        "More actions",
        vec![
            menu_entry(Some(icon::PENCIL), "Edit…", || {}),
            menu_entry(Some(icon::REFRESH), "Check now", || {}),
            MenuEntry::Separator,
            menu_destructive(Some(icon::TRASH), "Delete…", || {}),
        ],
        Some(176.0),
    );

    let fields = form_group(vec![
        form_row("Name", text_field(name).placeholder("DGX Spark")).into_element(),
        form_row("Base URL", text_field(signal(String::new())).mono().placeholder("http://192.168.1.50:8000/v1"))
            .hint("From Docker, this machine is http://host.docker.internal:PORT/v1.")
            .into_element(),
        form_row("Seqs", text_field(signal(String::from("16"))).numeric().width(96.0)).align_end().info("How many requests run at once.").into_element(),
        form_row(
            "Server",
            select_button(
                vec![option("vllm", "vLLM"), option("sglang", "SGLang"), option("ollama", "Ollama")],
                pick,
                |v| match v {
                    "vllm" => "vLLM".into(),
                    "sglang" => "SGLang".into(),
                    _ => "Ollama".into(),
                },
                "Choose a server",
                "Server",
            ),
        )
        .into_element(),
        form_row(
            "Model",
            combobox_field(
                vec![option("qwen3-32b", "qwen3-32b"), option("mock-flaky", "mock-flaky"), option("mock-reasoning", "mock-reasoning")],
                model,
                "Search models…",
                "Model",
            ),
        )
        .into_element(),
        form_row(
            "Seqs",
            view()
                .gap(px(6.0))
                .items_end()
                .child(number_stepper(steps, "seqs", 1, 4096).on_card())
                .child(seqs_choices(move || steps.get().trim().parse().unwrap_or(-1), move |n| steps.set(n.to_string()))),
        )
        .align_end()
        .hint("A stepper and the common values.")
        .into_element(),
        form_row(
            "Endpoint",
            RichSelect {
                items: std::rc::Rc::new(|| vec![option("1", "Local vLLM"), option("2", "OpenRouter")]),
                value: std::rc::Rc::new(move || rich.get()),
                on_change: std::rc::Rc::new(move |v| rich.set(Some(v))),
                shown: std::rc::Rc::new(move || {
                    rich.get().map(|v| {
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(plasma(&format!("endpoint-{v}"), 5, 20.0, radius::XS, false))
                            .child(body(text(if v == "1" { "Local vLLM" } else { "OpenRouter" })))
                            .into_element()
                    })
                }),
                render_item: std::rc::Rc::new(|e| {
                    view()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .child(plasma(&format!("endpoint-{}", e.value), 5, 20.0, radius::XS, false))
                        .child(body(text(format!("{} · Local", e.title))))
                        .into_element()
                }),
                placeholder: "Choose an endpoint",
                accessibility: "Endpoint",
                invalid: std::rc::Rc::new(|| false),
                element: None,
            },
        )
        .hint("A pop-up button with tiles in its rows.")
        .into_element(),
        form_row(
            "Typed model",
            TypedCombobox {
                items: std::rc::Rc::new(move || {
                    let q = typed_query.get().trim().to_owned();
                    let mut items = Vec::new();
                    if !q.is_empty() && q != "qwen3-32b" {
                        items.push(option(format!("typed:{q}"), format!("Use {q}")));
                    }
                    items.push(option("qwen3-32b", "qwen3-32b").group("Listed by this endpoint"));
                    items
                }),
                value: std::rc::Rc::new(move || vec![SharedString::from(typed_model.get())]),
                on_change: std::rc::Rc::new(move |v| {
                    let v = v.to_string();
                    typed_model.set(v.strip_prefix("typed:").unwrap_or(&v).to_owned());
                }),
                on_query: std::rc::Rc::new(move |q| typed_query.set(q.to_owned())),
                placeholder: "Choose or type a model id",
                accessibility: "Model id",
                empty: std::rc::Rc::new(|| "No models listed. Type a model id.".to_owned()),
                mono: true,
                disabled: std::rc::Rc::new(|| false),
                invalid: std::rc::Rc::new(|| false),
                element: None,
                render_item: None,
            },
        )
        .hint("A combobox that also takes what is typed.")
        .into_element(),
        form_row("Notes", text_area(notes).placeholder("Anything to remember")).stacked().into_element(),
        form_row(
            "Config",
            select_row(vec![option("main", "main"), option("socratic", "socratic")], move || Some(SharedString::from("main")), |_| {}, "Config")
                .fill()
                .mono(),
        )
        .into_element(),
        form_row("Pasted", text_area(signal(String::from("Mono at the regular size"))).mono_body().height(120.0)).stacked().into_element(),
    ])
    .dynamic_rows(|| vec![form_row("Comes and goes", body(text("A row after the fixed ones"))).into_element()]);

    let choices = view()
        .gap(px(12.0))
        .child(line_tabs(vec![(0_u8, "Samples", None), (1, "Failures", None), (2, "Settings", None)], tab))
        .child(segmented(vec![(0_u8, "Light"), (1, "Dark"), (2, "System")], seg, "Theme", false))
        .child(view().flex_row().gap(px(16.0)).items_center().child(toggle("Bold", on)).child(switch("Stream", on)).child(checkbox("Remove the saved key", checked)))
        .child(radio_group(vec![(0_u8, "Local"), (1, "Cloud")], radio));

    let states = view()
        .gap(px(12.0))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(16.0))
                .child(pill(Tone::Positive, "Reachable"))
                .child(pill(Tone::Negative, "Unreachable"))
                .child(pill(Tone::Attention, "Paused"))
                .child(pill(Tone::Plain, "Not checked"))
                .child(status_badge(Tone::Positive, None, "Running"))
                .child(run_status_badge(crate::api::types::RunStatus::Completed))
                .child(kbd(crate::shortcuts::label(crate::shortcuts::BACK)))
                .child(copy_button("value", "Copy value"))
                .child(info_tip("Seqs", "How many requests run at once."))
                .child(external_link("Get a key", "https://example.com")),
        )
        .child(chip_row(vec![
            chip("qwen3-32b").mono().into_element(),
            chip("Local vLLM").leading(plasma("endpoint-1", 6, 20.0, 10.0, false)).on_press(|| {}).into_element(),
            chip("gsm8k train").into_element(),
        ]))
        .child(tiles(vec![
            tile("of 7,473 samples", || "3,759".into()).span().child(progress_bar_full(3759, 37, 7473)).into_element(),
            tile("Output tokens/s", || "320".into()).into_element(),
            tile("Failed", || "37".into()).into_element(),
        ]))
        .child(error_block("Couldn’t load runs. The server answered 500.", Some(std::rc::Rc::new(|| {}))))
        .child(empty_block(vec![empty_text("No runs yet.").into_element()], Some(button("New run").into_element())))
        .child(view().flex_row().gap(px(8.0)).child(bar(160.0, 16.0)).child(bar(96.0, 16.0)));

    let overlays = view()
        .flex_row()
        .flex_wrap()
        .gap(px(8.0))
        .child(button("Dialog").variant(Variant::Secondary).on_press(move || open_dialog.set(true)))
        .child(button("Alert").variant(Variant::Secondary).on_press(move || open_alert.set(true)))
        .child(button("Sheet").variant(Variant::Secondary).on_press(move || open_sheet.set(true)))
        .child(button("Toast").variant(Variant::Secondary).on_press(|| toast_info("Copied the base URL.")))
        .child(
            button("Toast with a second line")
                .variant(Variant::Secondary)
                .on_press(|| toast_error_with("Couldn’t change seqs. The server is not answering.", "Seqs stays at 16.")),
        )
        .child(button("Wide dialog").variant(Variant::Secondary).on_press(move || open_wide_dialog.set(true)))
        .child(button("Wide sheet").variant(Variant::Secondary).on_press(move || open_wide_sheet.set(true)))
        .child(menu)
        .child(dialog(open_dialog, "Rename run", move || {
            view()
                .gap(px(16.0))
                .child(dialog_header("Rename run", Some(dialog_description("The new name shows in the list."))))
                .child(form_group(vec![form_row("Name", text_field(name)).into_element()]).in_dialog())
                .child(dialog_footer(vec![
                    button("Cancel").variant(Variant::Secondary).on_press(move || open_dialog.set(false)),
                    button("Rename").on_press(move || open_dialog.set(false)),
                ]))
                .into_element()
        })
        .width(DialogWidth::Sm))
        .child(alert(
            open_alert,
            || Alert {
                title: "Delete “DGX Spark”?".into(),
                description: "No runs use this endpoint.".into(),
                action: "Delete endpoint".into(),
                busy_label: "Deleting…".into(),
                destructive: true,
            },
            || false,
            || None,
            move || open_alert.set(false),
        ))
        .child(side_sheet(open_sheet, "Details", move || {
            view().p(px(20.0)).gap(px(12.0)).child(body(text("A side sheet."))).into_element()
        }))
        .child(side_sheet_width(open_wide_sheet, "Sample", 720.0, move || {
            view().p(px(20.0)).gap(px(12.0)).child(body(text("A side sheet up to 720 px wide (the sample viewer)."))).into_element()
        }))
        .child(
            dialog(open_wide_dialog, "Export dataset", move || {
                view()
                    .gap(px(16.0))
                    .child(dialog_header("Export dataset", Some(dialog_description("An 896 px dialog: the export dialogs."))))
                    .child(dialog_footer(vec![button("Cancel").variant(Variant::Secondary).on_press(move || open_wide_dialog.set(false))]))
                    .into_element()
            })
            .width(DialogWidth::FourXl),
        );

    page("Components")
        .bottom_bar(|| action_bar(vec![meta(text("3 prompts · 6 samples")).into_element(), button("Start run").into_element()]).into_element())
        .child(
            view()
                .gap(px(24.0))
                .child(panel(Some("Buttons".into()), None, None, buttons))
                .child(panel(Some("Fields".into()), None, None, fields))
                .child(panel(Some("Choices".into()), None, None, choices))
                .child(panel(Some("States".into()), None, None, states))
                .child(panel(Some("Overlays".into()), None, None, overlays))
                .child(form_disclosure("Headers", || "Optional".into(), disclosure, || vec![body(text("A row")).p(px(12.0)).into_element()], false))
                .child(field("Label above", text_field(signal(String::new())).search().placeholder("Search models…")).hint("A hint under the field."))
                .child(panel(Some("Model text".into()), None, Some(text_mode_toggle().into_element()), model_text(|| SAMPLE.to_owned(), None)))
                // The preference review's pieces: a card's 13 px muted text, a full-width pop-up
                // button on the dialog fill, the menu that reports it is open, the tinted bar.
                .child(panel(
                    Some("Review pieces".into()),
                    None,
                    None,
                    view()
                        .gap(px(12.0))
                        .child(model_text_with(|| "A *rejected* response, in muted 13 px text.".to_owned(), None, ModelTextStyle::small().muted_when(|| true)))
                        .child(select_field(
                            vec![option("1", "Distill gsm8k (Qwen3-32B)"), option("2", "Baseline (Llama-3.1-8B)")],
                            run_pick,
                            |v| if v == "1" { "Distill gsm8k (Qwen3-32B)".into() } else { "Baseline (Llama-3.1-8B)".into() },
                            "",
                            "Chosen run",
                            || crate::theme::token("on-dialog-control")(),
                        ))
                        .child(menu_with_open(
                            Button::icon(icon::ELLIPSIS, "More actions").variant(Variant::Secondary).without_tooltip(),
                            vec![menu_destructive(Some(icon::TRASH), "Delete preference set…", || {})],
                            Some(224.0),
                            menu_shown,
                        ))
                        .child(action_bar_tinted(
                            vec![body(text("A chosen over B.")).into_element(), Button::icon(icon::KEYBOARD, "Keyboard shortcuts").muted().into_element()],
                            "review-bar-tint",
                        )),
                ))
                .child(panel_tight(Some("Tight card".into()), Some("Body padding of 8 px".into()), None, model_text_with(|| SAMPLE.to_owned(), None, ModelTextStyle::small())))
                .child(view().h(px(1.0)).rounded(px(radius::XS))),
        )
}
