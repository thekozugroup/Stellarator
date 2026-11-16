import { Link, useNavigate } from "react-router"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Bar } from "@/components/shared/block-states"
import { count, int, relative } from "@/lib/format"
import type { PromptSet } from "@/lib/types"
import { describeSource } from "./source"

const head = "text-xs font-normal text-muted-foreground"

export function PromptSetTable({ sets }: { sets: PromptSet[] }) {
  const navigate = useNavigate()
  return (
    <>
      <div className="hidden md:block">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className={`${head} w-[38%]`}>Name</TableHead>
              <TableHead className={head}>Source</TableHead>
              <TableHead className={`${head} text-right`}>Prompts</TableHead>
              <TableHead className={`${head} text-right`}>Runs</TableHead>
              <TableHead className={`${head} text-right`}>Created</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sets.map((s) => (
              <TableRow
                key={s.id}
                className="cursor-pointer"
                onClick={(ev) => {
                  if (!(ev.target as HTMLElement).closest("a, button")) navigate(`/prompts/${s.id}`)
                }}
              >
                <TableCell className="max-w-0 min-w-56 py-3 whitespace-normal">
                  <Link to={`/prompts/${s.id}`} title={s.name} className="line-clamp-2 rounded-sm font-medium break-words underline-offset-4 hover:underline">
                    {s.name}
                  </Link>
                </TableCell>
                <TableCell className="max-w-0 min-w-48 py-3 text-muted-foreground">
                  <span title={describeSource(s)} className="block truncate">
                    {describeSource(s)}
                  </span>
                </TableCell>
                <TableCell className="py-3 text-right tabular">{int(s.prompt_count)}</TableCell>
                <TableCell className="py-3 text-right tabular">{int(s.run_count)}</TableCell>
                <TableCell className="py-3 text-right whitespace-nowrap text-muted-foreground">{relative(s.created_at)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      <ul className="divide-y border-y md:hidden">
        {sets.map((s) => (
          <li key={s.id}>
            <Link to={`/prompts/${s.id}`} className="grid gap-1 py-3.5">
              <span className="line-clamp-2 font-medium break-words">{s.name}</span>
              <span className="truncate text-xs text-muted-foreground">{describeSource(s)}</span>
              <span className="text-xs text-muted-foreground">
                <span className="tabular">{count(s.prompt_count, "prompt", "prompts")}</span> · <span className="tabular">{count(s.run_count, "run", "runs")}</span> ·{" "}
                {relative(s.created_at)}
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </>
  )
}

export function PromptSetTableSkeleton() {
  return (
    <div aria-busy="true" aria-label="Loading prompt sets" className="divide-y border-y">
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="grid gap-1.5 py-3.5 md:grid-cols-[38%_1fr_5rem_4rem_6rem] md:items-center md:gap-4">
          <Bar className="h-5 w-48 max-w-full" />
          <Bar className="h-4 w-52 max-w-full" />
          <Bar className="h-4 w-12 md:justify-self-end" />
          <Bar className="h-4 w-8 md:justify-self-end" />
          <Bar className="h-4 w-16 md:justify-self-end" />
        </div>
      ))}
    </div>
  )
}
