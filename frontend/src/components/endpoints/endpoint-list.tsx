import { MoreHorizontal, Pencil, RefreshCw, Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Bar } from "@/components/shared/block-states"
import { Plasma } from "@/components/shared/plasma"
import { count, DASH, int, middle } from "@/lib/format"
import type { Endpoint } from "@/lib/types"
import { BaseUrl } from "./base-url"
import { CheckedAt, EndpointStatus } from "./endpoint-status"
import { useCheckEndpoint } from "./use-check-endpoint"

type Handlers = {
  onOpen: (e: Endpoint) => void
  onEdit: (e: Endpoint) => void
  onDelete: (e: Endpoint) => void
}

/** The seed of an endpoint's pixel-field tile, so the same endpoint always gets the same tile. */
export const endpointSeed = (e: Pick<Endpoint, "id">) => `endpoint-${e.id}`

/** The whole row opens the details; a click on a control inside it does its own thing. */
const openOnRowClick = (open: () => void) => (ev: React.MouseEvent) => {
  if ((ev.target as HTMLElement).closest("button, a, input, [role=menuitem]")) return
  open()
}

/**
 * The models an endpoint serves, in mono: the one model id when there is one (a vLLM server), else
 * the first id and how many more. All of them are in the details sheet.
 */
function ModelsText({ endpoint }: { endpoint: Endpoint }) {
  const models = endpoint.models
  if (!models.length) return <span className="text-muted-foreground">{DASH}</span>
  return (
    <span className="flex min-w-0 items-baseline gap-1.5" title={models.slice(0, 20).join("\n") + (models.length > 20 ? "\n…" : "")}>
      <span className="min-w-0 truncate font-mono text-xs">{models[0]}</span>
      {models.length > 1 ? <span className="flex-none text-xs text-muted-foreground tabular">+{int(models.length - 1)}</span> : null}
    </span>
  )
}

function NameButton({ endpoint, onOpen }: { endpoint: Endpoint; onOpen: (e: Endpoint) => void }) {
  return (
    <button
      type="button"
      onClick={() => onOpen(endpoint)}
      title={endpoint.name}
      className="max-w-full justify-self-start rounded-xs text-left font-medium focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none"
    >
      <span className="line-clamp-2 break-words">{endpoint.name}</span>
    </button>
  )
}

function RowActions({ endpoint, onEdit, onDelete }: Omit<Handlers, "onOpen"> & { endpoint: Endpoint }) {
  const check = useCheckEndpoint(endpoint)
  return (
    <div className="flex items-center justify-end gap-1">
      <Button variant="ghost" size="sm" className="max-lg:hidden" onClick={() => onEdit(endpoint)} aria-label={`Edit ${endpoint.name}`}>
        Edit
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={`More actions for ${endpoint.name}`}>
            <MoreHorizontal aria-hidden="true" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-44">
          <DropdownMenuItem className="lg:hidden" onSelect={() => onEdit(endpoint)}>
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

/**
 * Endpoints (DESIGN.md, Tables). From 1024 px a table: the pixel-field tile, the name in ink and the
 * base URL in mono under it; the status as a dot and a word; the models in mono from 1280 px. Below
 * 1024 px one stacked row per endpoint with its facts on a second line.
 */
export function EndpointList({ endpoints, ...h }: { endpoints: Endpoint[] } & Handlers) {
  return (
    <>
      <div className="hidden lg:block">
        <Table className="table-fixed">
          <TableHeader>
            <TableRow className="hover:[&>td]:bg-transparent">
              <TableHead>Endpoint</TableHead>
              <TableHead className="w-40">Status</TableHead>
              <TableHead className="w-44 max-xl:hidden">Models</TableHead>
              <TableHead className="w-28">Checked</TableHead>
              <TableHead numeric className="w-16">
                Seqs
              </TableHead>
              <TableHead numeric className="w-16">
                Runs
              </TableHead>
              <TableHead className="w-24">
                <span className="sr-only">Actions</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {endpoints.map((e) => (
              <TableRow key={e.id} className="cursor-pointer" onClick={openOnRowClick(() => h.onOpen(e))}>
                <TableCell className="py-2 whitespace-normal">
                  <div className="flex min-w-0 items-center gap-3">
                    <Plasma seed={endpointSeed(e)} />
                    <div className="grid min-w-0 flex-1 gap-0.5">
                      <NameButton endpoint={e} onOpen={h.onOpen} />
                      <BaseUrl url={e.base_url} />
                    </div>
                  </div>
                </TableCell>
                <TableCell className="py-2 whitespace-normal">
                  <EndpointStatus endpoint={e} />
                </TableCell>
                <TableCell className="max-xl:hidden">
                  <ModelsText endpoint={e} />
                </TableCell>
                <TableCell className="text-muted-foreground">
                  <CheckedAt endpoint={e} />
                </TableCell>
                <TableCell numeric>{int(e.default_concurrency)}</TableCell>
                <TableCell numeric>{int(e.run_count)}</TableCell>
                <TableCell className="pr-1.5">
                  <RowActions endpoint={e} onEdit={h.onEdit} onDelete={h.onDelete} />
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {/* Phone and tablet (as Runs): one tappable row opens the details; the menu holds the other
          actions. The table needs about 700 px and would run off a 768 to 1023 px content column. */}
      <ul className="lg:hidden">
        {endpoints.map((e) => (
          <li key={e.id} className="flex items-center gap-1 border-b last:border-b-0">
            <button
              type="button"
              onClick={() => h.onOpen(e)}
              className="-mx-2 flex min-h-14 min-w-0 flex-1 items-center gap-3 rounded-md px-2 py-2.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring active:bg-accent"
            >
              <Plasma seed={endpointSeed(e)} />
              <span className="grid min-w-0 flex-1 gap-0.5">
                <span className="line-clamp-2 font-medium break-words">{e.name}</span>
                <span className="truncate font-mono text-xs text-muted-foreground">{middle(e.base_url, 40)}</span>
                <span className="text-xs text-muted-foreground">
                  <EndpointStatus endpoint={e} detail={false} className="text-xs" />
                  {e.models.length ? <> · <span className="tabular">{count(e.models.length, "model", "models")}</span></> : null} ·{" "}
                  <span className="tabular">{int(e.default_concurrency)}</span> seqs · <span className="tabular">{count(e.run_count, "run", "runs")}</span>
                </span>
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
    <div aria-busy="true" aria-label="Loading endpoints">
      <div className="hidden h-8 rounded-md bg-muted lg:block" />
      {[0, 1, 2].map((i) => (
        <div
          key={i}
          className="flex items-center gap-3 border-b py-2.5 last:border-b-0 lg:h-14 lg:px-3 lg:py-0"
        >
          <Bar className="size-7 rounded-sm" />
          <div className="grid flex-1 gap-1.5">
            <Bar className="h-4 w-40" />
            <Bar className="h-3 w-56 max-w-full" />
          </div>
          <Bar className="hidden h-4 w-20 lg:inline-block" />
          <Bar className="hidden h-4 w-16 lg:inline-block" />
        </div>
      ))}
    </div>
  )
}
