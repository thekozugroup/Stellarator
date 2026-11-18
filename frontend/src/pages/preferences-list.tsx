import { useEffect, useState } from "react"
import { Link } from "react-router"
import { useQuery } from "@tanstack/react-query"
import { ChevronRight, Plus } from "lucide-react"
import { NewPreferenceSetDialog } from "@/components/preferences/new-set-dialog"
import { prefKeys } from "@/components/preferences/queries"
import { Bar, EmptyBlock, ErrorBlock } from "@/components/shared/block-states"
import { Page } from "@/components/shared/page"
import { PhoneToolbarAction } from "@/components/shared/phone-chrome"
import { ProgressBar } from "@/components/shared/progress-bar"
import { Button } from "@/components/ui/button"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api } from "@/lib/api"
import { TOOLBAR_QUERY, useMediaQuery } from "@/lib/media"
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
        <Table className="table-fixed">
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-[28%]">Name</TableHead>
              <TableHead className="w-[20%]">Prompt set</TableHead>
              <TableHead className="w-[22%]">Runs</TableHead>
              <TableHead className="w-[17%] text-right">Progress</TableHead>
              <TableHead className="w-[13%] text-right">Created</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sets.map((set) => (
              <TableRow key={set.id} className="relative">
                <TableCell className="py-1.5 whitespace-normal">
                  <Link
                    to={`/preferences/${set.id}`}
                    className="line-clamp-2 rounded-sm font-medium break-words after:absolute after:inset-0 focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
                    title={set.name}
                  >
                    {set.name}
                  </Link>
                </TableCell>
                <TableCell className="py-1.5 whitespace-normal">
                  <Link
                    to={`/prompts/${set.prompt_set_id}`}
                    className="relative z-10 line-clamp-2 rounded-sm break-words text-muted-foreground underline-offset-2 hover:text-foreground hover:underline focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
                    title={set.prompt_set_name}
                  >
                    {set.prompt_set_name}
                  </Link>
                </TableCell>
                <TableCell className="py-1.5 whitespace-normal">
                  <RunModels set={set} />
                </TableCell>
                <TableCell className="py-1.5 text-right">
                  <Progress set={set} className="ml-auto max-w-44" />
                </TableCell>
                <TableCell className="text-right text-muted-foreground" title={new Date(set.created_at).toLocaleString()}>
                  {relative(set.created_at)}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {/* Phone: stacked rows with the name, the progress and the context. */}
      <ul className="divide-y lg:hidden">
        {sets.map((set) => (
          <li key={set.id}>
            <Link to={`/preferences/${set.id}`} className="-mx-2 flex items-center gap-3 rounded-lg px-2 py-3 active:bg-accent">
              <span className="grid min-w-0 flex-1 gap-2">
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
              </span>
              <ChevronRight className="size-4 flex-none text-muted-foreground" aria-hidden="true" />
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
        <div key={n} className="grid gap-2 py-3 md:grid-cols-[28%_20%_22%_17%_13%] md:gap-0">
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
  const phone = !useMediaQuery(TOOLBAR_QUERY)

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
        phone ? null : (
          <Button onClick={() => setOpen(true)}>
            <Plus aria-hidden="true" />
            New preference set
          </Button>
        )
      }
    >
      <PhoneToolbarAction>
        <Button variant="outline" size="icon" aria-label="New preference set" title="New preference set" onClick={() => setOpen(true)}>
          <Plus aria-hidden="true" />
        </Button>
      </PhoneToolbarAction>
      <>
        {sets.isPending ? (
          <LoadingRows />
        ) : sets.isError ? (
          <ErrorBlock
            message={`Couldn't load preference sets. ${sets.error.message}`}
            onRetry={() => sets.refetch()}
          />
        ) : sets.data.length === 0 ? (
          <EmptyBlock
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
      </>
      <NewPreferenceSetDialog open={open} onOpenChange={setOpen} />
    </Page>
  )
}
