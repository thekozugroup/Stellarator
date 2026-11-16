import { useEffect, useState } from "react"
import { Link } from "react-router"
import { useQuery } from "@tanstack/react-query"
import { Plus } from "lucide-react"
import { NewPreferenceSetDialog } from "@/components/preferences/new-set-dialog"
import { prefKeys } from "@/components/preferences/queries"
import { Bar, EmptyBlock, ErrorBlock } from "@/components/shared/block-states"
import { Page, Panel } from "@/components/shared/page"
import { ProgressBar } from "@/components/shared/progress-bar"
import { Button } from "@/components/ui/button"
import { api } from "@/lib/api"
import { int, middle, relative } from "@/lib/format"
import type { PreferenceSet, Run } from "@/lib/types"
import { cn } from "@/lib/utils"

function Progress({ set, className }: { set: PreferenceSet; className?: string }) {
  if (set.prompt_count === 0)
    return (
      <p className={cn("text-sm text-muted-foreground", className)} title="No prompt has two finished responses yet">
        Nothing to compare yet
      </p>
    )
  return (
    <div className={className}>
      <p className="tabular text-sm whitespace-nowrap">
        {int(set.decided)} of {int(set.prompt_count)} decided
      </p>
      <ProgressBar
        done={set.decided}
        total={set.prompt_count}
        label={`${int(set.decided)} of ${int(set.prompt_count)} prompts decided`}
        className="mt-1.5"
      />
    </div>
  )
}

function RunModels({ set }: { set: PreferenceSet }) {
  if (!set.runs.length) return <span className="text-muted-foreground">No runs left</span>
  return (
    <ul className="grid gap-0.5">
      {set.runs.map((r) => (
        <li key={r.id} className="truncate font-mono text-[13px] text-muted-foreground" title={`${r.name} (${r.model})`}>
          {middle(r.model, 44)}
        </li>
      ))}
    </ul>
  )
}

function SetsTable({ sets }: { sets: PreferenceSet[] }) {
  return (
    <>
      {/* Desktop and tablet: a table. */}
      <div className="hidden lg:block">
        <table className="w-full table-fixed text-sm">
          <thead>
            <tr className="text-left text-xs text-muted-foreground">
              <th scope="col" className="w-[28%] px-4 pt-3 pb-2 font-normal">Name</th>
              <th scope="col" className="w-[20%] px-4 pt-3 pb-2 font-normal">Prompt set</th>
              <th scope="col" className="w-[22%] px-4 pt-3 pb-2 font-normal">Runs</th>
              <th scope="col" className="w-[17%] px-4 pt-3 pb-2 text-right font-normal">Progress</th>
              <th scope="col" className="w-[13%] px-4 pt-3 pb-2 text-right font-normal">Created</th>
            </tr>
          </thead>
          <tbody>
            {sets.map((set) => (
              <tr key={set.id} className="relative border-t align-top hover:bg-muted/50">
                <td className="px-4 py-3">
                  <Link
                    to={`/preferences/${set.id}`}
                    className="line-clamp-2 font-medium break-words underline-offset-2 after:absolute after:inset-0 hover:underline"
                    title={set.name}
                  >
                    {set.name}
                  </Link>
                </td>
                <td className="px-4 py-3">
                  <Link
                    to={`/prompts/${set.prompt_set_id}`}
                    className="relative z-10 line-clamp-2 break-words text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
                    title={set.prompt_set_name}
                  >
                    {set.prompt_set_name}
                  </Link>
                </td>
                <td className="px-4 py-3">
                  <RunModels set={set} />
                </td>
                <td className="px-4 py-3 text-right">
                  <Progress set={set} className="ml-auto max-w-44" />
                </td>
                <td className="px-4 py-3 text-right whitespace-nowrap text-muted-foreground" title={new Date(set.created_at).toLocaleString()}>
                  {relative(set.created_at)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Phone: stacked rows with the name, the progress and the context. */}
      <ul className="lg:hidden">
        {sets.map((set) => (
          <li key={set.id} className="border-t first:border-t-0">
            <Link to={`/preferences/${set.id}`} className="grid gap-2 px-4 py-3 active:bg-muted">
              <span className="font-medium break-words">{set.name}</span>
              <Progress set={set} />
              <span className="grid gap-0.5 text-sm text-muted-foreground">
                <span className="truncate">{set.prompt_set_name}</span>
                {set.runs.map((r) => (
                  <span key={r.id} className="truncate font-mono text-[13px]">
                    {r.model}
                  </span>
                ))}
                <span>Created {relative(set.created_at)}</span>
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </>
  )
}

function LoadingRows() {
  return (
    <div aria-label="Loading preference sets…" className="divide-y">
      {[0, 1, 2].map((n) => (
        <div key={n} className="grid gap-2 px-4 py-4 md:grid-cols-[28%_20%_22%_17%_13%] md:gap-0">
          <Bar className="h-4 w-40" />
          <Bar className="h-4 w-28" />
          <Bar className="h-4 w-36" />
          <Bar className="h-4 w-28 md:ml-auto" />
          <Bar className="h-4 w-14 md:ml-auto" />
        </div>
      ))}
    </div>
  )
}

export function PreferencesPage() {
  const [open, setOpen] = useState(false)
  const sets = useQuery({ queryKey: prefKeys.list, queryFn: () => api.get<PreferenceSet[]>("/api/preference-sets") })
  const runs = useQuery({ queryKey: ["runs"], queryFn: () => api.get<Run[]>("/api/runs") })
  const noRuns = runs.isSuccess && runs.data.length === 0

  useEffect(() => {
    document.title = "Preferences · Stellarator"
    return () => {
      document.title = "Stellarator"
    }
  }, [])

  return (
    <Page
      title="Preferences"
      actions={
        <Button onClick={() => setOpen(true)}>
          <Plus aria-hidden="true" />
          New preference set
        </Button>
      }
    >
      <Panel bodyClassName="p-0">
        {sets.isPending ? (
          <LoadingRows />
        ) : sets.isError ? (
          <ErrorBlock
            className="m-4"
            message={`Couldn't load preference sets. ${sets.error.message}`}
            onRetry={() => sets.refetch()}
          />
        ) : sets.data.length === 0 ? (
          <EmptyBlock
            className="px-4"
            action={
              noRuns ? (
                <Button asChild variant="outline">
                  <Link to="/runs/new">New run</Link>
                </Button>
              ) : (
                <Button variant="outline" onClick={() => setOpen(true)}>
                  New preference set
                </Button>
              )
            }
          >
            {noRuns ? "No runs yet. Start a run with 2 or more samples per prompt." : "No preference sets yet."}
          </EmptyBlock>
        ) : (
          <SetsTable sets={sets.data} />
        )}
      </Panel>
      <NewPreferenceSetDialog open={open} onOpenChange={setOpen} />
    </Page>
  )
}
