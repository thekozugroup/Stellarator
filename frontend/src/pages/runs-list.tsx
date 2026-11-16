import { useEffect } from "react"
import { Link } from "react-router"
import { Plus } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Page } from "@/components/shared/page"
import { ProgressBar } from "@/components/shared/progress-bar"
import { RunStatusPill } from "@/components/shared/status-pill"
import { SetupChecklist } from "@/components/runs/setup-checklist"
import { useRuns } from "@/components/runs/run-data"
import { Button } from "@/components/ui/button"
import { dateTime, int, relative } from "@/lib/format"
import type { Run } from "@/lib/types"

/** "1,284 of 5,000" over the bar; the failed count in red only when there is one. */
function RunProgress({ run }: { run: Run }) {
  return (
    <div className="grid gap-1.5">
      <div className="flex items-baseline justify-between gap-3 text-sm tabular">
        <span className="whitespace-nowrap">
          {int(run.done)} <span className="text-muted-foreground">of {int(run.total)}</span>
        </span>
        {run.failed > 0 ? <span className="whitespace-nowrap text-destructive">{int(run.failed)} failed</span> : null}
      </div>
      <ProgressBar done={run.done} failed={run.failed} total={run.total} />
    </div>
  )
}

function RunsTable({ runs }: { runs: Run[] }) {
  return (
    <>
      {/* Tablet and desktop: a table. The name link covers its row. */}
      <div className="hidden overflow-hidden rounded-lg border lg:block">
        <table className="w-full table-fixed text-sm">
          <caption className="sr-only">Runs, newest first</caption>
          <thead>
            <tr className="text-left text-xs text-muted-foreground">
              <th scope="col" className="h-10 px-4 font-normal">Name</th>
              <th scope="col" className="hidden h-10 w-[14%] px-3 font-normal xl:table-cell">Prompt set</th>
              <th scope="col" className="h-10 w-[11rem] px-3 font-normal">Progress</th>
              <th scope="col" className="hidden h-10 w-[4rem] px-3 text-right font-normal xl:table-cell">Seqs</th>
              <th scope="col" className="h-10 w-[11.5rem] px-3 font-normal">Status</th>
              <th scope="col" className="h-10 w-[8.5rem] px-4 text-right font-normal">Created</th>
            </tr>
          </thead>
          <tbody>
            {runs.map((run) => (
              <tr key={run.id} className="relative border-t transition-colors duration-150 hover:bg-muted/60 has-[a:focus-visible]:bg-muted/60">
                <td className="px-4 py-3 align-middle">
                  <Link
                    to={`/runs/${run.id}`}
                    title={run.name}
                    className="block truncate font-medium outline-none after:absolute after:inset-0 after:content-[''] focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-ring"
                  >
                    {run.name}
                  </Link>
                  <p className="mt-0.5 truncate text-xs text-muted-foreground" title={`${run.model} on ${run.endpoint_name}`}>
                    <span className="font-mono">{run.model}</span> on {run.endpoint_name}
                  </p>
                </td>
                <td className="hidden truncate px-3 py-3 text-muted-foreground xl:table-cell" title={run.prompt_set_name}>
                  {run.prompt_set_name}
                </td>
                <td className="px-3 py-3">
                  <RunProgress run={run} />
                </td>
                <td className="hidden px-3 py-3 text-right tabular xl:table-cell">{int(run.concurrency)}</td>
                <td className="px-3 py-3">
                  <RunStatusPill status={run.status} failed={run.failed} />
                </td>
                <td className="px-4 py-3 text-right whitespace-nowrap text-muted-foreground tabular" title={dateTime(run.created_at)}>
                  {relative(run.created_at)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Phone: one row per run with its name, progress and status. */}
      <ul className="divide-y border-y lg:hidden">
        {runs.map((run) => (
          <li key={run.id} className="relative py-3">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <Link
                  to={`/runs/${run.id}`}
                  className="block truncate font-medium outline-none after:absolute after:inset-0 after:content-[''] focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-ring"
                >
                  {run.name}
                </Link>
                <p className="mt-0.5 truncate text-xs text-muted-foreground">
                  <span className="font-mono">{run.model}</span>
                  <span className="hidden sm:inline"> on {run.endpoint_name}, {relative(run.created_at)}</span>
                </p>
              </div>
              <RunStatusPill status={run.status} failed={run.failed} />
            </div>
            <div className="mt-2.5">
              <RunProgress run={run} />
            </div>
          </li>
        ))}
      </ul>
    </>
  )
}

function RunsLoading() {
  return (
    <div className="overflow-hidden rounded-lg border" aria-busy="true" aria-label="Loading runs">
      <div className="hidden h-10 items-center gap-4 px-4 text-xs text-muted-foreground lg:flex">
        <span className="flex-1">Name</span>
        <span className="w-[11rem]">Progress</span>
        <span className="w-[11.5rem]">Status</span>
        <span className="w-[6rem] text-right">Created</span>
      </div>
      {[0, 1, 2].map((i) => (
        <div key={i} className="flex h-[61px] items-center gap-4 border-t px-4 first:border-t-0 lg:first:border-t">
          <div className="grid flex-1 gap-1.5">
            <Bar className="w-48" />
            <Bar className="h-3 w-64 max-w-full" />
          </div>
          <div className="hidden w-[11rem] gap-1.5 lg:grid">
            <Bar className="w-24" />
            <Bar className="h-1.5 w-full" />
          </div>
          <span className="hidden w-[11.5rem] lg:block">
            <Bar className="h-5 w-20 rounded-full" />
          </span>
          <span className="hidden w-[6rem] text-right lg:block">
            <Bar className="w-16" />
          </span>
        </div>
      ))}
    </div>
  )
}

export function RunsListPage() {
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
      actions={
        <Button asChild>
          <Link to="/runs/new">
            <Plus aria-hidden="true" />
            New run
          </Link>
        </Button>
      }
    >
      {runs.isPending ? (
        <RunsLoading />
      ) : runs.isError ? (
        <ErrorBlock message={`Couldn't load runs. ${runs.error.message}`} onRetry={() => void runs.refetch()} />
      ) : runs.data.length === 0 ? (
        <div className="rounded-lg border px-4 py-4 sm:px-6">
          <SetupChecklist />
        </div>
      ) : (
        <RunsTable runs={runs.data} />
      )}
    </Page>
  )
}
