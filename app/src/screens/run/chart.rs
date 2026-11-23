//! The run's signature chart (frontend/src/components/runs/throughput-chart.tsx; DESIGN.md, "Data
//! visualization"): output tokens per second as a periwinkle line over a flat 14% tint, with the
//! seqs setting as a gray step line on its own scale, so the effect of changing seqs shows; once
//! there are readings on both sides of the last change a flag says by how much. Dashed hairline
//! grid, mono axis labels, paused time cut out of the axis and marked with a dashed line, a readout
//! under the pointer. Nothing animates.
//!
//! Drawn on a canvas, with the labels, the flag and the readout as views over it: nsh's `Plot`
//! sizes its axes itself and keeps the time labels inside the plot's ends, where the web UI's
//! axes are fixed widths and the last label may run into the seqs axis; and its marks take dashes
//! only from a JS style. The geometry is recharts' for the web UI's chart: 26 px above the plot,
//! 4 px right of it, a 30 px time axis, value axes as wide as their labels.

use std::rc::Rc;

use chrono::{DateTime, Utc};
use nsh_base::theme;
use nsh_core::prelude::*;

use super::chart_math::{
    MONO_CHAR_W, Point, Reading, SeqsEffect, Tick, ago_label, ago_ticks, axis_width, clock_ticks, effect_label, label_left, layout,
    seqs_effect, tick_step, value_ticks, visible_ticks,
};
use crate::api::types::MetricPoint;
use crate::format::{date_time_seconds, int, local_day, month_day, time, time_hm};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::body;
use crate::theme::{muted_ink, radius, token};
use crate::ui::material::{material_float, material_thick, popover_ink};
use crate::ui::page::window_width;

/// The current rate and setting and when they were read, drawn as the newest point while the run
/// goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveNow {
    pub tps: f64,
    pub seqs: f64,
    pub active: f64,
    /// When they were read (ms since the epoch).
    pub at: i64,
}

/// Space above the plot (the axis names).
const TOP: f32 = 26.0;
/// Space right of the plot.
const RIGHT: f32 = 4.0;
/// The time axis under the plot.
const X_AXIS: f32 = 30.0;

/// What the chart draws, worked out from the readings.
#[derive(Debug, Clone, PartialEq)]
struct Model {
    data: Vec<Point>,
    breaks: Vec<f64>,
    effect: Option<SeqsEffect>,
    ticks: Vec<(Tick, String)>,
    tps_ticks: [f64; 5],
    seqs_ticks: [f64; 5],
    left_w: f32,
    right_w: f32,
    x_min: f64,
    x_max: f64,
    multi_day: bool,
    label: String,
}

fn at(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap_or_default()
}

