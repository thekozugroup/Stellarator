//! The throughput chart's arithmetic (frontend/src/components/runs/throughput-chart.tsx), as pure
//! functions: where paused time is cut out of the x axis, the time ticks ("−10M … NOW" while the
//! run goes, clock times otherwise, the date on a session's first tick across days), the value
//! ticks, and the effect of the last seqs change (the flag).

/// One reading as drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// When it was taken (ms since the epoch).
    pub t: i64,
    /// Where it is drawn: its time minus the paused time before it.
    pub x: f64,
    /// `None` only on the break inserted where the run was paused.
    pub tps: Option<f64>,
    pub seqs: Option<f64>,
    pub active: Option<f64>,
}

/// A reading before layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub t: i64,
    pub tps: f64,
    pub seqs: f64,
    pub active: f64,
}

/// A stretch of readings without a pause, and how far it is drawn left of its real time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Session {
    pub from: i64,
    pub to: i64,
    pub shift: i64,
}

/// The readings laid out: the points (breaks included), where the breaks are drawn, the sessions.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub data: Vec<Point>,
    pub breaks: Vec<f64>,
    pub sessions: Vec<Session>,
}

/// A pause leaves a hole in the readings (five readings long, and at least 30 s). The paused time is
/// cut out of the x axis down to a narrow band (3% of the running time, at least 1 s), marked with
/// a dashed line, so a run resumed days later still shows each session at a readable width; a
/// break point in the band ends both lines. Within a session the axis is plain time.
pub fn layout(pts: &[Reading]) -> Layout {
    let mut steps: Vec<i64> = pts.windows(2).map(|w| w[1].t - w[0].t).collect();
    steps.sort_unstable();
    let median = steps.get(steps.len() / 2).copied().unwrap_or(0);
    let gap = 30_000.max(5 * median);
    let running: i64 = steps.iter().filter(|d| **d <= gap).sum();
    let band = 1000.0_f64.max(running as f64 * 0.03);
    let mut out = Layout::default();
    let mut shift: i64 = 0;
    let mut from = pts.first().map(|p| p.t).unwrap_or(0);
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            let prev = pts[i - 1];
            if p.t - prev.t > gap {
                out.sessions.push(Session { from, to: prev.t, shift });
                let x = (prev.t - shift) as f64 + band / 2.0;
                out.data.push(Point { t: prev.t + 1, x, tps: None, seqs: None, active: None });
                out.breaks.push(x);
                shift += p.t - prev.t - band as i64;
                from = p.t;
            }
        }
        out.data.push(Point { t: p.t, x: (p.t - shift) as f64, tps: Some(p.tps), seqs: Some(p.seqs), active: Some(p.active) });
    }
    if let Some(last) = pts.last() {
        out.sessions.push(Session { from, to: last.t, shift });
    }
    out
}

/// Tick steps, in seconds.
const STEPS_S: [i64; 18] = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200, 86400];

/// The smallest step (ms) that gives at most six labels across `span` ms of drawn time.
pub fn tick_step(span: f64) -> i64 {
    let seconds = (span / 1000.0).max(1.0);
    STEPS_S.iter().copied().find(|s| seconds / *s as f64 <= 6.0).unwrap_or(86400) * 1000
}

/// A time tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Where it is drawn.
    pub x: f64,
    /// The time it names.
    pub t: i64,
    /// The first tick of a session.
    pub first: bool,
    /// Labelled as time before now ("−5M"), not as a clock time.
    pub ago: bool,
}

/// Round clock ticks in each session, landing on whole steps.
pub fn clock_ticks(sessions: &[Session], step: i64) -> Vec<Tick> {
    let mut ticks = Vec::new();
    for s in sessions {
        let mut t = div_ceil(s.from, step) * step;
        while t <= s.to {
            ticks.push(Tick { x: (t - s.shift) as f64, t, first: t - step < s.from, ago: false });
            t += step;
        }
    }
    ticks
}

fn div_ceil(a: i64, b: i64) -> i64 {
    (a as f64 / b as f64).ceil() as i64
}

/// While the run goes, the newest reading is now, so the current session's ticks count back from
/// it: "−10M", "−5M", "NOW", at the same steps as the clock ticks.
pub fn ago_ticks(s: &Session, step: i64) -> Vec<Tick> {
    let mut ticks = Vec::new();
    let mut t = s.to;
    while t >= s.from {
        ticks.insert(0, Tick { x: (t - s.shift) as f64, t, first: false, ago: true });
        t -= step;
    }
    ticks
}

