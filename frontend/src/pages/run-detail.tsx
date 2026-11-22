import { useEffect, useState, type ReactNode } from "react"
import { Link, useParams } from "react-router"
import { useQuery } from "@tanstack/react-query"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Chip, ChipRow } from "@/components/shared/chip"
import { Page, Panel } from "@/components/shared/page"
import { Plasma } from "@/components/shared/plasma"
import { RunStatusBadge } from "@/components/shared/status-pill"
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
import { Missing } from "./not-found"

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
 * A notice above the facts (DESIGN.md: cards by tone, color on the fact): a --muted card with a
 * dot and the text in the signal's ink, red for a failure and coral for a pause. Its action is a
 * small button inset 8 px, so its 8 px corners sit concentric in the card's 16.
 */
function Notice({
  tone,
  role,
  action,
  children,
}: {
  tone: "negative" | "attention"
  role?: "status"
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <div
      role={role}
      className={cn("mb-2 flex min-h-11 flex-wrap items-center gap-x-3 gap-y-2 rounded-xl bg-muted py-2 pl-4 text-sm", action ? "pr-2" : "pr-4")}
    >
      <p className={cn("flex min-w-0 flex-1 basis-64 items-start gap-2.5", tone === "negative" ? "text-negative-ink" : "text-coral-ink")}>
        <span
          aria-hidden="true"
          className={cn("mt-[calc(0.5lh-3px)] size-1.5 flex-none rounded-full", tone === "negative" ? "bg-destructive" : "bg-coral")}
        />
        <span className="min-w-0">{children}</span>
      </p>
      {action}
    </div>
  )
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
  const paused = run.status === "paused" || run.status === "pausing"
  const fixEndpoint = run.endpoint_id !== null && (primary || (problem && ENDPOINT_PROBLEM.test(text)))
  const action =
    fixEndpoint && run.endpoint_id !== null ? (
      <Button asChild variant={primary ? "default" : "secondary"} size="sm">
        <Link to={editEndpointHref(run.endpoint_id, `/runs/${run.id}`, /could not connect|unreachable|timed out|took too long/i.test(text) ? "url" : "key")}>
          Edit endpoint
        </Link>
      </Button>
    ) : undefined
  if (!problem && !paused) return <p className="mb-4 text-sm text-muted-foreground">{text}</p>
  return (
    <Notice tone={problem ? "negative" : "attention"} action={action}>
      {text}
    </Notice>
  )
}

/**
 * Under the title (DESIGN.md, Layout): the status badge, then the objects the run uses as chips:
 * the model (mono), the endpoint and the prompt set (links, each with its pixel tile), then when it
 * was created, as plain text.
 */
function RunChips({ run }: { run: Run }) {
  return (
    <ChipRow>
      <RunStatusBadge status={run.status} />
      <Chip mono title="Model">
        {run.model}
      </Chip>
      {run.endpoint_id !== null ? (
        <Chip
          to={editEndpointHref(run.endpoint_id, `/runs/${run.id}`, null)}
          title={`Endpoint: ${run.endpoint_name}`}
          leading={<Plasma seed={`endpoint-${run.endpoint_id}`} cells={5} className="size-5 rounded-xs" />}
        >
          {run.endpoint_name}
        </Chip>
      ) : (
        <Chip title="Endpoint (deleted)">{run.endpoint_name}</Chip>
      )}
      <Chip
        to={`/prompts/${run.prompt_set_id}`}
        title={`Prompt set: ${run.prompt_set_name}`}
        leading={<Plasma seed={`prompt-set-${run.prompt_set_id}`} cells={5} className="size-5 rounded-xs" />}
      >
        {run.prompt_set_name}
      </Chip>
      {/* When it was made is not an object it uses: plain text at the end of the row. */}
      <span title={dateTime(run.created_at)} className="text-sm whitespace-nowrap text-muted-foreground">
        Created {relative(run.created_at)}
      </span>
    </ChipRow>
  )
}

