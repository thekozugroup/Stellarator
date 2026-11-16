import { Pencil, Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Bar } from "@/components/shared/block-states"
import { int } from "@/lib/format"
import type { Endpoint } from "@/lib/types"
import { BaseUrl } from "./base-url"
import { CheckButton } from "./check-button"
import { CheckedAt, EndpointStatus, locationLabel } from "./endpoint-status"

type Handlers = {
  onOpen: (e: Endpoint) => void
  onEdit: (e: Endpoint) => void
  onDelete: (e: Endpoint) => void
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
  return (
    <div className="flex items-center justify-end gap-1">
      <CheckButton endpoint={endpoint} />
      <Button variant="ghost" size="sm" onClick={() => onEdit(endpoint)} aria-label={`Edit ${endpoint.name}`}>
        <Pencil aria-hidden="true" />
        Edit
      </Button>
      <Button variant="ghost" size="icon-sm" onClick={() => onDelete(endpoint)} aria-label={`Delete ${endpoint.name}`} title="Delete">
        <Trash2 aria-hidden="true" />
      </Button>
    </div>
  )
}

export function EndpointList({ endpoints, ...h }: { endpoints: Endpoint[] } & Handlers) {
  return (
    <>
      <div className="hidden md:block">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-[36%] text-xs font-normal text-muted-foreground">Endpoint</TableHead>
              <TableHead className="text-xs font-normal text-muted-foreground">Where</TableHead>
              <TableHead className="text-xs font-normal text-muted-foreground">Status</TableHead>
              <TableHead className="text-xs font-normal text-muted-foreground">Checked</TableHead>
              <TableHead className="text-right text-xs font-normal text-muted-foreground">Default seqs</TableHead>
              <TableHead className="text-right text-xs font-normal text-muted-foreground">Runs</TableHead>
              <TableHead className="text-right text-xs font-normal text-muted-foreground">
                <span className="sr-only">Actions</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {endpoints.map((e) => (
              <TableRow key={e.id}>
                <TableCell className="max-w-0 min-w-56 py-3 align-top whitespace-normal">
                  <div className="grid gap-0.5">
                    <NameButton endpoint={e} onOpen={h.onOpen} />
                    <BaseUrl url={e.base_url} />
                  </div>
                </TableCell>
                <TableCell className="py-3 align-top">{locationLabel(e)}</TableCell>
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
          <li key={e.id} className="grid gap-2 py-4">
            <div className="grid gap-0.5">
              <NameButton endpoint={e} onOpen={h.onOpen} />
              <BaseUrl url={e.base_url} />
            </div>
            <div className="text-sm">
              <EndpointStatus endpoint={e} />
            </div>
            <p className="text-xs text-muted-foreground">
              {locationLabel(e)} · checked <CheckedAt endpoint={e} /> · <span className="tabular">{int(e.default_concurrency)}</span> seqs ·{" "}
              <span className="tabular">{int(e.run_count)}</span> {e.run_count === 1 ? "run" : "runs"}
            </p>
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
    <div aria-busy="true" aria-label="Loading endpoints" className="divide-y border-y">
      {[0, 1, 2].map((i) => (
        <div key={i} className="grid gap-2 py-3 md:grid-cols-[30%_1fr_1fr_1fr_6rem_4rem_14rem] md:items-start md:gap-4">
          <div className="grid gap-1.5">
            <Bar className="h-5 w-40" />
            <Bar className="h-4 w-56 max-w-full" />
          </div>
          <Bar className="h-5 w-12" />
          <Bar className="h-5 w-32" />
          <Bar className="h-5 w-20" />
          <Bar className="h-5 w-8 justify-self-end" />
          <Bar className="h-5 w-6 justify-self-end" />
          <Bar className="h-7 w-48 justify-self-end" />
        </div>
      ))}
    </div>
  )
}
