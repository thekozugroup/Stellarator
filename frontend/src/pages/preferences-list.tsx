import { useEffect, useState } from "react"
import { Link } from "react-router"
import { useQuery } from "@tanstack/react-query"
import { ChevronRight, Plus } from "lucide-react"
import { NewPreferenceSetDialog } from "@/components/preferences/new-set-dialog"
import { prefKeys } from "@/components/preferences/queries"
import { Bar, EmptyBlock, ErrorBlock } from "@/components/shared/block-states"
import { Page } from "@/components/shared/page"
import { Plasma } from "@/components/shared/plasma"
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
    <div className={cn("grid gap-1.5", className)}>
      <p className="text-sm whitespace-nowrap tabular">
        {int(set.decided)} <span className="text-muted-foreground">of {int(set.prompt_count)} decided</span>
      </p>
      <ProgressBar
        done={set.decided}
        total={set.prompt_count}
        label={`${int(set.decided)} of ${int(set.prompt_count)} prompts decided`}
      />
    </div>
  )
}

/** The runs a set compares, by model, in mono. */
function RunModels({ set }: { set: PreferenceSet }) {
  if (!set.runs.length) return <span className="text-muted-foreground">No runs left</span>
  return (
    <ul className="grid gap-0.5">
      {set.runs.map((r) => (
        <li key={r.id} className="truncate font-mono text-xs text-muted-foreground" title={`${r.name} (${r.model})`}>
          {middle(r.model, 44)}
        </li>
      ))}
    </ul>
  )
}

/** A prompt set's name beside its pixel-field tile (DESIGN.md, Plasma tile). */
function PromptSetName({ set, className }: { set: PreferenceSet; className?: string }) {
  return (
    <span className={cn("flex min-w-0 items-center gap-2", className)}>
      <Plasma seed={`prompt-set-${set.prompt_set_id}`} cells={5} className="size-5 rounded-xs" />
      <span className="min-w-0 truncate">{set.prompt_set_name}</span>
    </span>
  )
}

function SetsTable({ sets }: { sets: PreferenceSet[] }) {
  return (
    <>
      {/* From 1024 px: a table. */}
      <div className="hidden lg:block">
        <Table className="table-fixed">
          <TableHeader>
            <TableRow className="hover:[&>td]:bg-transparent">
              <TableHead className="w-[28%]">Name</TableHead>
              <TableHead className="w-[22%]">Prompt set</TableHead>
              <TableHead className="w-[22%]">Runs</TableHead>
              <TableHead className="w-[16%]">Progress</TableHead>
              <TableHead className="w-[12%] text-right">Created</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sets.map((set) => (
              <TableRow key={set.id} className="relative">
                <TableCell className="py-2 whitespace-normal">
                  <Link
                    to={`/preferences/${set.id}`}
                    className="line-clamp-2 rounded-xs font-medium break-words after:absolute after:inset-0 after:content-[''] focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none"
                    title={set.name}
                  >
                    {set.name}
                  </Link>
                </TableCell>
                <TableCell className="py-2">
                  <Link
                    to={`/prompts/${set.prompt_set_id}`}
                    className="relative z-10 -mx-1 flex w-fit max-w-[calc(100%+0.5rem)] rounded-xs px-1 text-muted-foreground transition-colors duration-150 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                    title={set.prompt_set_name}
                  >
                    <PromptSetName set={set} />
                  </Link>
                </TableCell>
                <TableCell className="py-2 whitespace-normal">
                  <RunModels set={set} />
                </TableCell>
                <TableCell className="py-2">
                  <Progress set={set} className="max-w-44" />
                </TableCell>
                <TableCell className="text-right text-muted-foreground" title={new Date(set.created_at).toLocaleString()}>
                  {relative(set.created_at)}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {/* Phone and tablet: one stacked row per set with the name, the progress and the context. */}
      <ul className="lg:hidden">
        {sets.map((set) => (
          <li key={set.id} className="border-b last:border-b-0">
            <Link
              to={`/preferences/${set.id}`}
              className="-mx-2 flex items-center gap-3 rounded-md px-2 py-3 outline-none focus-visible:ring-2 focus-visible:ring-ring active:bg-accent"
            >
              <span className="grid min-w-0 flex-1 gap-2">
                <span className="grid gap-0.5">
                  <span className="font-medium break-words">{set.name}</span>
                  <PromptSetName set={set} className="text-xs text-muted-foreground" />
                </span>
                <Progress set={set} />
                <span className="grid gap-0.5 text-xs text-muted-foreground">
                  {set.runs.map((r) => (
                    <span key={r.id} className="truncate font-mono">
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
    <div aria-busy="true" aria-label="Loading preference sets…">
      <div className="hidden h-8 rounded-md bg-muted lg:block" />
      {[0, 1, 2].map((n) => (
        <div key={n} className="grid items-center gap-2 border-b py-3 last:border-b-0 lg:h-14 lg:grid-cols-[28%_22%_22%_16%_12%] lg:gap-0 lg:px-3 lg:py-0">
          <Bar className="h-4 w-40" />
          <Bar className="h-4 w-28" />
          <Bar className="h-4 w-36" />
          <Bar className="h-1.5 w-28 rounded-full" />
          <Bar className="h-4 w-14 lg:ml-auto" />
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
        phone || (sets.isSuccess && sets.data.length === 0) ? null : (
          <Button onClick={() => setOpen(true)}>
            <Plus aria-hidden="true" />
            New preference set
          </Button>
        )
      }
      barActions={
        sets.isSuccess && sets.data.length === 0 ? null : (
          <Button variant="secondary" size="icon" aria-label="New preference set" title="New preference set" onClick={() => setOpen(true)}>
            <Plus aria-hidden="true" />
          </Button>
        )
      }
    >
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
            className="gap-4 rounded-xl bg-muted p-6"
            action={
              noRuns ? (
                <Button asChild>
                  <Link to="/runs/new">New run</Link>
                </Button>
              ) : (
                <Button onClick={() => setOpen(true)}>New preference set</Button>
              )
            }
          >
            <Plasma seed="preferences-empty" cells={8} className="size-14 rounded-md" />
            <p className="mt-4">
              {noRuns
                ? "No runs yet. Start a run with 2 or more samples per prompt, then compare its responses here."
                : "No preference sets yet. Pick runs on one prompt set and choose the better response to each prompt."}
            </p>
          </EmptyBlock>
        ) : (
          <SetsTable sets={sets.data} />
        )}
      </>
      <NewPreferenceSetDialog open={open} onOpenChange={setOpen} />
    </Page>
  )
}
