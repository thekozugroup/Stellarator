import { useMemo } from "react"
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  ZIndexLayer,
  usePlotArea,
  useXAxisScale,
  type TooltipContentProps,
} from "recharts"
import { int } from "@/lib/format"
import type { MetricPoint } from "@/lib/types"

interface Point {
  /** When the reading was taken. */
  t: number
  /** Where it is drawn: its time minus the paused time before it (see layout). */
  x: number
  /** Null only on the break inserted where the run was paused. */
  tps: number | null
  seqs: number | null
  active: number | null
}

/** A stretch of readings without a pause, and how far it is drawn left of its real time. */
interface Session {
  from: number
  to: number
  shift: number
}

/**
 * A pause leaves a hole in the readings (a hole is five readings long, and at least 30 s). The
 * paused time is cut out of the x axis down to a narrow band, marked with a dashed line, so a run
 * resumed days later still shows each session at a readable width; a null point in the band
 * breaks both lines. Within a session the axis is plain time.
 */
function layout(pts: Omit<Point, "x">[]): { data: Point[]; breaks: number[]; sessions: Session[] } {
  const steps = pts
    .slice(1)
    .map((p, i) => p.t - pts[i].t)
    .sort((a, b) => a - b)
  const gap = Math.max(30_000, 5 * (steps[Math.floor(steps.length / 2)] ?? 0))
  const running = steps.filter((d) => d <= gap).reduce((a, b) => a + b, 0)
  const band = Math.max(1000, running * 0.03)
  const data: Point[] = []
  const breaks: number[] = []
  const sessions: Session[] = []
  let shift = 0
  let from = pts[0]?.t ?? 0
  pts.forEach((p, i) => {
    const prev = pts[i - 1]
    if (prev && p.t - prev.t > gap) {
      sessions.push({ from, to: prev.t, shift })
      const x = prev.t - shift + band / 2
      data.push({ t: prev.t + 1, x, tps: null, seqs: null, active: null })
      breaks.push(x)
      shift += p.t - prev.t - band
      from = p.t
    }
    data.push({ ...p, x: p.t - shift })
  })
  if (pts.length) sessions.push({ from, to: pts[pts.length - 1].t, shift })
  return { data, breaks, sessions }
}

const STEPS_S = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200, 86400]

/** The smallest step that gives at most six labels across `span` ms of drawn time. */
const tickStep = (span: number) => (STEPS_S.find((s) => Math.max(1, span / 1000) / s <= 6) ?? 86400) * 1000

interface Tick {
  /** Where it is drawn. */
  x: number
  /** The time it names. */
  t: number
  /** The first tick of a session. */
  first?: boolean
  /** Labelled as time before now ("−5M"), not as a clock time. */
  ago?: boolean
}

/** Round clock ticks in each session, landing on whole steps. */
function clockTicks(sessions: Session[], step: number): Tick[] {
  const ticks: Tick[] = []
  for (const s of sessions) {
    for (let t = Math.ceil(s.from / step) * step; t <= s.to; t += step) ticks.push({ x: t - s.shift, t, first: t - step < s.from })
  }
  return ticks
}

/**
 * While the run is going, the newest reading is now, so the current session's ticks count back
 * from it: "−10M", "−5M", "NOW", at the same step sizes as the clock ticks.
 */
function agoTicks(s: Session, step: number): Tick[] {
  const ticks: Tick[] = []
  for (let t = s.to; t >= s.from; t -= step) ticks.unshift({ x: t - s.shift, t, ago: true })
  return ticks
}

function agoLabel(ms: number): string {
  const s = Math.round(ms / 1000)
  if (s === 0) return "NOW"
  if (s < 60) return `−${s}S`
  const m = Math.round(s / 60)
  if (m < 60 || m % 60 !== 0) return `−${m}M`
  return `−${m / 60}H`
}

/** 1, 2, 2.5 or 5 times a power of ten, at least `raw`. */
function niceStep(raw: number): number {
  if (raw <= 0) return 1
  const pow = 10 ** Math.floor(Math.log10(raw))
  const f = raw / pow
  const m = f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10
  return m * pow
}

/** Four even intervals from zero, the top tick above the maximum. */
function valueTicks(max: number, integer: boolean): number[] {
  let step = niceStep((max * 1.15) / 4)
  if (integer) step = Math.max(1, Math.ceil(step))
  return [0, 1, 2, 3, 4].map((i) => i * step)
}

const hm = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" })
const hms = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit", second: "2-digit" })
const md = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" })
const mdhm = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })
const mdhms = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit", second: "2-digit" })
const DAY_MS = 86_400_000

