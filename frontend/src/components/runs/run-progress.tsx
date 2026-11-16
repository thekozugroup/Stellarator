import type { ReactNode } from "react"
import { ProgressBar } from "@/components/shared/progress-bar"
import { DASH, dec1, duration, int } from "@/lib/format"
import type { RunLive } from "@/lib/types"
import { cn } from "@/lib/utils"
import { isActive } from "./run-data"

function Stat({ label, children, className }: { label: string; children: ReactNode; className?: string }) {
  return (
    <div className="min-w-0 xl:border-l xl:pl-5 xl:first:border-l-0 xl:first:pl-0">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className={cn("mt-0.5 text-base font-medium tabular", className)}>{children}</dd>
    </div>
  )
}

/**
 * The screen's answer: how many samples are done of how many, at the true ratio, then the rate,
 * the time left and what went wrong, as one row of label-over-value figures.
 */
export function RunProgress({ live }: { live: RunLive }) {
  const { run } = live
  const active = isActive(run.status)
  // Averages over the time the run spent running; under a second of it says nothing yet.
  const measured = run.active_seconds >= 1
  const avgTps = measured ? run.completion_tokens / run.active_seconds : null
  const avgSpm = measured ? (run.done / run.active_seconds) * 60 : null

  return (
    <div className="grid min-w-0 gap-3">
      <p className="flex flex-wrap items-baseline gap-x-2">
        <span className="text-[30px] leading-[1.1] font-medium tracking-[-0.01em] tabular">{int(run.done)}</span>
        <span className="text-base text-muted-foreground tabular">of {int(run.total)} samples</span>
        {run.total > 0 ? (
          <span className="ml-auto text-sm text-muted-foreground tabular">{Math.floor((run.done / run.total) * 100)}% done</span>
        ) : null}
      </p>
      <ProgressBar done={run.done} failed={run.failed} total={run.total} className="h-2" />
      <dl className="mt-2 grid grid-cols-2 gap-x-6 gap-y-3 sm:grid-cols-3 xl:flex xl:flex-wrap xl:gap-x-5">
        {active ? (
          <>
            <Stat label="Output tokens/s">{int(live.tokens_per_s)}</Stat>
            <Stat label="Samples/min">{dec1(live.samples_per_min)}</Stat>
            <Stat label="ETA">{live.eta_seconds !== null ? duration(live.eta_seconds * 1000) : DASH}</Stat>
          </>
        ) : (
          <>
            <Stat label="Avg output tokens/s">{avgTps !== null ? int(avgTps) : DASH}</Stat>
            <Stat label="Avg samples/min">{avgSpm !== null ? dec1(avgSpm) : DASH}</Stat>
            <Stat label={run.status === "completed" ? "Took" : "Run time"}>
              {run.active_seconds > 0 ? duration(run.active_seconds * 1000) : DASH}
            </Stat>
          </>
        )}
        <Stat label="Failed" className={run.failed > 0 ? "text-destructive" : undefined}>
          {int(run.failed)}
        </Stat>
        <Stat label="Truncated" className={run.truncated > 0 ? "text-warning-ink" : undefined}>
          {int(run.truncated)}
        </Stat>
        <Stat label="Tokens generated">{int(run.completion_tokens)}</Stat>
      </dl>
    </div>
  )
}
