import { useRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { ChevronRight, Eye, EyeOff, Loader2, Plus, X } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Field } from "@/components/shared/field"
import { ApiError, api } from "@/lib/api"
import { count, duration } from "@/lib/format"
import type { Endpoint, EndpointIn, EndpointTestOut } from "@/lib/types"
import { isKeyRejected } from "./endpoint-status"

const DEFAULT_SEQS = 16
const FIELD_ORDER = ["name", "base_url", "api_key", "default_concurrency", "headers"]

type HeaderRow = { id: number; key: string; value: string }

/** Add or edit one endpoint. `endpoint` set means edit. Values survive a failed save. */
export function EndpointFormDialog({
  open,
  endpoint,
  onOpenChange,
  focusField = null,
}: {
  open: boolean
  endpoint: Endpoint | null
  onOpenChange: (open: boolean) => void
  /** Open with this field focused: "api_key" after a rejected key, "base_url" when unreachable. */
  focusField?: "api_key" | "base_url" | null
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-lg"
        onOpenAutoFocus={(ev) => {
          if (!focusField) return
          ev.preventDefault()
          document.getElementById(`ep-${focusField}`)?.focus()
        }}
      >
        <EndpointForm endpoint={endpoint} onDone={() => onOpenChange(false)} onCancel={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  )
}

function EndpointForm({ endpoint, onDone, onCancel }: { endpoint: Endpoint | null; onDone: () => void; onCancel: () => void }) {
  const qc = useQueryClient()
  const editing = endpoint !== null
  const nextId = useRef(1)
  const [name, setName] = useState(endpoint?.name ?? "")
  const [baseUrl, setBaseUrl] = useState(endpoint?.base_url ?? "")
  const [apiKey, setApiKey] = useState("")
  const [showKey, setShowKey] = useState(false)
  const [removeKey, setRemoveKey] = useState(false)
  const [seqs, setSeqs] = useState(String(endpoint?.default_concurrency ?? DEFAULT_SEQS))
  const [headers, setHeaders] = useState<HeaderRow[]>(() =>
    Object.entries(endpoint?.headers ?? {}).map(([key, value]) => ({ id: nextId.current++, key, value })),
  )
  const [headersOpen, setHeadersOpen] = useState(headers.length > 0)
  const [errors, setErrors] = useState<Record<string, string>>({})
  const [formError, setFormError] = useState<string | null>(null)
  const [test, setTest] = useState<EndpointTestOut | null>(null)
  const [testError, setTestError] = useState<string | null>(null)

  const headerObject = () => {
    const out: Record<string, string> = {}
    for (const h of headers) if (h.key.trim()) out[h.key.trim()] = h.value
    return out
  }
  const staleTest = () => {
    setTest(null)
    setTestError(null)
  }

  const testMutation = useMutation({
    mutationFn: () =>
      api.post<EndpointTestOut>("/api/endpoints/test", {
        base_url: baseUrl.trim(),
        api_key: apiKey || null,
        headers: headerObject(),
        endpoint_id: endpoint && !removeKey ? endpoint.id : null,
      }),
    onSuccess: (r) => {
      setTest(r)
      setTestError(null)
    },
    onError: (e: Error) => {
      setTest(null)
      setTestError(e.message)
    },
  })

  const save = useMutation({
    mutationFn: (body: Partial<EndpointIn>) =>
      endpoint ? api.patch<Endpoint>(`/api/endpoints/${endpoint.id}`, body) : api.post<Endpoint>("/api/endpoints", body),
    onSuccess: (ep) => {
      qc.invalidateQueries({ queryKey: ["endpoints"] })
      toast.success(
        ep.last_ok
          ? `Saved ${ep.name}. It serves ${count(ep.models.length, "model", "models")}.`
          : `Saved ${ep.name}, but it is unreachable right now.`,
      )
      onDone()
    },
    onError: (e: Error) => {
      if (e instanceof ApiError && Object.keys(e.fields).length) {
        const mapped: Record<string, string> = {}
        for (const [path, msg] of Object.entries(e.fields)) mapped[path.split(".")[0]] = msg
        showErrors(mapped)
      } else {
        setFormError(e.message)
      }
    },
  })

  function showErrors(next: Record<string, string>) {
    setErrors(next)
    const first = FIELD_ORDER.find((f) => next[f])
    if (first) window.requestAnimationFrame(() => document.getElementById(`ep-${first}`)?.focus())
  }

  function submit(ev: React.FormEvent) {
    ev.preventDefault()
    if (save.isPending) return
    setFormError(null)
    const next: Record<string, string> = {}
    if (!name.trim()) next.name = "Enter a name so you can tell this endpoint apart."
    if (!baseUrl.trim()) next.base_url = "Enter the base URL of the server, for example http://192.168.1.50:8000/v1."
    else if (!/^https?:\/\//i.test(baseUrl.trim())) next.base_url = "Base URL must start with http:// or https://."
    const n = Number(seqs)
    if (!Number.isInteger(n) || n < 1 || n > 4096) next.default_concurrency = "Enter a whole number of seqs from 1 to 4096."
    setErrors(next)
    if (Object.keys(next).length) return showErrors(next)
    const body: Partial<EndpointIn> = {
      name: name.trim(),
      base_url: baseUrl.trim(),
      default_concurrency: n,
      headers: headerObject(),
    }
    if (!editing) body.location = "local"
    if (apiKey) body.api_key = apiKey
    else if (removeKey) body.api_key = ""
    save.mutate(body)
  }

  const clear = (field: string) => setErrors((e) => (e[field] ? { ...e, [field]: "" } : e))
  const describe = (field: string, hint: boolean) => (errors[field] ? `ep-${field}-error` : hint ? `ep-${field}-hint` : undefined)

  return (
    <form onSubmit={submit} noValidate className="flex min-h-0 flex-1 flex-col">
      <DialogHeader className="px-4 pt-4 pr-12 pb-3">
        <DialogTitle>{editing ? "Edit endpoint" : "Add endpoint"}</DialogTitle>
      </DialogHeader>
      <div className="grid min-h-0 flex-1 content-start gap-4 overflow-y-auto px-4 pt-1 pb-4">

      <Field id="ep-name" label="Name" error={errors.name}>
        <Input
          id="ep-name"
          value={name}
          placeholder="DGX Spark"
          autoComplete="off"
          aria-invalid={!!errors.name}
          aria-describedby={describe("name", false)}
          onChange={(e) => {
            setName(e.target.value)
            clear("name")
          }}
        />
      </Field>

      <Field
        id="ep-base_url"
        label="Base URL"
        error={errors.base_url}
        hint={/^https?:\/\/(localhost|127\.0\.0\.1)\b/i.test(baseUrl.trim()) ? "From Docker, this machine is http://host.docker.internal:PORT/v1." : undefined}
      >
        <Input
          id="ep-base_url"
          type="url"
          inputMode="url"
          value={baseUrl}
          placeholder="http://192.168.1.50:8000/v1"
          autoComplete="off"
          spellCheck={false}
          autoCapitalize="none"
          className="font-mono pointer-fine:text-[13px]"
          aria-invalid={!!errors.base_url}
          aria-describedby={describe("base_url", true)}
          onChange={(e) => {
            setBaseUrl(e.target.value)
            clear("base_url")
            staleTest()
          }}
        />
      </Field>

      <div className="grid gap-1.5">
        <Label htmlFor="ep-api_key" className="text-sm font-medium">
          API key<span className="font-normal text-muted-foreground"> (optional)</span>
        </Label>
        <div className="relative">
          <Input
            id="ep-api_key"
            type={showKey ? "text" : "password"}
            value={apiKey}
            disabled={removeKey}
            placeholder={editing && endpoint?.has_api_key ? undefined : "sk-012345…"}
            autoComplete="off"
            spellCheck={false}
            autoCapitalize="none"
            className="pr-10 font-mono pointer-fine:text-[13px]"
            aria-invalid={!!errors.api_key}
            aria-describedby={errors.api_key ? "ep-api_key-error" : editing && endpoint?.has_api_key ? "ep-api_key-hint" : undefined}
            onChange={(e) => {
              setApiKey(e.target.value)
              clear("api_key")
              staleTest()
            }}
          />
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="absolute top-0.5 right-0.5"
            aria-label={showKey ? "Hide API key" : "Show API key"}
            aria-pressed={showKey}
            onClick={() => setShowKey((s) => !s)}
          >
            {showKey ? <EyeOff aria-hidden="true" /> : <Eye aria-hidden="true" />}
          </Button>
        </div>
        {errors.api_key ? (
          <p id="ep-api_key-error" className="text-sm text-destructive">
            {errors.api_key}
          </p>
        ) : editing && endpoint?.has_api_key ? (
          <p id="ep-api_key-hint" className="text-xs text-muted-foreground">
            {removeKey ? "The saved key is removed when you save." : `Saved key ${endpoint.api_key_hint ?? "set"}. Leave blank to keep it.`}
          </p>
        ) : null}
        {editing && endpoint?.has_api_key ? (
          <Label htmlFor="ep-remove-key" className="flex min-h-9 items-center gap-2 text-sm font-normal pointer-coarse:min-h-11">
            <Checkbox
              id="ep-remove-key"
              checked={removeKey}
              onCheckedChange={(c) => {
                setRemoveKey(c === true)
                if (c === true) setApiKey("")
                staleTest()
              }}
            />
            Remove the saved key
          </Label>
        ) : null}
      </div>

      <Field
        id="ep-default_concurrency"
        label="Default seqs"
        error={errors.default_concurrency}
        hint="Prefilled on new runs."
      >
        <Input
          id="ep-default_concurrency"
          type="number"
          inputMode="numeric"
          min={1}
          max={4096}
          value={seqs}
          className="tabular max-w-40"
          aria-invalid={!!errors.default_concurrency}
          aria-describedby={describe("default_concurrency", true)}
          onChange={(e) => {
            setSeqs(e.target.value)
            clear("default_concurrency")
          }}
        />
      </Field>

      <Collapsible open={headersOpen} onOpenChange={setHeadersOpen}>
        <CollapsibleTrigger className="flex min-h-9 pointer-coarse:min-h-11 items-center gap-1.5 rounded-md text-sm font-medium focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none">
          <ChevronRight className={`size-4 text-muted-foreground transition-transform duration-150 ${headersOpen ? "rotate-90" : ""}`} aria-hidden="true" />
          Headers
          <span className="font-normal text-muted-foreground">
            ({headers.length ? `${headers.length} set` : "optional"})
          </span>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="mt-2 grid gap-2">
            {headers.map((h, i) => (
              <div key={h.id} className="grid grid-cols-[1fr_1fr_auto] items-center gap-2">
                <Input
                  aria-label={`Header ${i + 1} name`}
                  value={h.key}
                  placeholder="X-Org-Id"
                  spellCheck={false}
                  autoCapitalize="none"
                  className="font-mono pointer-fine:text-[13px]"
                  onChange={(e) => {
                    setHeaders((rows) => rows.map((r) => (r.id === h.id ? { ...r, key: e.target.value } : r)))
                    staleTest()
                  }}
                />
                <Input
                  aria-label={`Header ${i + 1} value`}
                  value={h.value}
                  placeholder="value"
                  spellCheck={false}
                  autoCapitalize="none"
                  className="font-mono pointer-fine:text-[13px]"
                  onChange={(e) => {
                    setHeaders((rows) => rows.map((r) => (r.id === h.id ? { ...r, value: e.target.value } : r)))
                    staleTest()
                  }}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Remove header ${i + 1}`}
                  onClick={() => setHeaders((rows) => rows.filter((r) => r.id !== h.id))}
                >
                  <X aria-hidden="true" />
                </Button>
              </div>
            ))}
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="w-fit"
              onClick={() => setHeaders((rows) => [...rows, { id: nextId.current++, key: "", value: "" }])}
            >
              <Plus aria-hidden="true" />
              Add header
            </Button>
          </div>
        </CollapsibleContent>
      </Collapsible>

      <div className="grid gap-2 border-t pt-4">
        <div className="flex flex-wrap items-center gap-3">
          <Button
            type="button"
            variant="outline"
            disabled={testMutation.isPending || !baseUrl.trim()}
            onClick={() => testMutation.mutate()}
          >
            {testMutation.isPending ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
            {testMutation.isPending ? "Testing…" : "Test connection"}
          </Button>
          {!baseUrl.trim() ? <span className="text-xs text-muted-foreground">Enter a base URL to test it.</span> : null}
        </div>
        <div aria-live="polite" className="text-sm">
          {test?.ok ? (
            <div className="grid gap-1">
              <p>
                Connected in {duration(test.latency_ms)}
                <span className="text-muted-foreground"> · {count(test.models.length, "model", "models")}</span>
              </p>
              {test.models.length ? (
                <p className="font-mono text-[13px] break-all text-muted-foreground">
                  {test.models.slice(0, 4).join(", ")}
                  {test.models.length > 4 ? `, and ${test.models.length - 4} more` : ""}
                </p>
              ) : null}
            </div>
          ) : test ? (
            <div className="grid gap-0.5 text-destructive">
              {isKeyRejected(test.error) ? <p className="font-medium">Key rejected</p> : null}
              <p>{test.error ?? "The server did not answer."}</p>
            </div>
          ) : testError ? (
            <p className="text-destructive">{testError}</p>
          ) : null}
        </div>
      </div>

      {formError ? (
        <p role="alert" className="text-sm text-destructive">
          {formError}
        </p>
      ) : null}
      </div>

      <DialogFooter className="m-0 shrink-0 rounded-b-xl px-4 py-3">
        <Button type="button" variant="outline" onClick={onCancel} disabled={save.isPending}>
          Cancel
        </Button>
        <Button type="submit" aria-disabled={save.isPending}>
          {save.isPending ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
          {save.isPending ? "Saving…" : editing ? "Save changes" : "Add endpoint"}
        </Button>
      </DialogFooter>
    </form>
  )
}
