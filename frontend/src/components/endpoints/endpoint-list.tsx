import { MoreHorizontal, RefreshCw, Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Bar } from "@/components/shared/block-states"
import { int } from "@/lib/format"
import type { Endpoint } from "@/lib/types"
import { BaseUrl } from "./base-url"
import { CheckedAt, EndpointStatus } from "./endpoint-status"
import { useCheckEndpoint } from "./use-check-endpoint"

type Handlers = {
  onOpen: (e: Endpoint) => void
  onEdit: (e: Endpoint) => void
  onDelete: (e: Endpoint) => void
}

/** The whole row opens the details; a click on a control inside it does its own thing. */
const openOnRowClick = (open: () => void) => (ev: React.MouseEvent) => {
  if ((ev.target as HTMLElement).closest("button, a, input, [role=menuitem]")) return
  open()
}

function NameButton({ endpoint, onOpen }: { endpoint: Endpoint; onOpen: (e: Endpoint) => void }) {
  return (
    <button
      type="button"
      onClick={() => onOpen(endpoint)}
      title={endpoint.name}
      className="max-w-full rounded-sm text-left font-medium pointer-coarse:min-h-11 underline-offset-4 hover:underline focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
    >
      <span className="line-clamp-2 break-words">{endpoint.name}</span>
    </button>
  )
}

function RowActions({ endpoint, onEdit, onDelete }: Omit<Handlers, "onOpen"> & { endpoint: Endpoint }) {
  const check = useCheckEndpoint(endpoint)
  return (
    <div className="flex items-center justify-end gap-1">
      <Button variant="ghost" size="sm" onClick={() => onEdit(endpoint)} aria-label={`Edit ${endpoint.name}`}>
        Edit
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={`More actions for ${endpoint.name}`}>
            <MoreHorizontal aria-hidden="true" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-40">
          <DropdownMenuItem disabled={check.isPending} onSelect={() => check.mutate()}>
            <RefreshCw aria-hidden="true" />
            {check.isPending ? "Checking…" : "Check now"}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive" onSelect={() => onDelete(endpoint)}>
            <Trash2 aria-hidden="true" />
            Delete…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}

const head = "text-xs font-normal text-muted-foreground"

export function EndpointList({ endpoints, ...h }: { endpoints: Endpoint[] } & Handlers) {
  return (
    <>
      <div className="hidden md:block">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className={`${head} w-[40%]`}>Endpoint</TableHead>
              <TableHead className={head}>Status</TableHead>
              <TableHead className={head}>Checked</TableHead>
              <TableHead className={`${head} text-right`}>Default seqs</TableHead>
              <TableHead className={`${head} text-right`}>Runs</TableHead>
              <TableHead className={`${head} text-right`}>
                <span className="sr-only">Actions</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {endpoints.map((e) => (
              <TableRow key={e.id} className="cursor-pointer" onClick={openOnRowClick(() => h.onOpen(e))}>
                <TableCell className="max-w-0 min-w-56 py-3 align-top whitespace-normal">
                  <div className="grid gap-0.5">
                    <NameButton endpoint={e} onOpen={h.onOpen} />
                    <BaseUrl url={e.base_url} />
                  </div>
                </TableCell>
                <TableCell className="py-3 align-top whitespace-normal">
                  <EndpointStatus endpoint={e} />
                </TableCell>
                <TableCell className="py-3 align-top whitespace-nowrap text-muted-foreground">
                  <CheckedAt endpoint={e} />
                </TableCell>
                <TableCell className="py-3 text-right align-top tabular">{int(e.default_concurrency)}</TableCell>
                <TableCell className="py-3 text-right align-top tabular">{int(e.run_count)}</TableCell>
                <TableCell className="py-2.5 align-top">
                  <RowActions endpoint={e} onEdit={h.onEdit} onDelete={h.onDelete} />
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      <ul className="divide-y border-y md:hidden">
        {endpoints.map((e) => (
          <li key={e.id} className="grid gap-1.5 py-3" onClick={openOnRowClick(() => h.onOpen(e))}>
            <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-2">
              <div className="grid min-w-0 gap-0.5">
                <NameButton endpoint={e} onOpen={h.onOpen} />
                <BaseUrl url={e.base_url} />
              </div>
              <RowActions endpoint={e} onEdit={h.onEdit} onDelete={h.onDelete} />
            </div>
            <div className="text-sm">
              <EndpointStatus endpoint={e} />
            </div>
            <p className="text-xs text-muted-foreground">
              Checked <CheckedAt endpoint={e} /> · <span className="tabular">{int(e.default_concurrency)}</span> seqs ·{" "}
              <span className="tabular">{int(e.run_count)}</span> {e.run_count === 1 ? "run" : "runs"}
            </p>
          </li>
        ))}
      </ul>
    </>
  )
}

/** Loading rows at the real pitch and column positions. */
export function EndpointListSkeleton() {
  return (
    <div aria-busy="true" aria-label="Loading endpoints" className="divide-y border-y">
      {[0, 1, 2].map((i) => (
        <div key={i} className="grid gap-2 py-3 md:grid-cols-[40%_1fr_1fr_6rem_4rem_8rem] md:items-start md:gap-4">
          <div className="grid gap-1.5">
            <Bar className="h-5 w-40" />
            <Bar className="h-4 w-56 max-w-full" />
          </div>
          <Bar className="h-5 w-20" />
          <Bar className="h-5 w-20" />
          <Bar className="h-5 w-8 justify-self-end" />
          <Bar className="h-5 w-6 justify-self-end" />
          <Bar className="h-7 w-20 justify-self-end" />
        </div>
      ))}
    </div>
  )
}
