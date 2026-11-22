import { useState } from "react"
import { Link } from "react-router"
import { Loader2, Search } from "lucide-react"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { CopyButton } from "@/components/shared/copy-button"
import { Plasma } from "@/components/shared/plasma"
import { Tile, Tiles } from "@/components/shared/tiles"
import { BaseUrl } from "./base-url"
import { CheckedAt, EndpointStatusBadge, endpointState } from "./endpoint-status"
import { endpointSeed } from "./endpoint-list"
import { useCheckEndpoint } from "./use-check-endpoint"
import { int } from "@/lib/format"
import { cn } from "@/lib/utils"
import type { Endpoint } from "@/lib/types"

/**
 * Inside the thick material the page's --muted is the same tone as the dialog (244 on 244 in light),
 * so groups and wells inside it take the ground in light and the raised tone in dark, as grouped
 * sections do on iOS sheets.
 */
const ON_MATERIAL = "[--muted:var(--background)] dark:[--muted:var(--secondary)]"

/** Everything about one endpoint, including the model ids it serves. */
export function EndpointDetailsSheet({
  endpoint,
  onClose,
  onEdit,
}: {
  endpoint: Endpoint | null
  onClose: () => void
  onEdit: (e: Endpoint) => void
}) {
  return (
    <Sheet open={endpoint !== null} onOpenChange={(o) => !o && onClose()}>
      <SheetContent className={cn("gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-md", ON_MATERIAL)}>{endpoint ? <Details endpoint={endpoint} onEdit={onEdit} /> : null}</SheetContent>
    </Sheet>
  )
}

/**
 * The sheet follows a detail screen's header (DESIGN.md, Layout): the tile and name, then the status
 * and when it was checked, then the facts as tiles; the models below in a muted box.
 */
function Details({ endpoint, onEdit }: { endpoint: Endpoint; onEdit: (e: Endpoint) => void }) {
  const check = useCheckEndpoint(endpoint)
  const [q, setQ] = useState("")
  const needle = q.trim().toLowerCase()
  const models = needle ? endpoint.models.filter((m) => m.toLowerCase().includes(needle)) : endpoint.models
  const searchable = endpoint.models.length > 20
  const failing = endpoint.last_ok === false
  const { reason } = endpointState(endpoint)
  return (
    <>
      <SheetHeader className="gap-0 px-5 pt-5 pr-14 pb-4">
        <div className="flex min-w-0 items-center gap-3">
          <Plasma seed={endpointSeed(endpoint)} cells={8} className="size-10 rounded-md" />
          <div className="grid min-w-0 flex-1 gap-0.5">
            <SheetTitle className="text-base leading-5 font-semibold break-words">{endpoint.name}</SheetTitle>
            <BaseUrl url={endpoint.base_url} />
          </div>
        </div>
        <SheetDescription className="sr-only">Connection details and the models this endpoint serves.</SheetDescription>
      </SheetHeader>

      <div className="grid gap-4 px-5">
        <div className="grid gap-2">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <EndpointStatusBadge endpoint={endpoint} />
            <span className="text-sm text-muted-foreground">
              Checked <CheckedAt endpoint={endpoint} />
              {endpoint.has_api_key ? (
                <>
                  {" · "}Key <span className="font-mono text-xs">{endpoint.api_key_hint ?? "set"}</span>
                </>
              ) : null}
            </span>
          </div>
          {reason ? (
            <p title={endpoint.last_error ?? undefined} className="text-sm text-muted-foreground">
              {reason}
            </p>
          ) : null}
        </div>
        <Tiles className="grid-cols-3 lg:grid-cols-3">
          <Tile label={endpoint.models.length === 1 ? "Model" : "Models"} value={int(endpoint.models.length)} />
          <Tile label="Default seqs" value={int(endpoint.default_concurrency)} />
          <Tile label={endpoint.run_count === 1 ? "Run" : "Runs"} value={int(endpoint.run_count)} />
        </Tiles>
      </div>

      <section aria-labelledby="endpoint-models" className="flex min-h-0 flex-1 flex-col gap-2 px-5 pt-6">
        <h3 id="endpoint-models" className="px-1 text-sm font-semibold">
          Models
        </h3>
        {endpoint.models.length === 0 ? (
          <p className="rounded-xl bg-muted px-4 py-3 text-sm text-muted-foreground">
            {failing ? "The last check failed, so the model list is empty." : "The server lists no models."}
          </p>
        ) : (
          <>
            {searchable ? (
              <div className="relative">
                <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
                <Input
                  type="search"
                  aria-label="Search models"
                  placeholder="Search models…"
                  autoComplete="off"
                  value={q}
                  onChange={(e) => setQ(e.target.value)}
                  className="pl-8"
                />
              </div>
            ) : null}
            {needle ? (
              <p className="px-1 text-xs text-muted-foreground tabular" aria-live="polite">
                {int(models.length)} of {int(endpoint.models.length)}
              </p>
            ) : null}
            {/* A muted box with 8 px padding holds rows with 8 px corners (16 - 8), concentric. */}
            <ul className="min-h-0 overflow-y-auto overscroll-contain rounded-xl bg-muted p-2">
              {models.map((m) => (
                <li key={m} className="flex min-h-8 items-center justify-between gap-1 rounded-sm pl-2 hover:bg-accent">
                  <span title={m} className="min-w-0 py-1 font-mono text-xs break-all">
                    {m}
                  </span>
                  <CopyButton value={m} label={`Copy ${m}`} />
                </li>
              ))}
              {models.length === 0 ? <li className="px-2 py-1.5 text-sm text-muted-foreground">No model matches “{q}”.</li> : null}
            </ul>
          </>
        )}
      </section>

      <div className="flex flex-wrap justify-end gap-2 px-5 pt-4 pb-5">
        <Button variant="secondary" onClick={() => onEdit(endpoint)}>
          Edit
        </Button>
        <Button variant={failing ? "default" : "secondary"} disabled={check.isPending} onClick={() => check.mutate()}>
          {check.isPending ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
          {check.isPending ? "Checking…" : "Check now"}
        </Button>
        <Link
          to={`/runs/new?endpoint=${endpoint.id}`}
          className={cn(buttonVariants({ variant: failing ? "secondary" : "default" }), "pointer-coarse:min-h-11")}
        >
          New run
        </Link>
      </div>
    </>
  )
}
