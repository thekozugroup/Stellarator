import { Link } from "react-router"
import { ChevronRight } from "lucide-react"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Bar } from "@/components/shared/block-states"
import { Plasma } from "@/components/shared/plasma"
import { count, int, relative } from "@/lib/format"
import type { PromptSet } from "@/lib/types"
import { describeSource, sourceParts } from "./source"
import { SetStatus } from "./set-status"

/** The source of a set: the kind in words, the dataset ID or file name in mono. */
function SourceText({ set, className }: { set: PromptSet; className?: string }) {
  const p = sourceParts(set)
  return (
    <span title={describeSource(set)} className={className}>
      {p.id ? null : p.kind}
      {p.id ? <span className="font-mono text-xs">{p.id}</span> : null}
      {p.where ? <span className="font-mono text-xs text-muted-foreground"> · {p.where}</span> : null}
    </span>
  )
}

export function PromptSetTable({ sets }: { sets: PromptSet[] }) {
  return (
    <>
      <div className="hidden lg:block">
        <Table className="table-fixed">
          <TableHeader>
            <TableRow className="hover:[&>td]:bg-transparent">
              <TableHead className="w-[34%]">Name</TableHead>
              <TableHead>Source</TableHead>
              <TableHead className="w-28">Status</TableHead>
              <TableHead numeric className="w-24">
                Prompts
              </TableHead>
              <TableHead numeric className="w-20">
                Runs
              </TableHead>
              <TableHead className="w-28 text-right">Created</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sets.map((s) => (
              <TableRow key={s.id} className="relative">
                <TableCell>
                  <div className="flex min-w-0 items-center gap-3">
                    <Plasma seed={`prompt-set-${s.id}`} />
                    <Link
                      to={`/prompts/${s.id}`}
                      title={s.name}
                      className="min-w-0 truncate rounded-sm font-medium after:absolute after:inset-0 after:content-[''] focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                    >
                      {s.name}
                    </Link>
                  </div>
                </TableCell>
                <TableCell className="max-w-0 truncate text-muted-foreground">
                  <SourceText set={s} />
                </TableCell>
                <TableCell>
                  <SetStatus status={s.status} />
                </TableCell>
                <TableCell numeric>{int(s.prompt_count)}</TableCell>
                <TableCell numeric>{int(s.run_count)}</TableCell>
                <TableCell className="text-right text-muted-foreground">{relative(s.created_at)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {/* Phone and tablet (as Runs): one stacked row per set with its facts on a second line. The
          table needs about 730 px and would run off a 768 to 1023 px content column. */}
      <ul className="lg:hidden">
        {sets.map((s) => (
          <li key={s.id} className="border-b last:border-b-0">
            <Link
              to={`/prompts/${s.id}`}
              className="-mx-2 flex min-h-14 items-center gap-3 rounded-md px-2 py-2.5 outline-none focus-visible:ring-2 focus-visible:ring-ring active:bg-accent"
            >
              <Plasma seed={`prompt-set-${s.id}`} />
              <span className="grid min-w-0 flex-1 gap-0.5">
                <span className="line-clamp-2 font-medium break-words">{s.name}</span>
                <SourceText set={s} className="truncate text-xs text-muted-foreground" />
                <span className="text-xs text-muted-foreground">
                  {s.status !== "ready" ? (
                    <>
                      <SetStatus status={s.status} className="align-middle text-xs" /> ·{" "}
                    </>
                  ) : null}
                  <span className="tabular">{count(s.prompt_count, "prompt", "prompts")}</span> ·{" "}
                  <span className="tabular">{count(s.run_count, "run", "runs")}</span> · {relative(s.created_at)}
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

export function PromptSetTableSkeleton() {
  return (
    <div aria-busy="true" aria-label="Loading prompt sets">
      {[0, 1, 2, 3].map((i) => (
        <div
          key={i}
          className="grid items-center gap-1.5 border-b py-3 last:border-b-0 lg:h-11 lg:grid-cols-[34%_1fr_7rem_6rem_5rem_7rem] lg:gap-0 lg:px-3 lg:py-0"
        >
          <Bar className="h-5 w-48 max-w-full" />
          <Bar className="h-4 w-52 max-w-full" />
          <Bar className="h-4 w-14" />
          <Bar className="h-4 w-12 lg:justify-self-end" />
          <Bar className="h-4 w-8 lg:justify-self-end" />
          <Bar className="h-4 w-16 lg:justify-self-end" />
        </div>
      ))}
    </div>
  )
}