/**
 * A clock tick's label: seconds for short spans; once the readings cross midnight (a run resumed
 * on a later day) the first tick of each session names the date instead of the time.
 */
function clockLabel(tick: Tick, step: number, multiDay: boolean): string {
  // The date replaces the time on a session's first tick: a date and a time together are twice as
  // wide as the other labels and run into the next one.
  const f = step >= DAY_MS || (multiDay && tick.first) ? md : step < 60_000 ? hms : hm
  return f.format(tick.t).toUpperCase()
}

/** Axis labels (DESIGN.md, Data visualization): label-mono in muted ink. The family and tracking come from the wrapper's class. */
const AXIS_TICK = { fill: "var(--muted-foreground)", fontSize: 12 }
/** Roboto Mono at 12 px with 0.04em tracking: every character is this wide. */
const MONO_CHAR_W = 7.7
/** A value axis as wide as its longest label, plus the gap to the plot. */
const axisWidth = (ticks: number[]) => Math.ceil(Math.max(...ticks.map((v) => int(v).length)) * MONO_CHAR_W) + 10

function ChartTooltip({ active, payload, multiDay }: TooltipContentProps & { multiDay: boolean }) {
  const p = active ? (payload?.[0]?.payload as Point | undefined) : undefined
  if (!p || p.tps === null) return null
  return (
    <div className="rounded-sm material-thick material-float px-3 py-2 text-xs text-popover-foreground">
      <p className="font-mono text-muted-foreground">{(multiDay ? mdhms : hms).format(p.t)}</p>
      <dl className="mt-1 grid grid-cols-[auto_auto] gap-x-4 gap-y-0.5">
        <dt className="flex items-center gap-1.5 text-muted-foreground">
          <span aria-hidden="true" className="h-0.5 w-2.5 rounded-full bg-periwinkle" />
          Output tokens/s
        </dt>
        <dd className="text-right font-mono font-medium">{int(p.tps)}</dd>
        <dt className="flex items-center gap-1.5 text-muted-foreground">
          <span aria-hidden="true" className="h-0.5 w-2.5 rounded-full bg-chart-2" />
          Seqs
        </dt>
        <dd className="text-right font-mono">{int(p.seqs)}</dd>
        <dt className="pl-4 text-muted-foreground">In flight</dt>
        <dd className="text-right font-mono">{int(p.active)}</dd>
      </dl>
    </div>
  )
}

/**
 * A time label under the plot, centered on its tick, except near either end of the chart, where
 * it lines up with the tick's edge so it is never cut off.
 */
function TimeTick({
  x = 0,
  y = 0,
  payload,
  label,
  rightRoom,
}: {
  x?: number
  y?: number
  payload?: { value: number }
  label: (x: number) => string
  rightRoom: number
}) {
  const plot = usePlotArea()
  const text = payload ? label(payload.value) : ""
  const half = (text.length * MONO_CHAR_W) / 2
  const right = plot ? plot.x + plot.width + rightRoom : Infinity
  const anchor = x + half > right ? "end" : x - half < 0 ? "start" : "middle"
  return (
    <text x={x} y={y} dy="0.71em" textAnchor={anchor} fontSize={12} fill="var(--muted-foreground)">
      {text}
    </text>
  )
}

/**
 * Names each scale above its own axis, with a stroke of its line: output tokens/s on the left,
 * seqs on the right. Above the plot, a name never sits on a line or on the fill.
 */
function AxisNames({ seqsAxisWidth }: { seqsAxisWidth: number }) {
  const plot = usePlotArea()
  if (!plot) return null
  const y = plot.y - 14
  const right = plot.x + plot.width + seqsAxisWidth
  return (
    <g aria-hidden="true" fontSize={12} fill="var(--muted-foreground)">
      <line x1={0} x2={10} y1={y - 4} y2={y - 4} stroke="var(--periwinkle)" strokeWidth={1.5} />
      <text x={16} y={y}>
        TOK/S
      </text>
      <line x1={right - 48} x2={right - 38} y1={y - 4} y2={y - 4} stroke="var(--chart-2)" strokeWidth={1.5} />
      <text x={right} y={y} textAnchor="end">
        SEQS
      </text>
    </g>
  )
}

interface SeqsEffect {
  /** Where seqs last changed, on the x axis. */
  x: number
  /** Throughput after the change over throughput before it, minus one. */
  change: number
}

const median = (xs: number[]) => {
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.floor(s.length / 2)]
}

/**
 * The effect of the last seqs change on throughput: the median of up to ten readings after it
 * (skipping the first, while new sequences start) over the median of up to ten readings before it.
 * Null until there are three readings on each side, or when the change is under 5%.
 */
