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
  useYAxisScale,
  type TooltipContentProps,
} from "recharts"
import { int } from "@/lib/format"
import type { MetricPoint } from "@/lib/types"

interface Point {
  t: number
  tps: number
  seqs: number
  active: number
}

const STEPS_S = [5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200, 86400]

/** Round time ticks: the smallest step that gives at most six labels, landing on whole steps. */
function timeTicks(min: number, max: number): { ticks: number[]; step: number } {
  const span = Math.max(1, (max - min) / 1000)
  const step = (STEPS_S.find((s) => span / s <= 6) ?? 86400) * 1000
  const ticks: number[] = []
  for (let t = Math.ceil(min / step) * step; t <= max; t += step) ticks.push(t)
  return { ticks, step }
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

const AXIS_TICK = { fill: "var(--muted-foreground)", fontSize: 11 }

function ChartTooltip({ active, payload }: TooltipContentProps<number, string>) {
  const p = active ? (payload?.[0]?.payload as Point | undefined) : undefined
  if (!p) return null
  return (
    <div className="rounded-md border bg-popover px-3 py-2 text-xs text-popover-foreground shadow-md">
      <p className="text-muted-foreground tabular">{hms.format(p.t)}</p>
      <dl className="mt-1 grid grid-cols-[auto_auto] gap-x-4 gap-y-0.5 tabular">
        <dt className="text-muted-foreground">Output tokens/s</dt>
        <dd className="text-right font-medium">{int(p.tps)}</dd>
        <dt className="text-muted-foreground">Seqs</dt>
        <dd className="text-right">{int(p.seqs)}</dd>
        <dt className="text-muted-foreground">In flight</dt>
        <dd className="text-right">{int(p.active)}</dd>
      </dl>
    </div>
  )
}

/**
 * Names each line at its right end instead of a legend. The upper line's name goes above it and
 * the lower line's below it, so the two never collide.
 */
function EndLabels({ last }: { last: Point }) {
  const x = useXAxisScale()
  const yT = useYAxisScale("tps")
  const yS = useYAxisScale("seqs")
  const plot = usePlotArea()
  if (!x || !yT || !yS || !plot) return null
  const px = Number(x(last.t))
  const tY = Number(yT(last.tps))
  const sY = Number(yS(last.seqs))
  if (![px, tY, sY].every(Number.isFinite)) return null
  const bottom = plot.y + plot.height
  const tpsUpper = tY <= sY
  const place = (y: number, upper: boolean) => {
    if (upper) return Math.max(plot.y + 10, y - 7)
    return y + 15 > bottom - 2 ? y - 7 : y + 15
  }
  let tLabel = place(tY, tpsUpper)
  let sLabel = place(sY, !tpsUpper)
  if (Math.abs(tLabel - sLabel) < 13) {
    if (tpsUpper) tLabel = Math.max(plot.y + 10, sLabel - 14)
    else sLabel = Math.max(plot.y + 10, tLabel - 14)
  }
  return (
    <g aria-hidden="true" fontSize={11}>
      <text x={px - 4} y={tLabel} textAnchor="end" fill="var(--foreground)" fontWeight={500}>
        tok/s
      </text>
      <text x={px - 4} y={sLabel} textAnchor="end" fill="var(--muted-foreground)">
        seqs
      </text>
    </g>
  )
}

/**
 * The run's signature: output tokens per second as a copper line over a flat tint, with the seqs
 * setting as a gray step line on its own scale, so the effect of changing seqs shows. Updates in
 * place; nothing animates.
 */
export function ThroughputChart({
  metrics,
  live,
}: {
  metrics: MetricPoint[]
  /** The current rate and setting, appended as "now" while the run is going. */
  live?: { tps: number; seqs: number; active: number } | null
}) {
  const data = useMemo<Point[]>(() => {
    const pts = metrics.map((m) => ({ t: Date.parse(m.t), tps: m.tokens_per_s, seqs: m.concurrency, active: m.active }))
    if (live && pts.length) {
      const now = Date.now()
      if (now > pts[pts.length - 1].t + 500) pts.push({ t: now, tps: live.tps, seqs: live.seqs, active: live.active })
    }
    return pts
  }, [metrics, live])

  const hasData = data.length >= 2 && data.some((d) => d.tps > 0)

  if (!hasData) {
    return (
      <div className="relative h-[160px] sm:h-[220px]" role="img" aria-label="Throughput chart, no data yet">
        <div className="absolute inset-x-0 top-0 bottom-6 ml-8 flex flex-col justify-between">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="border-t border-[var(--chart-grid)]" />
          ))}
          <div className="border-t border-input/60" />
        </div>
        <span className="absolute bottom-6 left-0 w-6 translate-y-1/2 text-right text-[11px] text-muted-foreground">0</span>
        <p className="absolute inset-x-0 top-1/2 ml-8 -translate-y-1/2 px-4 text-sm text-muted-foreground">
          Throughput appears here once the first tokens arrive.
        </p>
      </div>
    )
  }

  const tMin = data[0].t
  const tMax = data[data.length - 1].t
  const { ticks: xTicks, step } = timeTicks(tMin, tMax)
  const fmt = step < 60_000 ? hms : hm
  const tpsTicks = valueTicks(Math.max(...data.map((d) => d.tps)), false)
  const seqsTicks = valueTicks(Math.max(...data.map((d) => d.seqs)), true)
  const last = data[data.length - 1]
  const peak = Math.max(...data.map((d) => d.tps))

  return (
    <div
      className="h-[160px] sm:h-[220px]"
      role="img"
      aria-label={`Output tokens per second from ${hm.format(tMin)} to ${hm.format(tMax)}: now ${int(last.tps)} at ${int(last.seqs)} seqs, peak ${int(peak)}.`}
    >
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={data} margin={{ top: 12, right: 4, bottom: 0, left: 0 }}>
          <CartesianGrid yAxisId="tps" vertical={false} stroke="var(--chart-grid)" />
          <XAxis
            dataKey="t"
            type="number"
            scale="time"
            domain={[tMin, tMax]}
            ticks={xTicks}
            tickFormatter={(t: number) => fmt.format(t)}
            tick={AXIS_TICK}
            tickLine={false}
            axisLine={{ stroke: "var(--input)", strokeOpacity: 0.6 }}
            tickMargin={6}
            minTickGap={24}
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
            width={32}
          />
          <Tooltip
            content={ChartTooltip}
            cursor={{ stroke: "var(--input)", strokeWidth: 1 }}
            isAnimationActive={false}
          />
          <Area
            yAxisId="tps"
            dataKey="tps"
            type="linear"
            stroke="var(--copper)"
            strokeWidth={2}
            fill="var(--copper)"
            fillOpacity={0.14}
            dot={false}
            activeDot={{ r: 3, fill: "var(--copper)", stroke: "var(--background)", strokeWidth: 2 }}
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
          <EndLabels last={last} />
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  )
}
