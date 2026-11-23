//! Every component of `ui` on one screen, for checking them by eye and in screenshots. Not in
//! the navigation: start the desktop app with `STELLARATOR_GALLERY=1` to open it (Settings /
//! Components).

use nsh_core::prelude::*;

use crate::theme::radius;
use crate::theme::text::{body, meta};
use crate::ui::action_bar::action_bar;
use crate::ui::blocks::{bar, empty_block, empty_text, error_block};
use crate::ui::button::{Button, Size, Variant, button, external_link};
use crate::ui::chip::{chip, chip_row};
use crate::ui::choice::{checkbox, line_tabs, radio_group, segmented, switch, toggle};
use crate::ui::field::{field, form_disclosure, form_group, form_row, text_area, text_field};
use crate::ui::icon;
use crate::ui::info_tip::info_tip;
use crate::ui::model_text::{model_text, text_mode_toggle};
use crate::ui::overlay::{Alert, DialogWidth, alert, dialog, dialog_description, dialog_footer, dialog_header, side_sheet, toast_info};
use crate::ui::page::{page, panel};
use crate::ui::plasma::plasma;
use crate::ui::progress::progress_bar_full;
use crate::ui::select::{MenuEntry, combobox_field, menu_button, menu_destructive, menu_entry, option, select_button};
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
    let disclosure = signal(false);

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
        form_row("Notes", text_area(notes).placeholder("Anything to remember")).stacked().into_element(),
    ]);

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
        }));

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
                .child(view().h(px(1.0)).rounded(px(radius::XS))),
        )
}