function seqsEffect(data: Point[]): SeqsEffect | null {
  let at = -1
  for (let i = data.length - 1; i > 0; i--) {
    const a = data[i - 1].seqs
    const b = data[i].seqs
    if (a !== null && b !== null && a !== b) {
      at = i
      break
    }
  }
  if (at < 0) return null
  const before: number[] = []
  for (let i = at - 1; i >= 0 && before.length < 10; i--) {
    const p = data[i]
    if (p.tps === null || p.seqs !== data[at - 1].seqs) break
    before.push(p.tps)
  }
  const after: number[] = []
  for (let i = at + 1; i < data.length && after.length < 10; i++) {
    const p = data[i]
    if (p.tps === null) break
    after.push(p.tps)
  }
  if (before.length < 3 || after.length < 3) return null
  const b = median(before)
  if (b <= 0) return null
  const change = median(after) / b - 1
  return Math.abs(change) >= 0.05 ? { x: data[at].x, change } : null
}

/**
 * A flag on the chart (DESIGN.md, Data visualization): a capsule in the signal's tint with mono
 * text, at the top of the plot where seqs last changed. More throughput is the data color; less is
 * coral.
 */
function EffectFlag({ effect }: { effect: SeqsEffect }) {
  const plot = usePlotArea()
  const scale = useXAxisScale()
  if (!plot || !scale) return null
  const x = scale(effect.x)
  if (typeof x !== "number" || !Number.isFinite(x)) return null
  const pct = Math.round(Math.abs(effect.change) * 100)
  const text = `${effect.change > 0 ? "+" : "−"}${pct}% TOK/S`
  const w = text.length * MONO_CHAR_W + 16
  const h = 20
  // Beside the change, inside the plot; on the left of it when there is no room on the right.
  const left = x + 6 + w <= plot.x + plot.width ? x + 6 : Math.max(plot.x, x - 6 - w)
  const up = effect.change > 0
  return (
    <g aria-hidden="true">
      <rect
        x={left}
        y={plot.y + 4}
        width={w}
        height={h}
        rx={h / 2}
        fill={up ? "var(--periwinkle-tint)" : "var(--coral-tint)"}
      />
      <text
        x={left + w / 2}
        y={plot.y + 4 + h / 2}
        dominantBaseline="central"
        textAnchor="middle"
        fontSize={12}
        fill={up ? "var(--periwinkle-ink)" : "var(--coral-ink)"}
      >
        {text}
      </text>
    </g>
  )
}

/**
 * The run's signature: output tokens per second as a periwinkle line over a flat tint, with the seqs
 * setting as a gray step line on its own scale, so the effect of changing seqs shows (and, once
 * there are readings on both sides of the last change, a flag says by how much). Mono axis labels,
 * dashed hairline grid. Updates in place; nothing animates.
 */
