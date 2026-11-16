import { useEffect, useState } from "react"
import { Link, useParams } from "react-router"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Page, Panel } from "@/components/shared/page"
import { RunStatusPill } from "@/components/shared/status-pill"
import { ExportDialog } from "@/components/runs/export-dialog"
import { InFlightList } from "@/components/runs/in-flight"
import { RunActions } from "@/components/runs/run-actions"
import { isActive, useRunLive } from "@/components/runs/run-data"
import { RunProgress } from "@/components/runs/run-progress"
import { SamplesSection } from "@/components/runs/samples-section"
import { SeqsControl } from "@/components/runs/seqs-control"
import { ThroughputChart } from "@/components/runs/throughput-chart"
import { Button } from "@/components/ui/button"
import { ApiError } from "@/lib/api"
import { dateTime, int, relative, time } from "@/lib/format"
import type { Run } from "@/lib/types"
import { cn } from "@/lib/utils"

const PROBLEM = /fail|error|could ?n[o']t|unreach|refused|timed? ?out|rate limit|429|5\d\d|circuit/i
/** The endpoint itself needs fixing: a rejected key, or a server that cannot be reached. */
const ENDPOINT_PROBLEM = /\b40[13]\b|api key|could ?n[o']t connect|unreach|refused|resolve/i

function StatusReason({ run }: { run: Run }) {
  if (!run.status_reason) return null
  const problem = run.status !== "running" && PROBLEM.test(run.status_reason)
  const fixEndpoint = problem && run.endpoint_id !== null && ENDPOINT_PROBLEM.test(run.status_reason)
  return (
    <div
      className={cn(
        "mb-6 flex flex-wrap items-center gap-x-3 gap-y-2 text-sm",
        problem ? "rounded-lg bg-warning-tint px-3 py-2 text-warning-ink" : "text-muted-foreground",
      )}
    >
      <p className="min-w-0 flex-1 basis-64">{run.status_reason}</p>
      {fixEndpoint ? (
        <Button asChild variant="outline" size="sm" className="bg-background pointer-coarse:h-11">
          <Link to={`/endpoints?edit=${run.endpoint_id}`}>Edit endpoint</Link>
        </Button>
      ) : null}
    </div>
  )
}

function DetailSkeleton() {
  return (
    <Page title={<Bar className="h-7 w-72 max-w-full" />} back={{ to: "/runs", label: "Runs" }} meta={<Bar className="w-80 max-w-full" />}>
      <div className="grid gap-3" aria-busy="true" aria-label="Loading run">
        <Bar className="h-8 w-56" />
        <Bar className="h-2 w-full" />
        <div className="mt-2 flex gap-8">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="grid gap-1">
              <Bar className="h-3 w-16" />
              <Bar className="h-5 w-12" />
            </div>
          ))}
        </div>
        <Bar className="mt-6 h-[220px] w-full rounded-xl" />
      </div>
    </Page>
  )
}