/// The readings as the chart lays them out; `None` without throughput to draw.
fn model(metrics: &[MetricPoint], live: Option<LiveNow>) -> Option<Model> {
    let mut pts: Vec<Reading> = metrics
        .iter()
        .map(|m| Reading { t: m.t.timestamp_millis(), tps: m.tokens_per_s, seqs: m.concurrency as f64, active: m.active as f64 })
        .collect();
    if let (Some(live), Some(last)) = (live, pts.last().copied())
        && live.at > last.t + 500
    {
        pts.push(Reading { t: live.at, tps: live.tps, seqs: live.seqs, active: live.active });
    }
    let l = layout(&pts);
    let has_data = l.data.len() >= 2 && l.data.iter().any(|d| d.tps.unwrap_or(0.0) > 0.0);
    if !has_data {
        return None;
    }
    let (first, last) = (l.data[0], l.data[l.data.len() - 1]);
    let step = tick_step(last.x - first.x);
    let multi_day = local_day(at(first.t)) != local_day(at(last.t));
    let mut ticks: Vec<Tick> = if live.is_some() {
        let (current, earlier) = l.sessions.split_last().expect("a session");
        let mut t = clock_ticks(earlier, step);
        t.extend(ago_ticks(current, step));
        t
    } else {
        clock_ticks(&l.sessions, step)
    };
    ticks.sort_by(|a, b| a.x.total_cmp(&b.x));
    let labelled: Vec<(Tick, String)> = ticks
        .into_iter()
        .map(|k| {
            let label = if k.ago { ago_label(last.t - k.t) } else { clock_label(&k, step, multi_day) };
            (k, label)
        })
        .collect();
    let peak = l.data.iter().filter_map(|d| d.tps).fold(0.0_f64, f64::max);
    let seqs_max = l.data.iter().filter_map(|d| d.seqs).fold(0.0_f64, f64::max);
    let tps_ticks = value_ticks(peak, false);
    let seqs_ticks = value_ticks(seqs_max, true);
    let labels = |t: &[f64; 5]| t.iter().map(|v| int(*v as i64)).collect::<Vec<_>>();
    let left_w = axis_width(&labels(&tps_ticks)).max(44.0) as f32;
    let right_w = axis_width(&labels(&seqs_ticks)).max(36.0) as f32;
    let effect = seqs_effect(&l.data);
    let span = |ms: i64| if multi_day { crate::format::date_time(at(ms)) } else { time_hm(at(ms)) };
    let paused = match l.breaks.len() {
        0 => String::new(),
        1 => " Paused once; the paused time is left out.".into(),
        n => format!(" Paused {n} times; the paused time is left out."),
    };
    let effect_text = effect
        .map(|e| {
            let pct = crate::format::js_round(e.change.abs() * 100.0) as i64;
            format!(" Since seqs last changed, throughput is {pct}% {}.", if e.change > 0.0 { "higher" } else { "lower" })
        })
        .unwrap_or_default();
    let label = format!(
        "Output tokens per second from {} to {}: now {} at {} seqs, peak {}.{paused}{effect_text}",
        span(first.t),
        span(last.t),
        int(last.tps.unwrap_or(0.0).round() as i64),
        int(last.seqs.unwrap_or(0.0) as i64),
        int(peak.round() as i64),
    );
    Some(Model {
        data: l.data,
        breaks: l.breaks,
        effect,
        ticks: labelled,
        tps_ticks,
        seqs_ticks,
        left_w,
        right_w,
        x_min: first.x,
        x_max: last.x,
        multi_day,
        label,
    })
}

/// A clock tick's label: seconds for short spans; across days the first tick of each session names
/// the date instead of the time.
fn clock_label(tick: &Tick, step: i64, multi_day: bool) -> String {
    let t = at(tick.t);
    let s = if step >= 86_400_000 || (multi_day && tick.first) {
        month_day(t)
    } else if step < 60_000 {
        time(t)
    } else {
        time_hm(t)
    };
    s.to_uppercase()
}

/// The plot's box in the chart: left, top, right, bottom.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frame {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Frame {
    fn of(m: &Model, w: f32, h: f32) -> Frame {
        Frame { left: m.left_w, top: TOP, right: (w - RIGHT - m.right_w).max(m.left_w + 1.0), bottom: (h - X_AXIS).max(TOP + 1.0) }
    }

    fn x(&self, m: &Model, x: f64) -> f32 {
        let span = (m.x_max - m.x_min).max(1.0);
        self.left + ((x - m.x_min) / span) as f32 * (self.right - self.left)
    }

    fn y(&self, value: f64, top_value: f64) -> f32 {
        let top_value = top_value.max(f64::MIN_POSITIVE);
        self.bottom - (value / top_value) as f32 * (self.bottom - self.top)
    }
}

/// `color` in the appearance shown.
fn resolved(color: AdaptiveColor) -> nsh_core::color::Hsla {
    if dark() { color.dark } else { color.light }
}

