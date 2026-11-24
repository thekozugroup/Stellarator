//! The run's facts as tiles (frontend/src/components/runs/run-progress.tsx; DESIGN.md, "Layout"):
//! samples done of the total at the true ratio in the wide tile with the bar (the failed count names
//! its red part); then the rate in output tokens and in samples, and the time left. A run that is
//! not going shows averages over the time it spent running, and how long that was.
//!
//! A run with tools has one more tile, the tool calls it ran (and how many per finished sample),
//! before the time. In code mode there are two: "Code runs" (the code tool's executions) and
//! "Tool calls" (the calls the code made), each with its figure per finished sample.
//!
//! The grid: five columns from 1024 px (the samples tile spans two), three from 640 px and two
//! below (the samples tile on a row of its own; on phones the time on one too). With the tool
//! calls tile: six, four and two (on phones the tool calls beside the time). With both code mode
//! tiles: seven in one row from 1024 px; from 640 px four columns, the time two wide beside the
//! rate and the samples a minute, the code runs and tool calls two wide each on the row below; on
//! phones the two beside each other and the time on a row of its own.

use nsh_base::theme;
use nsh_core::prelude::*;

use super::data::{duration_parts, int_f, is_active};
use crate::api::types::{RunLive, RunStatus, ToolMode};
use crate::format::{DASH, dec1, int};
use crate::theme::fonts::INTER_DISPLAY;
use crate::theme::text::{body, meta};
use crate::theme::{muted_ink, radius, token};
use crate::ui::page::window_width;
use crate::ui::progress::progress_bar_full;

/// The gap between tiles.
const GAP: f32 = 8.0;

/// A figure in the tiles' style: Inter Display 28 px at 600, tabular.
fn figure(words: impl Into<SharedString>) -> Text {
    text(words.into())
        .font_family(INTER_DISPLAY.family)
        .text_size(px(28.0))
        .line_height(px(31.0))
        .font_weight(FontWeight::SEMIBOLD)
        .letter_spacing(px(-0.84))
        .tabular_nums()
        .number_of_lines(1)
        .text_color(move || theme().foreground)
}

/// A unit after a number in a duration figure: 15 px at 500, muted, normal tracking, a thin space
/// (the web UI's 2 px) before it.
fn unit(words: &str) -> Text {
    text(format!("\u{2009}{words}")).text_size(px(15.0)).line_height(px(16.5)).font_weight(FontWeight::MEDIUM).letter_spacing(px(0.0)).text_color(muted_ink)
}

/// A duration as a figure: the numbers at figure size, the units small and muted ("2 h 14 min"); a
/// gray dash when unknown.
fn duration_figure(seconds: Option<f64>) -> Text {
    let Some(seconds) = seconds else { return figure(DASH).text_color(muted_ink) };
    let Some(parts) = duration_parts(seconds) else {
        return figure("<1").child(unit("s"));
    };
    let mut t = figure("");
    for (i, (n, u)) in parts.into_iter().enumerate() {
        t = t.child(text(format!("{}{}", if i > 0 { " " } else { "" }, int(n)))).child(unit(u));
    }
    t
}

/// A tile's width: `span` of `cols` columns in a row `row` points wide, with the gaps it spans.
pub fn tile_width(row: f32, cols: f32, span: f32) -> f32 {
    let col = (row - (cols - 1.0) * GAP) / cols;
    span * col + (span - 1.0) * GAP
}

