import { MoreHorizontal, Pencil, RefreshCw, Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Bar } from "@/components/shared/block-states"
import { int, middle } from "@/lib/format"
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
      className="max-w-full rounded-sm text-left font-medium focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
    >
      <span className="line-clamp-2 break-words">{endpoint.name}</span>
    </button>
  )
}

function RowActions({ endpoint, onEdit, onDelete }: Omit<Handlers, "onOpen"> & { endpoint: Endpoint }) {
  const check = useCheckEndpoint(endpoint)
  return (
    <div className="flex items-center justify-end gap-1">
      <Button variant="ghost" size="sm" className="max-md:hidden" onClick={() => onEdit(endpoint)} aria-label={`Edit ${endpoint.name}`}>
        Edit
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={`More actions for ${endpoint.name}`}>
            <MoreHorizontal aria-hidden="true" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-40">
          <DropdownMenuItem className="md:hidden" onSelect={() => onEdit(endpoint)}>
            <Pencil aria-hidden="true" />
            Edit…
          </DropdownMenuItem>
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

export function EndpointList({ endpoints, ...h }: { endpoints: Endpoint[] } & Handlers) {
  return (
    <>
      <div className="hidden lg:block">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-[40%]">Endpoint</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Checked</TableHead>
              <TableHead numeric>Default seqs</TableHead>
              <TableHead numeric>Runs</TableHead>
              <TableHead>
                <span className="sr-only">Actions</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {endpoints.map((e) => (
              <TableRow key={e.id} className="cursor-pointer" onClick={openOnRowClick(() => h.onOpen(e))}>
                <TableCell className="max-w-0 min-w-56 py-1.5 whitespace-normal">
                  <div className="grid gap-0.5">
                    <NameButton endpoint={e} onOpen={h.onOpen} />
                    <BaseUrl url={e.base_url} />
                  </div>
                </TableCell>
                <TableCell className="whitespace-normal">
                  <EndpointStatus endpoint={e} />
                </TableCell>
                <TableCell className="text-muted-foreground">
                  <CheckedAt endpoint={e} />
                </TableCell>
                <TableCell numeric>{int(e.default_concurrency)}</TableCell>
                <TableCell numeric>{int(e.run_count)}</TableCell>
                <TableCell>
                  <RowActions endpoint={e} onEdit={h.onEdit} onDelete={h.onDelete} />
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {/* Phone and tablet (as Runs): one tappable row opens the details; the menu holds the other
          actions. The table needs about 700 px and would run off a 768 to 1023 px content column. */}
      <ul className="divide-y lg:hidden">
        {endpoints.map((e) => (
          <li key={e.id} className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => h.onOpen(e)}
              className="-mx-2 grid min-w-0 flex-1 gap-0.5 rounded-lg px-2 py-2.5 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring active:bg-accent"
            >
              <span className="line-clamp-2 font-medium break-words">{e.name}</span>
              <span className="truncate font-mono text-[13px] text-muted-foreground">{middle(e.base_url, 34)}</span>
              <span className="text-xs text-muted-foreground">
                <EndpointStatus endpoint={e} detail={false} /> · Checked <CheckedAt endpoint={e} /> · <span className="tabular">{int(e.default_concurrency)}</span> seqs ·{" "}
                <span className="tabular">{int(e.run_count)}</span> {e.run_count === 1 ? "run" : "runs"}
              </span>
            </button>
            <RowActions endpoint={e} onEdit={h.onEdit} onDelete={h.onDelete} />
          </li>
        ))}
      </ul>
    </>
  )
}

/** Loading rows at the real pitch and column positions. */
export function EndpointListSkeleton() {
  return (
    <div aria-busy="true" aria-label="Loading endpoints" className="divide-y">
      {[0, 1, 2].map((i) => (
        <div key={i} className="grid gap-2 py-2.5 lg:grid-cols-[40%_1fr_1fr_6rem_4rem_8rem] lg:items-start lg:gap-4">
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
