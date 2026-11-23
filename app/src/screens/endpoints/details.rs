//! Everything about one endpoint, in a side sheet (frontend/src/components/endpoints/
//! endpoint-details-sheet.tsx): the tile and the name with the base URL; the status badge, when it
//! was checked and the key's last characters; the facts as tiles; the models it serves in a box
//! (searchable from 21); Edit, Check now and New run.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::list::{base_url, endpoint_seed};
use super::status::{checked_at, endpoint_state, endpoint_status_badge};
use crate::api::types::EndpointOut as Endpoint;
use crate::data::now_utc;
use crate::format::int;
use crate::theme::text::{body, meta, mono, title};
use crate::theme::{muted_ink, radius};
use crate::ui::button::{Variant, button};
use crate::ui::field::text_field;
use crate::ui::material::dialog_muted;
use crate::ui::overlay::sheet_close;
use crate::ui::plasma::plasma;
use crate::ui::tiles::{tile, tiles};
use crate::ui::tooltip::copy_button;

/// The sheet's content for `endpoint`. `open` closes it; `on_edit` and `on_check` are the row
/// actions; `checking` holds while a check runs.
pub fn details(
    endpoint: Endpoint,
    open: Signal<bool>,
    on_edit: Rc<dyn Fn(Endpoint)>,
    on_check: Rc<dyn Fn(Endpoint)>,
    checking: impl Fn() -> bool + Clone + 'static,
) -> impl IntoElement {
    let query = signal(String::new());
    let state = endpoint_state(&endpoint);
    let failing = endpoint.last_ok == Some(false);
    let searchable = endpoint.models.len() > 20;
    let all_models = Rc::new(endpoint.models.clone());

    let header = view()
        .px(px(20.0))
        .pt(px(20.0))
        .pr(px(56.0))
        .pb(px(16.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .min_w(px(0.0))
                .child(plasma(&endpoint_seed(&endpoint), 8, 40.0, radius::MD, false))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(2.0))
                        .child(title(text(endpoint.name.clone())).role(AccessibilityRole::Header))
                        .child(base_url(&endpoint.base_url)),
                ),
        );

    let mut checked = format!("Checked {}", checked_at(&endpoint, now_utc()));
    if endpoint.has_api_key {
        checked.push_str(" · Key ");
    }
    let facts = view()
        .px(px(20.0))
        .gap(px(16.0))
        .child(
            view()
                .gap(px(8.0))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_x(px(12.0))
                        .gap_y(px(8.0))
                        .child(endpoint_status_badge(&endpoint))
                        .child(
                            body(text(checked))
                                .text_color(muted_ink)
                                .child(endpoint.has_api_key.then(|| {
                                    mono(text(endpoint.api_key_hint.clone().unwrap_or_else(|| "set".into()))).text_color(muted_ink)
                                })),
                        ),
                )
                .child(state.reason.map(|r| body(text(r)).text_color(muted_ink))),
        )
        .child(tiles(vec![
            tile(if endpoint.models.len() == 1 { "Model" } else { "Models" }, {
                let n = endpoint.models.len() as i64;
                move || int(n)
            })
            .background(dialog_muted)
            .into_element(),
            tile("Default seqs", {
                let n = endpoint.default_concurrency;
                move || int(n)
            })
            .background(dialog_muted)
            .into_element(),
            tile(if endpoint.run_count == 1 { "Run" } else { "Runs" }, {
                let n = endpoint.run_count;
                move || int(n)
            })
            .background(dialog_muted)
            .into_element(),
        ]));

    let models_box: AnyElement = if endpoint.models.is_empty() {
        body(text(if failing { "The last check failed, so the model list is empty." } else { "The server lists no models." }))
            .text_color(muted_ink)
            .px(px(16.0))
            .py(px(12.0))
            .rounded(px(radius::XL))
            .bg(dialog_muted)
            .into_element()
    } else {
        let models = all_models.clone();
        let shown = move || {
            let needle = query.get().trim().to_lowercase();
            if needle.is_empty() {
                models.to_vec()
            } else {
                models.iter().filter(|m| m.to_lowercase().contains(&needle)).cloned().collect()
            }
        };
        let total = all_models.len();
        let shown_count = shown.clone();
        view()
            .flex_1()
            .min_h(px(0.0))
            .gap(px(8.0))
            .child(searchable.then(|| {
                text_field(query).search().placeholder("Search models…").accessibility_label("Search models")
            }))
            .child(dynamic(move || {
                let needle = query.get();
                (!needle.trim().is_empty()).then(|| {
                    meta(text(format!("{} of {}", int(shown_count().len() as i64), int(total as i64))))
                        .tabular_nums()
                        .px(px(4.0))
                })
            }))
            .child(
                scroll().flex_shrink(1.0).min_h(px(0.0)).rounded(px(radius::XL)).bg(dialog_muted).child(
                    view().p(px(8.0)).child(dynamic(move || {
                        let list = shown();
                        if list.is_empty() {
                            return body(text(format!("No model matches “{}”.", query.get())))
                                .text_color(muted_ink)
                                .px(px(8.0))
                                .py(px(6.0))
                                .into_element();
                        }
                        view()
                            .children(list.into_iter().map(|m| {
                                view()
                                    .min_h(px(32.0))
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(4.0))
                                    .pl(px(8.0))
                                    .rounded(px(radius::SM))
                                    .hover(|s| s.bg(move || theme().accent))
                                    .child(mono(text(m.clone())).py(px(4.0)).flex_shrink(1.0).min_w(px(0.0)))
                                    .child(copy_button(m.clone(), format!("Copy {m}")))
                                    .into_element()
                            }).collect::<Vec<_>>())
                            .into_element()
                    })),
                ),
            )
            .into_element()
    };
    let models_section = view()
        .flex_1()
        .min_h(px(0.0))
        .gap(px(8.0))
        .px(px(20.0))
        .pt(px(24.0))
        .child(body(text("Models")).font_weight(FontWeight::SEMIBOLD).px(px(4.0)).role(AccessibilityRole::Header))
        .child(models_box);

    let e1 = endpoint.clone();
    let e2 = endpoint.clone();
    let id = endpoint.id;
    let c1 = checking.clone();
    let c2 = checking.clone();
    let footer = view()
        .flex_row()
        .flex_wrap()
        .justify_end()
        .gap(px(8.0))
        .px(px(20.0))
        .pt(px(16.0))
        .pb(px(20.0))
        .child(button("Edit").variant(Variant::Secondary).on_press(move || on_edit(e1.clone())))
        .child(
            button("Check now")
                .variant(if failing { Variant::Primary } else { Variant::Secondary })
                .busy(move || c1(), "Checking…")
                .disabled(move || c2())
                .on_press(move || on_check(e2.clone())),
        )
        .child(button("New run").variant(if failing { Variant::Secondary } else { Variant::Primary }).on_press(move || {
            open.set(false);
            crate::screens::new_run::open(crate::screens::new_run::Prefill { endpoint: Some(id), ..Default::default() });
        }));

    view()
        .flex_1()
        .min_h(px(0.0))
        .child(header)
        .child(facts)
        .child(models_section)
        .child(footer)
        .child(sheet_close(open))
}