/// "NOW", "−30S", "−5M", "−2H" (a minus sign, U+2212).
pub fn ago_label(ms: i64) -> String {
    let s = crate::format::js_round(ms as f64 / 1000.0) as i64;
    if s == 0 {
        return "NOW".into();
    }
    if s < 60 {
        return format!("−{s}S");
    }
    let m = crate::format::js_round(s as f64 / 60.0) as i64;
    if m < 60 || m % 60 != 0 {
        return format!("−{m}M");
    }
    format!("−{}H", m / 60)
}

/// 1, 2, 2.5 or 5 times a power of ten, at least `raw`.
pub fn nice_step(raw: f64) -> f64 {
    if raw <= 0.0 {
        return 1.0;
    }
    let pow = 10f64.powf(raw.log10().floor());
    let f = raw / pow;
    let m = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 2.5 {
        2.5
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    m * pow
}

/// Four even intervals from zero, the top tick above the maximum.
pub fn value_ticks(max: f64, integer: bool) -> [f64; 5] {
    let mut step = nice_step(max * 1.15 / 4.0);
    if integer {
        step = step.ceil().max(1.0);
    }
    [0.0, step, 2.0 * step, 3.0 * step, 4.0 * step]
}

/// Roboto Mono at 12 px with 0.04em tracking: every character is this wide.
pub const MONO_CHAR_W: f64 = 7.7;

/// A value axis as wide as its longest label, plus the gap to the plot.
pub fn axis_width(labels: &[String]) -> f64 {
    let longest = labels.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    (longest as f64 * MONO_CHAR_W).ceil() + 10.0
}

/// The effect of the last seqs change on throughput: where it was, and throughput after it over
/// throughput before it, minus one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeqsEffect {
    pub x: f64,
    pub change: f64,
}

fn median(xs: &[f64]) -> f64 {
    let mut s = xs.to_vec();
    s.sort_by(f64::total_cmp);
    s[s.len() / 2]
}

/// The median of up to ten readings after the last seqs change (from the change on) over the
/// median of up to ten readings before it; none until there are three readings on each side, or
/// when the change is under 5%.
pub fn seqs_effect(data: &[Point]) -> Option<SeqsEffect> {
    let at = (1..data.len()).rev().find(|&i| matches!((data[i - 1].seqs, data[i].seqs), (Some(a), Some(b)) if a != b))?;
    let before_seqs = data[at - 1].seqs;
    let mut before = Vec::new();
    for p in data[..at].iter().rev() {
        if before.len() >= 10 {
            break;
        }
        match p.tps {
            Some(tps) if p.seqs == before_seqs => before.push(tps),
            _ => break,
        }
    }
    let mut after = Vec::new();
    for p in &data[at + 1..] {
        if after.len() >= 10 {
            break;
        }
        match p.tps {
            Some(tps) => after.push(tps),
            None => break,
        }
    }
    if before.len() < 3 || after.len() < 3 {
        return None;
    }
    let b = median(&before);
    if b <= 0.0 {
        return None;
    }
    let change = median(&after) / b - 1.0;
    (change.abs() >= 0.05).then_some(SeqsEffect { x: data[at].x, change })
}

/// The flag's words: "+102% TOK/S" or "−12% TOK/S".
pub fn effect_label(change: f64) -> String {
    let pct = crate::format::js_round(change.abs() * 100.0) as i64;
    format!("{}{pct}% TOK/S", if change > 0.0 { "+" } else { "−" })
}

/// Which time labels show: from the last one back, each kept only when it stays `min_gap` points
/// clear of the one kept after it (recharts' `preserveEnd` with `minTickGap`). `boxes` are each
/// label's (left, right) edges, in order along the axis.
pub fn visible_ticks(boxes: &[(f64, f64)], min_gap: f64) -> Vec<bool> {
    let mut keep = vec![false; boxes.len()];
    let mut limit = f64::INFINITY;
    for i in (0..boxes.len()).rev() {
        let (left, right) = boxes[i];
        if right + min_gap <= limit || limit.is_infinite() {
            keep[i] = true;
            limit = left;
        }
    }
    keep
}

