import { useState } from "react"
import { Link } from "react-router"
import { Loader2, Search } from "lucide-react"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { CopyButton } from "@/components/shared/copy-button"
import { BaseUrl } from "./base-url"
import { CheckedAt, EndpointStatus } from "./endpoint-status"
import { useCheckEndpoint } from "./use-check-endpoint"
import { int } from "@/lib/format"
import { cn } from "@/lib/utils"
import type { Endpoint } from "@/lib/types"

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
      <SheetContent className="w-full gap-0 sm:max-w-md">{endpoint ? <Details endpoint={endpoint} onEdit={onEdit} /> : null}</SheetContent>
    </Sheet>
  )
}

function Details({ endpoint, onEdit }: { endpoint: Endpoint; onEdit: (e: Endpoint) => void }) {
  const check = useCheckEndpoint(endpoint)
  const [q, setQ] = useState("")
  const needle = q.trim().toLowerCase()
  const models = needle ? endpoint.models.filter((m) => m.toLowerCase().includes(needle)) : endpoint.models
  const searchable = endpoint.models.length > 20
  return (
    <>
      <SheetHeader className="gap-1 pr-12">
        <SheetTitle className="text-base break-words">{endpoint.name}</SheetTitle>
        <SheetDescription className="sr-only">Connection details and the models this endpoint serves.</SheetDescription>
        <BaseUrl url={endpoint.base_url} />
      </SheetHeader>
      <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 border-b px-4 pb-4 text-sm">
        <dt className="text-muted-foreground">Status</dt>
        <dd>
          <EndpointStatus endpoint={endpoint} />
        </dd>
        <dt className="text-muted-foreground">Checked</dt>
        <dd>
          <CheckedAt endpoint={endpoint} />
        </dd>
        <dt className="text-muted-foreground">Default seqs</dt>
        <dd className="tabular">{int(endpoint.default_concurrency)}</dd>
        <dt className="text-muted-foreground">Runs</dt>
        <dd className="tabular">{int(endpoint.run_count)}</dd>
      </dl>

      <div className="flex min-h-0 flex-1 flex-col gap-3 px-4 pt-4">
        <h3 className="text-base font-semibold">Models</h3>
        {endpoint.models.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {endpoint.last_ok === false
              ? "The last check failed."
              : "No models listed."}
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
              <p className="text-xs text-muted-foreground tabular" aria-live="polite">
                {int(models.length)} of {int(endpoint.models.length)}
              </p>
            ) : null}
            <ul className="-mx-1 min-h-0 flex-1 overflow-y-auto pb-4">
              {models.map((m) => (
                <li key={m} className="flex items-center justify-between gap-1 rounded-md px-1 hover:bg-muted">
                  <span title={m} className="min-w-0 py-1 font-mono text-[13px] break-all">
                    {m}
                  </span>
                  <CopyButton value={m} label={`Copy ${m}`} />
                </li>
              ))}
              {models.length === 0 ? <li className="px-1 py-2 text-sm text-muted-foreground">No model matches “{q}”.</li> : null}
            </ul>
          </>
        )}
      </div>

      <div className="flex flex-wrap gap-2 border-t p-4">
        <Link
          to={`/runs/new?endpoint=${endpoint.id}`}
          className={cn(buttonVariants({ variant: endpoint.last_ok === false ? "outline" : "default" }), "pointer-coarse:min-h-11")}
        >
          New run
        </Link>
        <Button variant="outline" onClick={() => onEdit(endpoint)}>
          Edit
        </Button>
        <Button variant={endpoint.last_ok === false ? "default" : "outline"} disabled={check.isPending} onClick={() => check.mutate()}>
          {check.isPending ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
          {check.isPending ? "Checking…" : "Check now"}
        </Button>
      </div>
    </>
  )
}
