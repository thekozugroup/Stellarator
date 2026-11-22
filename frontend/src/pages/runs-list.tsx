import { Fragment, useEffect, type ReactNode } from "react"
import { Link } from "react-router"
import { Plus } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Page } from "@/components/shared/page"
import { Plasma } from "@/components/shared/plasma"
import { ProgressBar } from "@/components/shared/progress-bar"
import { RunStatusPill } from "@/components/shared/status-pill"
import { SetupChecklist } from "@/components/runs/setup-checklist"
import { useRuns } from "@/components/runs/run-data"
import { Button } from "@/components/ui/button"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { dateTime, int, relative } from "@/lib/format"
import { TOOLBAR_QUERY, useMediaQuery } from "@/lib/media"
import type { Run } from "@/lib/types"

/** The bar with "1,284 of 5,000" beside it on one line; the failed count in red only when there is one. */
function RunProgress({ run, stacked = false }: { run: Run; stacked?: boolean }) {
  const figures = (
    <span className="flex items-baseline gap-x-2 text-sm whitespace-nowrap tabular">
      <span>
        {int(run.done)} <span className="text-muted-foreground">of {int(run.total)}</span>
      </span>
      {run.failed > 0 ? <span className="text-destructive">{int(run.failed)} failed</span> : null}
    </span>
  )
  if (stacked) {
    return (
      <div className="grid gap-1.5">
        <div className="flex items-baseline justify-between gap-3">{figures}</div>
        <ProgressBar done={run.done} failed={run.failed} total={run.total} />
      </div>
    )
  }
  return (
    <div className="flex items-center gap-3">
      <ProgressBar done={run.done} failed={run.failed} total={run.total} className="w-20 flex-none" />
      {figures}
    </div>
  )
}

/**
 * The model, endpoint and prompt set under a run's name, minus what the name already says: a
 * default name ("<model> on <set>") leaves only the endpoint.
 */
function RunFacts({ run }: { run: Run }) {
  const name = run.name.toLowerCase()
  const parts: ReactNode[] = []
  if (!name.includes(run.model.toLowerCase())) parts.push(<span className="font-mono">{run.model}</span>)
  parts.push(run.endpoint_name)
  if (!name.includes(run.prompt_set_name.toLowerCase())) parts.push(run.prompt_set_name)
  return (
    <>
      {parts.map((p, i) => (
        <Fragment key={i}>
          {i > 0 ? " · " : null}
          {p}
        </Fragment>
      ))}
    </>
  )
}

function factsTitle(run: Run) {
  return `${run.model} on ${run.endpoint_name} · ${run.prompt_set_name}`
}