/// Where a time label sits: centered on its tick, except near either end of the chart, where it
/// lines up with the tick's edge so it is never cut off. The label's left edge for a label `w` wide
/// at `x`, in a chart whose labels may reach `right`.
pub fn label_left(x: f64, w: f64, right: f64) -> f64 {
    let half = w / 2.0;
    if x + half > right {
        x - w
    } else if x - half < 0.0 {
        x
    } else {
        x - half
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(t: i64, tps: f64, seqs: f64) -> Reading {
        Reading { t, tps, seqs, active: seqs }
    }

    #[test]
    fn a_pause_is_cut_out_of_the_axis() {
        // Readings every 5 s, then a pause of an hour, then two more.
        let mut pts: Vec<Reading> = (0..10).map(|i| r(i * 5000, 100.0, 8.0)).collect();
        pts.push(r(45_000 + 3_600_000, 90.0, 8.0));
        pts.push(r(50_000 + 3_600_000, 95.0, 8.0));
        let l = layout(&pts);
        assert_eq!(l.sessions.len(), 2);
        assert_eq!(l.breaks.len(), 1);
        // The running time is 50 s: the band is 1 s (3% is 1.5 s): 1.5 s.
        let band = 1500.0;
        assert_eq!(l.breaks[0], 45_000.0 + band / 2.0);
        // The break point has no values; the next point sits a band after the last one.
        let brk = l.data.iter().position(|p| p.tps.is_none()).unwrap();
        assert_eq!(brk, 10);
        assert_eq!(l.data[11].x, 45_000.0 + band);
        assert_eq!(l.sessions[1].shift, 3_600_000 - 1500);
        assert_eq!(l.sessions[0], Session { from: 0, to: 45_000, shift: 0 });
    }

    #[test]
    fn steps_give_at_most_six_labels() {
        assert_eq!(tick_step(10_000.0), 2000);
        assert_eq!(tick_step(600_000.0), 120_000);
        assert_eq!(tick_step(500.0), 1000);
        assert_eq!(tick_step(30.0 * 86_400_000.0), 86_400_000);
    }

    #[test]
    fn clock_ticks_land_on_whole_steps_and_mark_a_sessions_first() {
        let s = [Session { from: 61_000, to: 300_000, shift: 0 }, Session { from: 4_000_000, to: 4_130_000, shift: 3_600_000 }];
        let ticks = clock_ticks(&s, 60_000);
        let ts: Vec<i64> = ticks.iter().map(|t| t.t).collect();
        assert_eq!(ts, vec![120_000, 180_000, 240_000, 300_000, 4_020_000, 4_080_000]);
        assert!(ticks[0].first && !ticks[1].first);
        assert!(ticks[4].first);
        assert_eq!(ticks[4].x, 420_000.0);
    }

    #[test]
    fn a_live_session_counts_back_from_now() {
        let s = Session { from: 0, to: 650_000, shift: 0 };
        let ticks = ago_ticks(&s, 300_000);
        let labels: Vec<String> = ticks.iter().map(|t| ago_label(s.to - t.t)).collect();
        assert_eq!(labels, vec!["−10M", "−5M", "NOW"]);
        assert_eq!(ago_label(30_000), "−30S");
        assert_eq!(ago_label(7_200_000), "−2H");
        assert_eq!(ago_label(5_400_000), "−90M");
    }

    #[test]
    fn value_ticks_are_nice_and_clear_the_top() {
        assert_eq!(value_ticks(171.0, false), [0.0, 50.0, 100.0, 150.0, 200.0]);
        assert_eq!(value_ticks(8.0, true), [0.0, 3.0, 6.0, 9.0, 12.0]);
        assert_eq!(value_ticks(0.0, true), [0.0, 1.0, 2.0, 3.0, 4.0]);
        assert_eq!(nice_step(0.3), 0.5);
        assert_eq!(nice_step(23.0), 25.0);
        assert_eq!(axis_width(&["0".into(), "1,200".into()]), 49.0);
    }

    #[test]
    fn the_flag_reads_the_last_seqs_change() {
        let pts: Vec<Reading> = (0..8).map(|i| r(i * 1000, 100.0, 8.0)).chain((8..12).map(|i| r(i * 1000, 202.0, 16.0))).collect();
        let l = layout(&pts);
        let e = seqs_effect(&l.data).unwrap();
        assert_eq!(e.x, 8000.0);
        assert_eq!(effect_label(e.change), "+102% TOK/S");
        // Two readings after the change are not enough.
        assert_eq!(seqs_effect(&l.data[..10]), None);
        // Under 5% is no effect.
        let flat: Vec<Reading> = (0..8).map(|i| r(i * 1000, 100.0, 8.0)).chain((8..12).map(|i| r(i * 1000, 103.0, 16.0))).collect();
        assert_eq!(seqs_effect(&layout(&flat).data), None);
        assert_eq!(effect_label(-0.124), "−12% TOK/S");
    }

    #[test]
    fn labels_stay_inside_and_apart() {
        assert_eq!(label_left(100.0, 40.0, 500.0), 80.0);
        assert_eq!(label_left(5.0, 40.0, 500.0), 5.0);
        assert_eq!(label_left(495.0, 40.0, 500.0), 455.0);
        // The last label is kept; one too close before it goes.
        assert_eq!(visible_ticks(&[(0.0, 40.0), (50.0, 90.0), (100.0, 140.0)], 28.0), vec![true, false, true]);
    }
}
