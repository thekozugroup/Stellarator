import { useEffect, useState, type ReactNode } from "react"
import { Link, useParams } from "react-router"
import { useQuery } from "@tanstack/react-query"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Page, Panel } from "@/components/shared/page"
import { RunStatusPill } from "@/components/shared/status-pill"
import { ExportDialog } from "@/components/runs/export-dialog"
import { InFlightList } from "@/components/runs/in-flight"
import { RunActions } from "@/components/runs/run-actions"
import { editEndpointHref, endpointTrouble, isActive, runKeys, useRunLive } from "@/components/runs/run-data"
import { RunProgress } from "@/components/runs/run-progress"
import { SamplesSection } from "@/components/runs/samples-section"
import { SeqsControl } from "@/components/runs/seqs-control"
import { ThroughputChart } from "@/components/runs/throughput-chart"
import { Button } from "@/components/ui/button"
import { ApiError, api } from "@/lib/api"
import { dateTime, int, relative, time } from "@/lib/format"
import type { Endpoint, Page as PageOf, Run, SampleSummary } from "@/lib/types"
import { cn } from "@/lib/utils"

const PROBLEM = /fail|error|could ?n[o']t|unreach|refused|timed? ?out|rate limit|429|5\d\d|circuit/i
/** The endpoint itself needs fixing: a rejected key, or a server that cannot be reached. */
const ENDPOINT_PROBLEM = /\b40[13]\b|api key|could ?n[o']t connect|unreach|refused|resolve/i

/**
 * For a finished run with failures: the endpoint's error when the endpoint caused them (its last
 * check failed, or the failed samples were refused or could not connect, and no check has passed
 * since). `pending` while that is still being found out.
 */
function useEndpointProblem(run: Run | undefined): { pending: boolean; error: string | null } {
  const check = !!run && run.status === "completed" && run.failed > 0 && run.endpoint_id !== null
  const endpoint = useQuery({ // taste-ignore: missing-states -- feeds RunNotice only; with no answer yet the notice waits, and the run reads as usual
    queryKey: ["endpoints", run?.endpoint_id],
    queryFn: () => api.get<Endpoint>(`/api/endpoints/${run?.endpoint_id}`),
    enabled: check,
  })
  const failed = useQuery({
    queryKey: [...runKeys.samples(run?.id ?? 0), "first-failed"],
    queryFn: () => api.get<PageOf<SampleSummary>>(`/api/runs/${run?.id}/samples?status=failed&limit=1`),
    enabled: check,
  })
  if (!run || !check) return { pending: false, error: null }
  const ep = endpoint.data
  const trouble = endpointTrouble(ep)
  if (ep && trouble) return { pending: false, error: ep.last_error || `${trouble} at last check.` }
  if (endpoint.isPending || failed.isPending) return { pending: true, error: null }
  const error = failed.data?.items[0]?.error
  if (!error || !ENDPOINT_PROBLEM.test(error)) return { pending: false, error: null }
  const fixedSince = !!ep?.last_ok && !!ep.last_checked_at && Date.parse(ep.last_checked_at) > Date.parse(run.finished_at ?? run.created_at)
  return { pending: false, error: fixedSince ? null : error }
}

/**
 * Why the run stopped, and the way out when the endpoint is the cause. Edit endpoint is the
 * screen's one primary action when it is what a finished run needs next; on a paused run, Resume
 * run keeps that place and Edit endpoint sits beside the reason.
 */
function RunNotice({ run, endpointError }: { run: Run; endpointError: string | null }) {
  const primary = !!endpointError
  const text = endpointError ?? run.status_reason
  if (!text) return null
  const problem = primary || (run.status !== "running" && PROBLEM.test(text))
  const fixEndpoint = run.endpoint_id !== null && (primary || (problem && ENDPOINT_PROBLEM.test(text)))
  return (
    <div
      className={cn(
        "mb-6 flex flex-wrap items-center gap-x-3 gap-y-2 text-sm",
        problem ? "rounded-lg bg-warning-tint px-3 py-2 text-warning-ink" : "text-muted-foreground",
      )}
    >
      <p className="min-w-0 flex-1 basis-64">{text}</p>
      {fixEndpoint && run.endpoint_id !== null ? (
        <Button asChild variant={primary ? "default" : "outline"} size="sm" className={primary ? undefined : "bg-background"}>
          <Link to={editEndpointHref(run.endpoint_id, `/runs/${run.id}`, /could not connect|unreachable|timed out|took too long/i.test(text) ? "url" : "key")}>
            Edit endpoint
          </Link>
        </Button>
      ) : null}
    </div>
  )
}

/**
 * Facts in a row with a dot between them. Each dot leads its fact, and the one that would start a
 * wrapped line is clipped, so no line starts or ends with a dot.
 */
function Facts({ items }: { items: ReactNode[] }) {
  return (
    <span className="-m-1 block overflow-hidden p-1">
      <span className="-ml-5 flex flex-wrap items-center gap-y-1">
        {items.map((item, i) => (
          <span key={i} className="inline-flex min-w-0 items-center">
            <span aria-hidden="true" className="w-5 flex-none text-center">
              ·
            </span>
            {item}
          </span>
        ))}
      </span>
    </span>
  )
}

/** Status, then the model, endpoint and prompt set minus what the run's name already says, then when. */
function RunMeta({ run }: { run: Run }) {
  const name = run.name.toLowerCase()
  const showModel = !name.includes(run.model.toLowerCase())
  const showSet = !name.includes(run.prompt_set_name.toLowerCase())
  const link = "inline-flex items-center underline decoration-muted-foreground/40 underline-offset-4 hover:text-foreground hover:decoration-current"
  const endpoint =
    run.endpoint_id !== null ? (
      <Link to={editEndpointHref(run.endpoint_id, `/runs/${run.id}`, null)} className={link}>
        {run.endpoint_name}
      </Link>
    ) : (
      run.endpoint_name
    )
  const items: ReactNode[] = [<RunStatusPill status={run.status} failed={run.failed} />]
  items.push(
    showModel ? (
      <span className="min-w-0 [overflow-wrap:anywhere]">
        <span className="font-mono text-[13px] text-foreground">{run.model}</span> on {endpoint}
      </span>
    ) : (
      endpoint
    ),
  )
  if (showSet) {
    items.push(
      <Link to={`/prompts/${run.prompt_set_id}`} className={link}>
        {run.prompt_set_name}
      </Link>,
    )
  }
  items.push(<span title={dateTime(run.created_at)}>Created {relative(run.created_at)}</span>)
  return <Facts items={items} />
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
  const problem = useEndpointProblem(run)
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
      meta={<RunMeta run={r} />}
      actions={<RunActions run={r} onExport={() => setExportOpen(true)} primaryElsewhere={problem.pending || !!problem.error} />}
    >
      {lostContact ? (
        <p role="status" className="mb-4 flex flex-wrap items-center gap-x-3 rounded-lg bg-warning-tint px-3 py-2 text-sm text-warning-ink">
          Lost contact with the Stellarator server. Showing figures from {time(new Date(live.dataUpdatedAt).toISOString())}.
          <Button variant="outline" size="sm" className="bg-background" onClick={() => void live.refetch()}>
            Retry
          </Button>
        </p>
      ) : null}
      <RunNotice run={r} endpointError={problem.error} />

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
              <h2 id="inflight-heading" className="flex items-baseline gap-2 border-b px-4 py-3 text-base font-semibold">
                In flight
                <span className="text-sm font-normal text-muted-foreground tabular">{int(data.in_flight)}</span>
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
