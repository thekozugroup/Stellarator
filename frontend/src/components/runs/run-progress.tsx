import { Fragment } from "react"
import { ProgressBar } from "@/components/shared/progress-bar"
import { Tile, Tiles } from "@/components/shared/tiles"
import { DASH, dec1, int } from "@/lib/format"
import type { RunLive } from "@/lib/types"
import { isActive } from "./run-data"

/**
 * A duration as a figure: the numbers at figure size, the units small and muted ("2 h 14 min",
 * "31 min 18 s"), so the value fits a half-width tile on a phone. A gray dash when unknown.
 */
function DurationFigure({ seconds }: { seconds: number | null }) {
  if (seconds === null) return <span className="text-muted-foreground">{DASH}</span>
  // Under a second reads as "<1 s", not as a zero.
  if (seconds < 1) {
    return (
      <>
        {"<1"}
        <span className="ml-0.5 text-base font-medium tracking-normal text-muted-foreground">s</span>
      </>
    )
  }
  const s = Math.round(seconds)
  const m = Math.floor(s / 60)
  const h = Math.floor(m / 60)
  const parts: [number, string][] =
    s < 60 ? [[s, "s"]] : m < 60 ? [[m, "min"], [s % 60, "s"]] : h < 48 ? [[h, "h"], [m % 60, "min"]] : [[Math.floor(h / 24), "d"], [h % 24, "h"]]
  return (
    <>
      {parts.map(([n, unit], i) => (
        <Fragment key={unit}>
          {i > 0 ? " " : null}
          {int(n)}
          <span className="ml-0.5 text-base font-medium tracking-normal text-muted-foreground">{unit}</span>
        </Fragment>
      ))}
    </>
  )
}

/**
 * The screen's facts as tiles (DESIGN.md, Layout): samples done of the total, at the true ratio,
 * in the wide tile with the bar (the failed count names its red part); then the rate in output
 * tokens and in samples, and the time left. A run that is not going shows averages over the time
 * it spent running, and how long that was. Per-status counts live on the sample tabs.
 */
export function RunProgress({ live }: { live: RunLive }) {
  const { run } = live
  const active = isActive(run.status)
  const finished = run.status === "completed"
  // Averages over the time the run spent running; under a second of it, or with nothing done,
  // there is no rate to show yet.
  const measured = run.active_seconds >= 1 && run.done > 0
  const avgTps = measured ? run.completion_tokens / run.active_seconds : null
  const avgSpm = measured ? (run.done / run.active_seconds) * 60 : null
  const dash = <span className="text-muted-foreground">{DASH}</span>

  return (
    <Tiles className="sm:grid-cols-3 lg:grid-cols-5">
      <Tile
        span
        className="sm:col-span-3 lg:col-span-2"
        value={int(run.done)}
        unit={`of ${int(run.total)}`}
        label={
          <span className="flex justify-between gap-3">
            <span className="truncate">Samples done</span>
            {run.failed > 0 ? <span className="text-negative-ink tabular">{int(run.failed)} failed</span> : null}
          </span>
        }
      >
        <ProgressBar done={run.done} failed={run.failed} total={run.total} />
      </Tile>
      {active ? (
        <>
          <Tile label="Output tokens/s" value={int(live.tokens_per_s)} />
          <Tile label="Samples/min" value={dec1(live.samples_per_min)} />
          <Tile className="max-sm:col-span-2" label="Time left" value={<DurationFigure seconds={live.eta_seconds} />} />
        </>
      ) : (
        <>
          <Tile label="Avg output tokens/s" value={avgTps !== null ? int(avgTps) : dash} />
          <Tile label="Avg samples/min" value={avgSpm !== null ? dec1(avgSpm) : dash} />
          <Tile
            className="max-sm:col-span-2"
            label={finished ? "Took" : "Run time"}
            value={<DurationFigure seconds={run.active_seconds > 0 ? run.active_seconds : null} />}
          />
        </>
      )}
    </Tiles>
  )
}