export function RunDetailPage() {
  const { runId } = useParams()
  const id = Number(runId)
  const live = useRunLive(id)
  const [exportOpen, setExportOpen] = useState(false)

  const run = live.data?.run
  useEffect(() => {
    if (!run) return
    document.title = `${isActive(run.status) ? `${int(run.done)} of ${int(run.total)} · ` : ""}${run.name} · Stellarator`
    return () => {
      document.title = "Stellarator"
    }
  }, [run])

  if (!Number.isInteger(id) || (live.error instanceof ApiError && live.error.status === 404)) {
    return (
      <Page title="Run not found" back={{ to: "/runs", label: "Runs" }}>
        <p className="text-sm text-muted-foreground">
          There is no run with this address. It may have been deleted.{" "}
          <Link to="/runs" className="text-foreground underline underline-offset-4">
            See all runs
          </Link>
        </p>
      </Page>
    )
  }
  if (live.isPending) return <DetailSkeleton />
  if (!live.data) {
    return (
      <Page title="Run" back={{ to: "/runs", label: "Runs" }}>
        <ErrorBlock message={`Couldn't load this run. ${live.error?.message ?? ""}`} onRetry={() => void live.refetch()} />
      </Page>
    )
  }

  const data = live.data
  const r = data.run
  const lostContact = live.isError
  const finished = r.status === "completed"
  const active = isActive(r.status)
  // A run that is not going and drew no throughput has nothing to chart; nothing can be in flight
  // unless it is going.
  const showChart = active || (data.metrics.length >= 2 && data.metrics.some((m) => m.tokens_per_s > 0))

  return (
    <Page
      title={
        <span className="line-clamp-2" title={r.name}>
          {r.name}
        </span>
      }
      back={{ to: "/runs", label: "Runs" }}
      meta={
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <RunStatusPill status={r.status} failed={r.failed} />
          <span className="min-w-0 [overflow-wrap:anywhere]">
            <span className="font-mono text-[13px] text-foreground">{r.model}</span> on {r.endpoint_name}
          </span>
          <span aria-hidden="true">·</span>
          <Link to={`/prompts/${r.prompt_set_id}`} className="underline-offset-4 hover:text-foreground hover:underline">
            {r.prompt_set_name}
          </Link>
          <span aria-hidden="true">·</span>
          <span title={dateTime(r.created_at)}>Created {relative(r.created_at)}</span>
        </span>
      }
      actions={<RunActions run={r} onExport={() => setExportOpen(true)} />}
    >
      {lostContact ? (
        <p role="status" className="mb-4 flex flex-wrap items-center gap-x-3 rounded-lg bg-warning-tint px-3 py-2 text-sm text-warning-ink">
          Lost contact with the Stellarator server. Showing figures from {time(new Date(live.dataUpdatedAt).toISOString())}.
          <Button variant="outline" size="sm" className="bg-background" onClick={() => void live.refetch()}>
            Retry
          </Button>
        </p>
      ) : null}
      <StatusReason run={r} />

      <div className={cn("grid grid-cols-1 gap-6", !finished && "lg:grid-cols-[minmax(0,1fr)_auto] lg:gap-10")}>
        <RunProgress live={data} />
        {!finished ? (
          <div className="border-t pt-5 lg:border-t-0 lg:border-l lg:pt-0 lg:pl-8">
            <SeqsControl run={r} inFlight={data.in_flight} />
          </div>
        ) : null}
      </div>

      {showChart ? (
        <div className={cn("mt-8 grid grid-cols-1 gap-6", active && "xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]")}>
          <Panel title="Throughput">
            <ThroughputChart
              metrics={data.metrics}
              live={active && !lostContact ? { tps: data.tokens_per_s, seqs: r.concurrency, active: data.in_flight, at: live.dataUpdatedAt } : null}
            />
          </Panel>
          {active ? (
            <section aria-labelledby="inflight-heading" className="flex min-h-0 flex-col rounded-xl border xl:[contain:size]">
              <h2 id="inflight-heading" className="border-b px-4 py-3 text-base font-semibold">
                In flight
              </h2>
              <div
                tabIndex={data.sequences.length ? 0 : undefined}
                role={data.sequences.length ? "region" : undefined}
                aria-label={data.sequences.length ? "Sequences in flight" : undefined}
                className="max-h-[28rem] min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pt-3 pb-3 xl:max-h-none"
              >
                <InFlightList sequences={data.sequences} inFlight={data.in_flight} samplesPerPrompt={r.params.samples_per_prompt} />
              </div>
            </section>
          ) : null}
        </div>
      ) : null}

      <div className={showChart ? "mt-6" : "mt-8"}>
        <SamplesSection run={r} />
      </div>

      <ExportDialog run={r} open={exportOpen} onOpenChange={setExportOpen} />
    </Page>
  )
}
