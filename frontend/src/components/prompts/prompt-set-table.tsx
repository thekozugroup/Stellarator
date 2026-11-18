import { Link } from "react-router"
import { ChevronRight } from "lucide-react"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Bar } from "@/components/shared/block-states"
import { count, int, relative } from "@/lib/format"
import type { PromptSet } from "@/lib/types"
import { describeSource } from "./source"
import { SetStatus } from "./set-status"

export function PromptSetTable({ sets }: { sets: PromptSet[] }) {
  return (
    <>
      <div className="hidden lg:block">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-[38%]">Name</TableHead>
              <TableHead>Source</TableHead>
              <TableHead>Status</TableHead>
              <TableHead numeric>Prompts</TableHead>
              <TableHead numeric>Runs</TableHead>
              <TableHead className="text-right">Created</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sets.map((s) => (
              <TableRow
                key={s.id}
                className="relative"
              >
                <TableCell className="max-w-0 min-w-56">
                  <Link to={`/prompts/${s.id}`} title={s.name} className="block truncate rounded-sm font-medium break-words after:absolute after:inset-0 after:content-[''] focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none">
                    {s.name}
                  </Link>
                </TableCell>
                <TableCell className="max-w-0 min-w-48 text-muted-foreground">
                  <span title={describeSource(s)} className="block truncate">
                    {describeSource(s)}
                  </span>
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

      {/* Phone and tablet (as Runs): an inset list, one row per set, with the disclosure chevron. The
          table needs about 730 px and would run off a 768 to 1023 px content column. */}
      <ul className="divide-y lg:hidden">
        {sets.map((s) => (
          <li key={s.id}>
            <Link to={`/prompts/${s.id}`} className="-mx-2 flex min-h-14 items-center gap-3 rounded-lg px-2 py-2.5 active:bg-accent">
              <span className="grid min-w-0 flex-1 gap-0.5">
                <span className="line-clamp-2 font-medium break-words">{s.name}</span>
                <span className="truncate text-xs text-muted-foreground">{describeSource(s)}</span>
                <span className="text-xs text-muted-foreground">
                  {s.status !== "ready" ? (
                    <>
                      <SetStatus status={s.status} className="align-middle text-xs" /> ·{" "}
                    </>
                  ) : null}
                  <span className="tabular">{count(s.prompt_count, "prompt", "prompts")}</span> · <span className="tabular">{count(s.run_count, "run", "runs")}</span> · {relative(s.created_at)}
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
    <div aria-busy="true" aria-label="Loading prompt sets" className="divide-y">
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="grid gap-1.5 py-3 lg:grid-cols-[38%_1fr_5rem_5rem_4rem_6rem] lg:items-center lg:gap-4">
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