/// The grid's columns and each tile's span, as the window's width sets them (the samples tile, the
/// rate, the samples a minute, the time, the tool calls, the second tool tile). `extra`: how many
/// tool tiles the run has (none, the tool calls, or the code runs and the tool calls).
fn grid(extra: usize) -> (f32, [f32; 6]) {
    let w = window_width();
    match (w >= 1024.0, w >= 640.0, extra) {
        (true, _, 0) => (5.0, [2.0, 1.0, 1.0, 1.0, 0.0, 0.0]),
        (true, _, 1) => (6.0, [2.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        (true, _, _) => (7.0, [2.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        (false, true, 0) => (3.0, [3.0, 1.0, 1.0, 1.0, 0.0, 0.0]),
        (false, true, 1) => (4.0, [4.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        (false, true, _) => (4.0, [4.0, 1.0, 1.0, 2.0, 2.0, 2.0]),
        (false, false, 0) => (2.0, [2.0, 1.0, 1.0, 2.0, 0.0, 0.0]),
        (false, false, 1) => (2.0, [2.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        (false, false, _) => (2.0, [2.0, 1.0, 1.0, 2.0, 1.0, 1.0]),
    }
}

/// A tile: the figure over its label, with what goes under it. It is `span` of the grid's columns
/// wide, in a row `row` points wide.
fn tile(value: AnyElement, label: AnyElement, under: Option<AnyElement>, span: f32, row: Signal<f32>, extra: usize) -> AnyElement {
    view()
        .w(move || {
            let (cols, _) = grid(extra);
            px(tile_width(row.get(), cols, span).max(0.0))
        })
        .flex_none()
        .min_w(px(0.0))
        .gap(px(4.0))
        .p(px(16.0))
        .rounded(px(radius::XL))
        .bg(move || theme().muted)
        .child(value)
        .child(label)
        .child(under.map(|u| view().mt(px(8.0)).child(u)))
        .into_element()
}

fn label(words: impl Into<SharedString>) -> AnyElement {
    meta(text(words.into())).number_of_lines(1).into_element()
}

/// The tiles of `live` (reactive).
pub fn run_progress(live: impl Fn() -> RunLive + 'static) -> impl IntoElement {
    let row_w = signal(0.0_f32);
    let tiles = dynamic(move || {
        let live = live();
        let run = &live.run;
        let code = run.tools.as_ref().is_some_and(|t| t.mode == ToolMode::Code);
        let extra = match (&run.tools, code) {
            (None, _) => 0,
            (Some(_), false) => 1,
            (Some(_), true) => 2,
        };
        let (_, spans) = grid(extra);
        let tile = |value: AnyElement, label: AnyElement, under: Option<AnyElement>, span: f32, row: Signal<f32>| {
            tile(value, label, under, span, row, extra)
        };
        let active = is_active(run.status);
        let finished = run.status == RunStatus::Completed;
        let samples = tile(
            view()
                .flex_row()
                .items_baseline()
                .gap(px(6.0))
                .min_w(px(0.0))
                .child(figure(int(run.done)))
                .child(body(text(format!("of {}", int(run.total)))).tabular_nums().number_of_lines(1).text_color(muted_ink))
                .into_element(),
            view()
                .flex_row()
                .justify_between()
                .gap(px(12.0))
                .child(meta(text("Samples done")).number_of_lines(1).flex_shrink(1.0))
                .child((run.failed > 0).then(|| meta(text(format!("{} failed", int(run.failed)))).tabular_nums().text_color(token("negative-ink"))))
                .into_element(),
            Some(progress_bar_full(run.done, run.failed, run.total).into_element()),
            spans[0],
            row_w,
        );
        // Averages over the time the run spent running; under a second of it, or with nothing
        // done, there is no rate yet.
        let measured = run.active_seconds >= 1.0 && run.done > 0;
        let dash = || figure(DASH).text_color(muted_ink).into_element();
        let (rate, per_min, time): (AnyElement, AnyElement, AnyElement) = if active {
            (
                tile(figure(int_f(live.tokens_per_s)).into_element(), label("Output tokens/s"), None, spans[1], row_w),
                tile(figure(dec1(live.samples_per_min)).into_element(), label("Samples/min"), None, spans[2], row_w),
                tile(duration_figure(live.eta_seconds).into_element(), label("Time left"), None, spans[3], row_w),
            )
        } else {
            let tps = measured.then(|| run.completion_tokens as f64 / run.active_seconds);
            let spm = measured.then(|| run.done as f64 / run.active_seconds * 60.0);
            (
                tile(tps.map(|v| figure(int_f(v)).into_element()).unwrap_or_else(dash), label("Avg output tokens/s"), None, spans[1], row_w),
                tile(spm.map(|v| figure(dec1(v)).into_element()).unwrap_or_else(dash), label("Avg samples/min"), None, spans[2], row_w),
                tile(
                    duration_figure((run.active_seconds > 0.0).then_some(run.active_seconds)).into_element(),
                    label(if finished { "Took" } else { "Run time" }),
                    None,
                    spans[3],
                    row_w,
                ),
            )
        };
        // The tool calls so far, and per finished sample.
        // In code mode the code tool's executions count as `tool_calls` and the calls the code made
        // as `inner_tool_calls`: one tile each. Otherwise the tool calls alone.
        let counter = |name: &'static str, total: i64, span: f32| {
            let per = (run.done > 0).then(|| format!("{} per sample", dec1(total as f64 / run.done as f64)));
            tile(
                // "1.5 per sample" goes under a wide figure when it does not fit beside it.
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_baseline()
                    .gap_x(px(6.0))
                    .min_w(px(0.0))
                    .child(figure(int(total)))
                    .child(per.map(|p| body(text(p)).tabular_nums().number_of_lines(1).text_color(muted_ink)))
                    .into_element(),
                label(name),
                None,
                span,
                row_w,
            )
        };
        let mut counters: Vec<AnyElement> = Vec::new();
        if code {
            counters.push(counter("Code runs", run.tool_calls, spans[4]));
            counters.push(counter("Tool calls", run.inner_tool_calls, spans[5]));
        } else if run.tools.is_some() {
            counters.push(counter("Tool calls", run.tool_calls, spans[4]));
        }
        let w = window_width();
        let row = |tiles: Vec<AnyElement>| view().flex_row().gap(px(GAP)).children(tiles).into_element();
        let rows = if counters.is_empty() {
            if w >= 1024.0 {
                vec![row(vec![samples, rate, per_min, time])]
            } else if w >= 640.0 {
                vec![row(vec![samples]), row(vec![rate, per_min, time])]
            } else {
                vec![row(vec![samples]), row(vec![rate, per_min]), row(vec![time])]
            }
        } else if code {
            let mut it = counters.into_iter();
            let (runs, calls) = (it.next().expect("code runs tile"), it.next().expect("tool calls tile"));
            if w >= 1024.0 {
                vec![row(vec![samples, rate, per_min, runs, calls, time])]
            } else if w >= 640.0 {
                vec![row(vec![samples]), row(vec![rate, per_min, time]), row(vec![runs, calls])]
            } else {
                vec![row(vec![samples]), row(vec![rate, per_min]), row(vec![runs, calls]), row(vec![time])]
            }
        } else {
            let calls = counters.pop().expect("tool calls tile");
            if w >= 1024.0 {
                vec![row(vec![samples, rate, per_min, calls, time])]
            } else if w >= 640.0 {
                vec![row(vec![samples]), row(vec![rate, per_min, calls, time])]
            } else {
                vec![row(vec![samples]), row(vec![rate, per_min]), row(vec![calls, time])]
            }
        };
        view().gap(px(GAP)).children(rows).into_element()
    });
    // The tiles' widths come from the row's: the grid's columns share it, as CSS grid shares it.
    view().on_layout(move |b: &Bounds<Points>| row_w.set(b.size.width.0)).child(tiles)
}

#[cfg(test)]
mod tests {
    use super::tile_width;

    #[test]
    fn tiles_share_the_row_as_grid_columns() {
        // Five columns in 1,136 px: 220.8 each, the samples tile two of them and a gap.
        assert!((tile_width(1136.0, 5.0, 2.0) - 449.6).abs() < 0.01);
        assert!((tile_width(1136.0, 5.0, 1.0) - 220.8).abs() < 0.01);
        assert_eq!(tile_width(358.0, 2.0, 2.0), 358.0);
    }
}
