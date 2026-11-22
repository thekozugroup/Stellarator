import { useMemo } from "react"
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  usePlotArea,
  useXAxisScale,
  type TooltipContentProps,
} from "recharts"
import { int } from "@/lib/format"
import type { MetricPoint } from "@/lib/types"

interface Point {
  t: number
  /** Null only on the break inserted where the run was paused. */
  tps: number | null
  seqs: number | null
  active: number | null
}

/**
 * A pause leaves a hole in the readings. A null point there breaks both lines, instead of a slope
 * drawn across time when nothing ran. A hole is five readings long, and at least 30 s.
 */
function breakGaps(pts: Point[]): Point[] {
  const steps = pts
    .slice(1)
    .map((p, i) => p.t - pts[i].t)
    .sort((a, b) => a - b)
  const gap = Math.max(30_000, 5 * (steps[Math.floor(steps.length / 2)] ?? 0))
  const out: Point[] = []
  for (const p of pts) {
    const prev = out[out.length - 1]
    if (prev && p.t - prev.t > gap) out.push({ t: prev.t + 1, tps: null, seqs: null, active: null })
    out.push(p)
  }
  return out
}

const STEPS_S = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200, 86400]

/** Round time ticks: the smallest step that gives at most six labels, landing on whole steps. */
function timeTicks(min: number, max: number): { ticks: number[]; step: number } {
  const span = Math.max(1, (max - min) / 1000)
  const step = (STEPS_S.find((s) => span / s <= 6) ?? 86400) * 1000
  const ticks: number[] = []
  for (let t = Math.ceil(min / step) * step; t <= max; t += step) ticks.push(t)
  return { ticks, step }
}

/**
 * While the run is going the newest reading is now, so the time axis counts back from it:
 * "−10M", "−5M", "NOW", at the same step sizes as the clock ticks.
 */
function agoTicks(min: number, max: number): { ticks: number[]; step: number } {
  const span = Math.max(1, (max - min) / 1000)
  const step = (STEPS_S.find((s) => span / s <= 6) ?? 86400) * 1000
  const ticks: number[] = []
  for (let t = max; t >= min; t -= step) ticks.unshift(t)
  return { ticks, step }
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

/** Axis labels (DESIGN.md, Data visualization): label-mono in muted ink. The family and tracking come from the wrapper's class. */
const AXIS_TICK = { fill: "var(--muted-foreground)", fontSize: 12 }
const SEQS_AXIS_W = 36
/** Roboto Mono at 12 px with 0.04em tracking: every character is this wide. */
const MONO_CHAR_W = 7.7

function ChartTooltip({ active, payload }: TooltipContentProps) {
  const p = active ? (payload?.[0]?.payload as Point | undefined) : undefined
  if (!p || p.tps === null) return null
  return (
    <div className="rounded-sm material-thick material-float px-3 py-2 text-xs text-popover-foreground">
      <p className="font-mono text-muted-foreground">{hms.format(p.t)}</p>
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
 * Names each scale above its own axis, with a stroke of its line: output tokens/s on the left,
 * seqs on the right. Above the plot, a name never sits on a line or on the fill.
 */
function AxisNames() {
  const plot = usePlotArea()
  if (!plot) return null
  const y = plot.y - 14
  const right = plot.x + plot.width + SEQS_AXIS_W
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
  /** When seqs last changed. */
  t: number
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
  return Math.abs(change) >= 0.05 ? { t: data[at].t, change } : null
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
  const x = scale(effect.t)
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
  const data = useMemo<Point[]>(() => {
    const pts: Point[] = metrics.map((m) => ({ t: Date.parse(m.t), tps: m.tokens_per_s, seqs: m.concurrency, active: m.active }))
    if (live && pts.length && live.at > pts[pts.length - 1].t + 500) {
      pts.push({ t: live.at, tps: live.tps, seqs: live.seqs, active: live.active })
    }
    return breakGaps(pts)
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
  const ago = !!live
  const { ticks: xTicks, step } = ago ? agoTicks(tMin, tMax) : timeTicks(tMin, tMax)
  const fmt = step < 60_000 ? hms : hm
  const tickLabel = ago ? (t: number) => agoLabel(tMax - t) : (t: number) => fmt.format(t).toUpperCase()
  const tpsTicks = valueTicks(Math.max(...data.map((d) => d.tps ?? 0)), false)
  const seqsTicks = valueTicks(Math.max(...data.map((d) => d.seqs ?? 0)), true)
  const last = data[data.length - 1]
  const peak = Math.max(...data.map((d) => d.tps ?? 0))
  const effectText = effect
    ? ` Since seqs last changed, throughput is ${Math.round(Math.abs(effect.change) * 100)}% ${effect.change > 0 ? "higher" : "lower"}.`
    : ""

  return (
    <div
      className={frame}
      role="img"
      aria-label={`Output tokens per second from ${hm.format(tMin)} to ${hm.format(tMax)}: now ${int(last.tps)} at ${int(last.seqs)} seqs, peak ${int(peak)}.${effectText}`}
    >
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={data} margin={{ top: 26, right: 4, bottom: 0, left: 0 }}>
          <CartesianGrid yAxisId="tps" vertical={false} stroke="var(--chart-grid)" strokeDasharray="3 4" />
          <XAxis
            dataKey="t"
            type="number"
            scale="time"
            domain={[tMin, tMax]}
            ticks={xTicks}
            tickFormatter={tickLabel}
            tick={AXIS_TICK}
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
            width={44}
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
            width={SEQS_AXIS_W}
          />
          <Tooltip
            content={ChartTooltip}
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
          <AxisNames />
          {effect ? <EffectFlag effect={effect} /> : null}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  )
}