function RunsTable({ runs }: { runs: Run[] }) {
  return (
    <>
      {/* Tablet and desktop: a list. The name link covers its row. */}
      <div className="hidden lg:block">
        <Table className="table-fixed">
          <caption className="sr-only">Runs, newest first</caption>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead scope="col">Name</TableHead>
              <TableHead scope="col" className="w-[17.5rem]">Progress</TableHead>
              <TableHead scope="col" numeric className="hidden w-[4.5rem] xl:table-cell">Seqs</TableHead>
              <TableHead scope="col" className="w-[8rem]">Status</TableHead>
              <TableHead scope="col" numeric className="w-[7rem]">Created</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {runs.map((run) => (
              <TableRow key={run.id} className="relative">
                <TableCell>
                  <Link
                    data-touch="inline"
                    to={`/runs/${run.id}`}
                    title={`${run.name}\n${factsTitle(run)}`}
                    className="block truncate outline-none after:absolute after:inset-0 after:content-[''] focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-ring"
                  >
                    <span className="font-medium">{run.name}</span>
                    <span className="ml-2 text-xs text-muted-foreground">
                      <RunFacts run={run} />
                    </span>
                  </Link>
                </TableCell>
                <TableCell>
                  <RunProgress run={run} />
                </TableCell>
                <TableCell numeric className="hidden xl:table-cell">{int(run.concurrency)}</TableCell>
                <TableCell>
                  <RunStatusPill status={run.status} failed={run.failed} />
                </TableCell>
                <TableCell numeric className="text-muted-foreground" title={dateTime(run.created_at)}>
                  {relative(run.created_at)}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {/* Phone and tablet: one row per run, its name and status, then its facts and progress. */}
      <ul className="divide-y lg:hidden">
        {runs.map((run) => (
          <li key={run.id} className="relative -mx-3 rounded-md px-3 py-3 has-[a:focus-visible]:bg-accent">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <Link
                  data-touch="inline"
                  to={`/runs/${run.id}`}
                  className="block truncate font-medium outline-none after:absolute after:inset-0 after:rounded-md after:content-[''] focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-ring"
                >
                  {run.name}
                </Link>
                <p className="mt-0.5 truncate text-xs text-muted-foreground">
                  <RunFacts run={run} />
                  <span className="hidden sm:inline"> · {relative(run.created_at)}</span>
                </p>
              </div>
              <RunStatusPill status={run.status} failed={run.failed} />
            </div>
            <div className="mt-2.5">
              <RunProgress run={run} stacked />
            </div>
          </li>
        ))}
      </ul>
    </>
  )
}

function RunsLoading() {
  return (
    <div aria-busy="true" aria-label="Loading runs">
      <div className="hidden h-8 items-center gap-4 rounded-md bg-muted px-3 text-xs text-muted-foreground lg:flex">
        <span className="flex-1">Name</span>
        <span className="w-[17.5rem]">Progress</span>
        <span className="w-[8rem]">Status</span>
        <span className="w-[7rem] text-right">Created</span>
      </div>
      {[0, 1, 2].map((i) => (
        <div key={i} className="flex h-[76px] items-center gap-4 border-b lg:h-11 lg:px-3">
          <div className="grid flex-1 gap-1.5 lg:block">
            <Bar className="w-48" />
            <Bar className="h-3 w-64 max-w-full lg:hidden" />
            <Bar className="h-1.5 w-full rounded-full lg:hidden" />
          </div>
          <span className="hidden w-[17.5rem] lg:block">
            <Bar className="w-40" />
          </span>
          <span className="hidden w-[8rem] lg:block">
            <Bar className="w-20" />
          </span>
          <span className="hidden w-[7rem] text-right lg:block">
            <Bar className="w-16" />
          </span>
        </div>
      ))}
    </div>
  )
}

/**
 * No runs yet: the list keeps its frame (the header band) and says what will appear there, then
 * the steps to a first run.
 */
function RunsEmpty() {
  return (
    <div className="grid gap-4">
      <div aria-hidden="true" className="hidden h-8 items-center gap-4 rounded-md bg-muted px-3 text-xs text-muted-foreground lg:flex">
        <span className="flex-1">Name</span>
        <span className="w-[17.5rem]">Progress</span>
        <span className="w-[8rem]">Status</span>
        <span className="w-[7rem] text-right">Created</span>
      </div>
      <div className="flex items-center gap-4 lg:px-3">
        <Plasma seed="runs-empty" cells={8} className="size-14 rounded-md" />
        <p className="max-w-[60ch] text-sm text-muted-foreground">
          No runs yet. Each run you start shows here with its progress and status, newest first.
        </p>
      </div>
      <SetupChecklist />
    </div>
  )
}

/** Creating a run: a push button in the toolbar, or a "+" in the phone bar (Page's barActions). */
const newRun = (
  <Button asChild>
    <Link to="/runs/new">
      <Plus aria-hidden="true" />
      New run
    </Link>
  </Button>
)
const newRunIcon = (
  <Button asChild variant="secondary" size="icon">
    <Link to="/runs/new" aria-label="New run" title="New run">
      <Plus aria-hidden="true" />
    </Link>
  </Button>
)

export function RunsListPage() {
  const phone = !useMediaQuery(TOOLBAR_QUERY)
  const runs = useRuns()
  const running = runs.data?.filter((r) => r.status === "running").length ?? 0
  useEffect(() => {
    document.title = running ? `Runs · ${running} running · Stellarator` : "Runs · Stellarator"
    return () => {
      document.title = "Stellarator"
    }
  }, [running])

  return (
    <Page
      title="Runs"
      actions={phone ? null : newRun}
      barActions={newRunIcon}
    >
      {runs.isPending ? (
        <RunsLoading />
      ) : runs.isError ? (
        <ErrorBlock message={`Couldn't load runs. ${runs.error.message}`} onRetry={() => void runs.refetch()} />
      ) : runs.data.length === 0 ? (
        <RunsEmpty />
      ) : (
        <RunsTable runs={runs.data} />
      )}
    </Page>
  )
}