function DetailSkeleton() {
  return (
    <Page
      title={null}
      wide
      back={{ to: "/runs", label: "Runs" }}
      meta={
        <ChipRow>
          {["w-20", "w-40", "w-32", "w-28"].map((w) => (
            <Bar key={w} className={cn("h-7 rounded-full", w)} />
          ))}
        </ChipRow>
      }
    >
      <div aria-busy="true" aria-label="Loading run" className="grid gap-2">
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-5">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className={cn("grid gap-2 rounded-xl bg-muted p-4", i === 0 && "col-span-2 sm:col-span-3 lg:col-span-2")}>
              <Bar className="h-7 w-20" />
              <Bar className="h-3 w-24" />
            </div>
          ))}
        </div>
        <div className="h-[260px] rounded-xl bg-muted sm:h-[320px]" />
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
        <Missing seed="run-missing" to="/runs" action="See all runs">
          There is no run with this address. It may have been deleted.
        </Missing>
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
  const hasThroughput = data.metrics.length >= 2 && data.metrics.some((m) => m.tokens_per_s > 0)
  // The chart's card also holds the seqs control, so it stays while the run can still go; a
  // finished run that drew no throughput has nothing to chart. Nothing is in flight unless it is going.
  const showChart = !finished || hasThroughput

  return (
    <Page
      wide
      title={r.name}
      back={{ to: "/runs", label: "Runs" }}
      meta={<RunChips run={r} />}
      actions={<RunActions run={r} onExport={() => setExportOpen(true)} primaryElsewhere={problem.pending || !!problem.error} />}
    >
      {lostContact ? (
        <Notice
          tone="attention"
          role="status"
          action={
            <Button variant="secondary" size="sm" onClick={() => void live.refetch()}>
              Retry
            </Button>
          }
        >
          Lost contact with the Stellarator server. Showing figures from {time(new Date(live.dataUpdatedAt).toISOString())}.
        </Notice>
      ) : null}
      <RunNotice run={r} endpointError={problem.error} />

      <RunProgress live={data} />

      {showChart ? (
        <div className={cn("mt-2 grid grid-cols-1 gap-2", active && "xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]")}>
          <Panel
            title="Throughput"
            description={r.completion_tokens > 0 ? `${int(r.completion_tokens)} output tokens` : undefined}
            actions={
              finished ? (
                // A finished run's seqs can no longer change, so it is a plain figure here.
                <p className="text-sm text-muted-foreground">
                  Seqs <span className="ml-1 font-mono text-foreground">{int(r.concurrency)}</span>
                </p>
              ) : (
                <SeqsControl run={r} />
              )
            }
            bodyClassName="pt-2"
          >
            <ThroughputChart
              metrics={data.metrics}
              live={active && !lostContact ? { tps: data.tokens_per_s, seqs: r.concurrency, active: data.in_flight, at: live.dataUpdatedAt } : null}
            />
          </Panel>
          {active ? (
            <Panel
              title={
                <>
                  In flight <span className="ml-1 font-normal text-muted-foreground tabular">{int(data.in_flight)}</span>
                </>
              }
              // The scroll region's focus ring goes round the whole card, outside the fade.
              className="flex flex-col has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-ring xl:[contain:size]"
              bodyClassName="flex min-h-0 flex-1 flex-col p-0"
            >
              {/* The last 24 px fade out, so a row cut at the edge reads as more below; the bottom
                  padding is as tall, so the last row is clear once scrolled to the end. */}
              <div
                tabIndex={data.sequences.length ? 0 : undefined}
                role={data.sequences.length ? "region" : undefined}
                aria-label={data.sequences.length ? "Sequences in flight" : undefined}
                className="max-h-[28rem] min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pt-1 pb-6 outline-none [-webkit-mask-image:linear-gradient(to_bottom,#000_calc(100%-1.5rem),transparent)] [mask-image:linear-gradient(to_bottom,#000_calc(100%-1.5rem),transparent)] xl:max-h-none" /* taste-ignore: outline-check -- the card around it draws the ring (has-[:focus-visible]:outline-2) */
              >
                <InFlightList sequences={data.sequences} inFlight={data.in_flight} samplesPerPrompt={r.params.samples_per_prompt} />
              </div>
            </Panel>
          ) : null}
        </div>
      ) : null}

      <div className="mt-8">
        <SamplesSection run={r} />
      </div>

      <ExportDialog run={r} open={exportOpen} onOpenChange={setExportOpen} />
    </Page>
  )
}