/// The points between breaks, each run of them a segment.
fn segments(data: &[Point]) -> Vec<Vec<Point>> {
    let mut out = vec![Vec::new()];
    for p in data {
        if p.tps.is_none() {
            out.push(Vec::new());
        } else {
            out.last_mut().expect("a segment").push(*p);
        }
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// The lines, the fill, the grid, the break marks and, under the pointer, the cursor and the dot.
fn draw(m: &Model, w: f32, h: f32, hover: Option<usize>) -> Painter {
    let f = Frame::of(m, w, h);
    let tps_top = m.tps_ticks[4];
    let seqs_top = m.seqs_ticks[4];
    let mut p = Painter::new();
    // The grid: dashed hairlines at the tokens/s ticks.
    p = p.save().stroke_color(resolved(token("chart-grid")())).line_width(1.0).line_dash(&[px(3.0), px(4.0)]);
    for v in m.tps_ticks {
        let y = f.y(v, tps_top).round() + 0.5;
        p = p.begin_path().move_to(f.left, y).line_to(f.right, y).stroke();
    }
    p = p.restore();
    // Paused time: a dashed line in the band, under the lines.
    p = p.save().stroke_color(resolved(theme().input)).line_width(1.0).line_dash(&[px(3.0), px(3.0)]);
    for b in &m.breaks {
        let x = f.x(m, *b).round() + 0.5;
        p = p.begin_path().move_to(x, f.top).line_to(x, f.bottom).stroke();
    }
    p = p.restore();
    let periwinkle = resolved(token("periwinkle")());
    for seg in segments(&m.data) {
        // The fill under the tokens/s line: a flat 14% tint.
        let first = seg[0];
        let mut fill = p.save().begin_path().move_to(f.x(m, first.x), f.bottom);
        for q in &seg {
            fill = fill.line_to(f.x(m, q.x), f.y(q.tps.unwrap_or(0.0), tps_top));
        }
        p = fill
            .line_to(f.x(m, seg[seg.len() - 1].x), f.bottom)
            .close_path()
            .fill_color(periwinkle.opacity(0.14))
            .fill()
            .restore();
        // Seqs: a gray step line on its own scale.
        let mut steps = p
            .save()
            .stroke_color(resolved(token("chart-2")()))
            .line_width(1.5)
            .begin_path()
            .move_to(f.x(m, first.x), f.y(first.seqs.unwrap_or(0.0), seqs_top));
        for pair in seg.windows(2) {
            let x = f.x(m, pair[1].x);
            steps = steps.line_to(x, f.y(pair[0].seqs.unwrap_or(0.0), seqs_top)).line_to(x, f.y(pair[1].seqs.unwrap_or(0.0), seqs_top));
        }
        p = steps.stroke().restore();
        // Tokens/s: the periwinkle line.
        let mut line = p.save().stroke_color(periwinkle).line_width(1.5).begin_path().move_to(f.x(m, first.x), f.y(first.tps.unwrap_or(0.0), tps_top));
        for q in &seg[1..] {
            line = line.line_to(f.x(m, q.x), f.y(q.tps.unwrap_or(0.0), tps_top));
        }
        p = line.stroke().restore();
    }
    // The reading under the pointer: a dashed cursor and a dot on the line.
    if let Some(point) = hover.and_then(|i| m.data.get(i)).filter(|q| q.tps.is_some()) {
        let x = f.x(m, point.x).round() + 0.5;
        p = p
            .save()
            .stroke_color(resolved(theme().input))
            .line_width(1.0)
            .line_dash(&[px(3.0), px(3.0)])
            .begin_path()
            .move_to(x, f.top)
            .line_to(x, f.bottom)
            .stroke()
            .restore();
        let y = f.y(point.tps.unwrap_or(0.0), tps_top);
        p = p
            .save()
            .begin_path()
            .arc(f.x(m, point.x), y, 3.0, 0.0, std::f32::consts::TAU)
            .fill_color(periwinkle)
            .fill()
            .stroke_color(resolved(theme().muted))
            .line_width(2.0)
            .stroke()
            .restore();
    }
    p
}

/// A label in the axes' style: Roboto Mono 12 px, 0.04em tracking, muted.
fn axis_text(words: String) -> Text {
    text(words)
        .font_family(ROBOTO_MONO.family)
        .text_size(px(12.0))
        .line_height(px(16.0))
        .letter_spacing(px(0.48))
        .text_color(muted_ink)
        .number_of_lines(1)
}

/// The chart's height: 180 px on phones, 240 px from 640 px.
fn chart_height() -> f32 {
    if window_width() >= 640.0 { 240.0 } else { 180.0 }
}

/// The chart of `metrics`, with `live` as its newest point while the run goes.
pub fn throughput_chart(metrics: impl Fn() -> Vec<MetricPoint> + 'static, live: impl Fn() -> Option<LiveNow> + 'static) -> impl IntoElement {
    let metrics = Rc::new(metrics);
    let live = Rc::new(live);
    let current = computed(move || model(&metrics(), live()).map(Rc::new));
    dynamic(move || match current.get() {
        Some(m) => chart(m).into_element(),
        None => empty_chart().into_element(),
    })
}

/// No throughput yet: the grid's frame and a line saying what will appear.
fn empty_chart() -> impl IntoElement {
    let lines: Vec<AnyElement> = (0..5)
        .map(|_| view().h(px(0.0)).border_t_1().border_dashed().border_color(token("chart-grid")).into_element())
        .collect();
    view()
        .h(move || px(chart_height()))
        .role(AccessibilityRole::Image)
        .label("Throughput chart, no data yet")
        .child(view().absolute().left(px(44.0)).right(px(0.0)).top(px(24.0)).bottom(px(24.0)).justify_between().children(lines))
        .child(axis_text("0".into()).absolute().left(px(0.0)).bottom(px(16.0)).w(px(32.0)).text_right())
        .child(
            view()
                .absolute()
                .left(px(44.0))
                .right(px(0.0))
                .top(move || px(chart_height() * 0.45 - 9.0))
                .px(px(16.0))
                .flex_row()
                .child(
                    body(text("Throughput appears here once the first tokens arrive."))
                        .text_color(muted_ink)
                        .pr(px(8.0))
                        .bg(move || theme().muted),
                ),
        )
}

/// The chart of a model.
fn chart(m: Rc<Model>) -> impl IntoElement {
    let size = signal((0.0_f32, 0.0_f32));
    let hover = signal(None::<(usize, f32, f32)>);

    let painted = {
        let m = m.clone();
        canvas(move || {
            let (w, h) = size.get();
            draw(&m, w, h, hover.get().map(|(i, _, _)| i))
        })
    };

    let overlay = {
        let m = m.clone();
        dynamic(move || {
            let (w, h) = size.get();
            if w <= 0.0 {
                return view().into_element();
            }
            labels(&m, w, h).into_element()
        })
    };

    let readout = {
        let m = m.clone();
        dynamic(move || {
            let (w, _) = size.get();
            hover.get().and_then(|(i, mx, _)| m.data.get(i).map(|p| readout(*p, m.multi_day, mx, w)))
        })
    };

    let pick = {
        let m = m.clone();
        move |x: f32, y: f32| {
            let (w, h) = size.get_untracked();
            let f = Frame::of(&m, w, h);
            if x < f.left || x > f.right || y < f.top || y > f.bottom {
                hover.set(None);
                return;
            }
            let nearest = m
                .data
                .iter()
                .enumerate()
                .filter(|(_, p)| p.tps.is_some())
                .min_by(|(_, a), (_, b)| (f.x(&m, a.x) - x).abs().total_cmp(&(f.x(&m, b.x) - x).abs()))
                .map(|(i, _)| i);
            hover.set(nearest.map(|i| (i, x, y)));
        }
    };

    view()
        .h(move || px(chart_height()))
        .role(AccessibilityRole::Image)
        .label(m.label.clone())
        .on_layout(move |b: &Bounds<Points>| size.set((b.size.width.0, b.size.height.0)))
        .on_mouse_move(move |e: &MouseEvent| pick(e.position.x.0, e.position.y.0))
        .on_mouse_leave(move |_: &MouseEvent| hover.set(None))
        .child(painted.absolute().inset(px(0.0)).pointer_events(PointerEvents::None))
        .child(view().absolute().inset(px(0.0)).pointer_events(PointerEvents::None).child(overlay))
        .child(view().absolute().inset(px(0.0)).pointer_events(PointerEvents::None).child(readout))
}

/// The axis names, the value and time labels, and the flag, at a chart `w` by `h`.
fn labels(m: &Model, w: f32, h: f32) -> impl IntoElement {
    let f = Frame::of(m, w, h);
    let mut out: Vec<AnyElement> = Vec::new();
    // Each scale named above its own axis, with a stroke of its line.
    out.push(view().absolute().left(px(0.0)).top(px(7.25)).w(px(10.0)).h(px(1.5)).bg(token("periwinkle")).into_element());
    out.push(axis_text("TOK/S".into()).absolute().left(px(16.0)).top(px(0.0)).into_element());
    let right_edge = f.right + m.right_w;
    out.push(view().absolute().left(px(right_edge - 48.0)).top(px(7.25)).w(px(10.0)).h(px(1.5)).bg(token("chart-2")).into_element());
    out.push(axis_text("SEQS".into()).absolute().right(px(w - right_edge)).top(px(0.0)).into_element());
    // Value labels, centered on their ticks, 8 px from the plot.
    for v in m.tps_ticks {
        let y = f.y(v, m.tps_ticks[4]);
        out.push(axis_text(int(v as i64)).absolute().right(px(w - f.left + 8.0)).top(px(y - 8.0)).into_element());
    }
    for v in m.seqs_ticks {
        let y = f.y(v, m.seqs_ticks[4]);
        out.push(axis_text(int(v as i64)).absolute().left(px(f.right + 8.0)).top(px(y - 8.0)).into_element());
    }
    // Time labels under the plot, inside the chart, apart from each other.
    let in_domain: Vec<&(Tick, String)> = m.ticks.iter().filter(|(k, _)| k.x >= m.x_min && k.x <= m.x_max).collect();
    let boxes: Vec<(f64, f64)> = in_domain
        .iter()
        .map(|(k, s)| {
            let lw = s.chars().count() as f64 * MONO_CHAR_W;
            let left = label_left(f.x(m, k.x) as f64, lw, right_edge as f64);
            (left, left + lw)
        })
        .collect();
    for ((_, s), ((left, right), keep)) in in_domain.iter().zip(boxes.iter().zip(visible_ticks(&boxes, 28.0))) {
        if keep {
            out.push(
                axis_text(s.clone())
                    .absolute()
                    .left(px(*left as f32))
                    .w(px((*right - *left) as f32 + 2.0))
                    .top(px(f.bottom + 10.0))
                    .into_element(),
            );
        }
    }
    // The flag: a capsule in the signal's tint at the top of the plot, beside the last change.
    if let Some(e) = m.effect {
        let words = effect_label(e.change);
        let fw = words.chars().count() as f32 * MONO_CHAR_W as f32 + 16.0;
        let x = f.x(m, e.x);
        let left = if x + 6.0 + fw <= f.right { x + 6.0 } else { f.left.max(x - 6.0 - fw) };
        let up = e.change > 0.0;
        out.push(
            view()
                .absolute()
                .left(px(left))
                .top(px(f.top + 4.0))
                .w(px(fw))
                .h(px(20.0))
                .rounded(px(10.0))
                .items_center()
                .justify_center()
                .bg(if up { token("periwinkle-tint") } else { token("coral-tint") })
                .child(axis_text(words).text_color(if up { token("periwinkle-ink") } else { token("coral-ink") }))
                .into_element(),
        );
    }
    view().absolute().inset(px(0.0)).accessibility_elements_hidden(true).children(out)
}

/// The readout of reading `p`: 10 px right of the pointer at `mx` (left of it when there is no
/// room), level with the top of the plot, as recharts places it; over the flag.
fn readout(p: Point, multi_day: bool, mx: f32, w: f32) -> AnyElement {
    const BOX_W: f32 = 180.0;
    let when = if multi_day { date_time_seconds(at(p.t)) } else { time(at(p.t)) };
    let left = if mx + 10.0 + BOX_W > w { (mx - 10.0 - BOX_W).max(0.0) } else { mx + 10.0 };
    let row = |swatch: Option<&'static str>, name: &'static str, value: String, strong: bool| {
        let mut v = text(value).font_family(ROBOTO_MONO.family).text_size(px(12.0)).line_height(px(16.0)).text_color(popover_ink).text_right();
        if strong {
            v = v.font_weight(FontWeight::MEDIUM);
        }
        view()
            .flex_row()
            .items_center()
            .gap(px(16.0))
            .child(
                view()
                    .flex_1()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .pl(px(if swatch.is_some() { 0.0 } else { 16.0 }))
                    .child(swatch.map(|c| view().w(px(10.0)).h(px(2.0)).rounded(px(1.0)).bg(token(c))))
                    .child(text(name).text_size(px(12.0)).line_height(px(16.0)).text_color(muted_ink)),
            )
            .child(v)
    };
    material_float(material_thick(view()))
        .absolute()
        .left(px(left))
        .top(px(TOP))
        .w(px(BOX_W))
        .rounded(px(radius::SM))
        .px(px(12.0))
        .py(px(8.0))
        .gap(px(2.0))
        .child(
            text(when)
                .font_family(ROBOTO_MONO.family)
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(muted_ink)
                .mb(px(2.0)),
        )
        .child(row(Some("periwinkle"), "Output tokens/s", int(p.tps.unwrap_or(0.0).round() as i64), true))
        .child(row(Some("chart-2"), "Seqs", int(p.seqs.unwrap_or(0.0) as i64), false))
        .child(row(None, "In flight", int(p.active.unwrap_or(0.0) as i64), false))
        .into_element()
}