export function ThroughputChart({
  metrics,
  live,
}: {
  metrics: MetricPoint[]
  /** The current rate and setting and when they were read, appended as the newest point while the run is going. */
  live?: { tps: number; seqs: number; active: number; at: number } | null
}) {
  const { data, breaks, sessions } = useMemo(() => {
    const pts = metrics.map((m) => ({ t: Date.parse(m.t), tps: m.tokens_per_s, seqs: m.concurrency, active: m.active }))
    if (live && pts.length && live.at > pts[pts.length - 1].t + 500) {
      pts.push({ t: live.at, tps: live.tps, seqs: live.seqs, active: live.active })
    }
    return layout(pts)
  }, [metrics, live])
  const effect = useMemo(() => seqsEffect(data), [data])

  const hasData = data.length >= 2 && data.some((d) => (d.tps ?? 0) > 0)
  const frame = "h-[180px] sm:h-[240px] [&_.recharts-text]:font-mono [&_.recharts-text]:tracking-[0.04em] [&_text]:font-mono [&_text]:tracking-[0.04em]"

  if (!hasData) {
    return (
      <div className={`relative ${frame}`} role="img" aria-label="Throughput chart, no data yet">
        <div className="absolute inset-x-0 top-6 bottom-6 ml-11 flex flex-col justify-between">
          {[0, 1, 2, 3, 4].map((i) => (
            <div key={i} className="border-t border-dashed border-chart-grid" />
          ))}
        </div>
        <span className="label-mono absolute bottom-6 left-0 w-8 translate-y-1/2 text-right text-muted-foreground">0</span>
        <p className="absolute inset-x-0 top-[45%] ml-11 -translate-y-1/2 px-4 text-sm text-muted-foreground">
          <span className="bg-muted pr-2">Throughput appears here once the first tokens arrive.</span>
        </p>
      </div>
    )
  }

  const tMin = data[0].t
  const tMax = data[data.length - 1].t
  const xMin = data[0].x
  const xMax = data[data.length - 1].x
  const step = tickStep(xMax - xMin)
  const multiDay = new Date(tMin).toDateString() !== new Date(tMax).toDateString()
  const span = multiDay ? mdhm : hm
  // Live: the current session counts back from now; earlier sessions keep clock times (with the
  // date once they are on another day).
  const xTicks = live
    ? [...clockTicks(sessions.slice(0, -1), step), ...agoTicks(sessions[sessions.length - 1], step)]
    : clockTicks(sessions, step)
  const labelAt = new Map(xTicks.map((k) => [k.x, k.ago ? agoLabel(tMax - k.t) : clockLabel(k, step, multiDay)]))
  const tpsTicks = valueTicks(Math.max(...data.map((d) => d.tps ?? 0)), false)
  const seqsTicks = valueTicks(Math.max(...data.map((d) => d.seqs ?? 0)), true)
  const seqsAxisWidth = Math.max(36, axisWidth(seqsTicks))
  const last = data[data.length - 1]
  const peak = Math.max(...data.map((d) => d.tps ?? 0))
  const pausedText = breaks.length ? ` Paused ${breaks.length === 1 ? "once" : `${breaks.length} times`}; the paused time is left out.` : ""
  const effectText = effect
    ? ` Since seqs last changed, throughput is ${Math.round(Math.abs(effect.change) * 100)}% ${effect.change > 0 ? "higher" : "lower"}.`
    : ""

  return (
    <div
      className={frame}
      role="img"
      aria-label={`Output tokens per second from ${span.format(tMin)} to ${span.format(tMax)}: now ${int(last.tps)} at ${int(last.seqs)} seqs, peak ${int(peak)}.${pausedText}${effectText}`}
    >
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={data} margin={{ top: 26, right: 4, bottom: 0, left: 0 }}>
          <CartesianGrid yAxisId="tps" vertical={false} stroke="var(--chart-grid)" strokeDasharray="3 4" />
          <XAxis
            dataKey="x"
            type="number"
            domain={[xMin, xMax]}
            ticks={xTicks.map((k) => k.x)}
            tickFormatter={(x: number) => labelAt.get(x) ?? ""}
            tick={<TimeTick label={(x) => labelAt.get(x) ?? ""} rightRoom={seqsAxisWidth} />}
            tickLine={false}
            axisLine={false}
            tickMargin={8}
            minTickGap={28}
          />
          <YAxis
            yAxisId="tps"
            domain={[0, tpsTicks[tpsTicks.length - 1]]}
            ticks={tpsTicks}
            tickFormatter={(v: number) => int(v)}
            tick={AXIS_TICK}
            tickLine={false}
            axisLine={false}
            width={Math.max(44, axisWidth(tpsTicks))}
          />
          <YAxis
            yAxisId="seqs"
            orientation="right"
            domain={[0, seqsTicks[seqsTicks.length - 1]]}
            ticks={seqsTicks}
            tickFormatter={(v: number) => int(v)}
            tick={AXIS_TICK}
            tickLine={false}
            axisLine={false}
            width={seqsAxisWidth}
          />
          <Tooltip
            content={(props) => <ChartTooltip {...props} multiDay={multiDay} />}
            cursor={{ stroke: "var(--input)", strokeWidth: 1, strokeDasharray: "3 3" }}
            isAnimationActive={false}
          />
          <Area
            yAxisId="tps"
            dataKey="tps"
            type="linear"
            stroke="var(--periwinkle)"
            strokeWidth={1.5}
            fill="var(--periwinkle)"
            fillOpacity={0.14}
            dot={false}
            activeDot={{ r: 3, fill: "var(--periwinkle)", stroke: "var(--muted)", strokeWidth: 2 }}
            isAnimationActive={false}
          />
          <Line
            yAxisId="seqs"
            dataKey="seqs"
            type="stepAfter"
            stroke="var(--chart-2)"
            strokeWidth={1.5}
            dot={false}
            activeDot={false}
            isAnimationActive={false}
          />
          {breaks.map((x) => (
            // Under the lines and the flag, over the grid.
            <ReferenceLine key={x} yAxisId="tps" x={x} stroke="var(--input)" strokeDasharray="3 3" ifOverflow="hidden" zIndex={-50} />
          ))}
          <AxisNames seqsAxisWidth={seqsAxisWidth} />
          {/* Over the lines (400), under the hover cursor (1100). */}
          {effect ? (
            <ZIndexLayer zIndex={1050}>
              <EffectFlag effect={effect} />
            </ZIndexLayer>
          ) : null}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  )
}
