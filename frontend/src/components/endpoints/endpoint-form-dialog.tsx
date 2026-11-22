import { useRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { ExternalLink, Eye, EyeOff, Loader2, Plus, X } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { FormDisclosure, FormGroup, FormItem, FormRow } from "@/components/shared/form-group"
import { Pill } from "@/components/shared/status-pill"
import { ApiError, api } from "@/lib/api"
import { count, duration } from "@/lib/format"
import type { Endpoint, EndpointIn, EndpointLocation, EndpointPreset, EndpointTestOut } from "@/lib/types"
import { isKeyRejected } from "./endpoint-status"
import { PresetRow, useEndpointPresets } from "./presets"

const DEFAULT_SEQS = 16
const FIELD_ORDER = ["name", "base_url", "api_key", "default_concurrency", "headers"]

type HeaderRow = { id: number; key: string; value: string }

/** Add or edit one endpoint. `endpoint` set means edit. Values survive a failed save. */
export function EndpointFormDialog({
  open,
  endpoint,
  preset = null,
  onOpenChange,
  focusField = null,
}: {
  open: boolean
  endpoint: Endpoint | null
  /** Add mode only: start from this preset. */
  preset?: EndpointPreset | null
  onOpenChange: (open: boolean) => void
  /** Open with this field focused: "api_key" after a rejected key, "base_url" when unreachable. */
  focusField?: "api_key" | "base_url" | null
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden sm:max-w-xl"
        onOpenAutoFocus={(ev) => {
          const target = focusField ?? (preset?.needs_api_key ? "api_key" : null)
          if (!target) return
          ev.preventDefault()
          document.getElementById(`ep-${target}`)?.focus()
        }}
      >
        <EndpointForm endpoint={endpoint} preset={preset} onDone={() => onOpenChange(false)} onCancel={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  )
}

function EndpointForm({
  endpoint,
  preset,
  onDone,
  onCancel,
}: {
  endpoint: Endpoint | null
  preset: EndpointPreset | null
  onDone: () => void
  onCancel: () => void
}) {
  const qc = useQueryClient()
  const editing = endpoint !== null
  const nextId = useRef(1)
  const presets = useEndpointPresets()
  const [presetId, setPresetId] = useState<string | null>(preset?.id ?? null)
  const [keyUrl, setKeyUrl] = useState<string | null>(preset?.needs_api_key ? preset.api_key_url : null)
  const [location, setLocation] = useState<EndpointLocation>(preset?.location ?? "local")
  const [name, setName] = useState(endpoint?.name ?? preset?.name ?? "")
  const [baseUrl, setBaseUrl] = useState(endpoint?.base_url ?? preset?.base_url ?? "")
  const [apiKey, setApiKey] = useState("")
  const [showKey, setShowKey] = useState(false)
  const [removeKey, setRemoveKey] = useState(false)
  const [seqs, setSeqs] = useState(String(endpoint?.default_concurrency ?? preset?.default_concurrency ?? DEFAULT_SEQS))
  const [headers, setHeaders] = useState<HeaderRow[]>(() =>
    Object.entries(endpoint?.headers ?? preset?.headers ?? {}).map(([key, value]) => ({ id: nextId.current++, key, value })),
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

  /** Fill the form from a preset (null = Custom, which clears it) and focus what still needs input. */
  function pick(p: EndpointPreset | null) {
    setPresetId(p ? p.id : "custom")
    setKeyUrl(p?.needs_api_key ? p.api_key_url : null)
    setLocation(p?.location ?? "local")
    setName(p?.name ?? "")
    setBaseUrl(p?.base_url ?? "")
    setSeqs(String(p?.default_concurrency ?? DEFAULT_SEQS))
    setApiKey("")
    const rows = Object.entries(p?.headers ?? {}).map(([key, value]) => ({ id: nextId.current++, key, value }))
    setHeaders(rows)
    setHeadersOpen(rows.length > 0)
    setErrors({})
    setFormError(null)
    staleTest()
    window.requestAnimationFrame(() => document.getElementById(p ? (p.needs_api_key ? "ep-api_key" : "ep-name") : "ep-name")?.focus())
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
    if (!editing) body.location = location
    if (apiKey) body.api_key = apiKey
    else if (removeKey) body.api_key = ""
    save.mutate(body)
  }

  const clear = (field: string) => setErrors((e) => (e[field] ? { ...e, [field]: "" } : e))
  const describe = (field: string, hint: boolean) => (errors[field] ? `ep-${field}-error` : hint ? `ep-${field}-hint` : undefined)

  return (
    <form onSubmit={submit} noValidate className="flex min-h-0 flex-1 flex-col">
      <DialogHeader className="pr-8 pb-4">
        <DialogTitle>{editing ? "Edit endpoint" : "Add endpoint"}</DialogTitle>
      </DialogHeader>
      {/* The scroll area runs to the dialog's edges so focus rings are not clipped. */}
      <div className="-mx-5 grid min-h-0 flex-1 content-start gap-5 overflow-y-auto px-5 py-1">
        {!editing && presets.data?.length ? <PresetRow presets={presets.data} value={presetId} onPick={pick} /> : null}

        <FormGroup>
          <FormRow id="ep-name" label="Name" error={errors.name}>
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
          </FormRow>

          <FormRow
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
              className="font-mono pointer-fine:text-xs"
              aria-invalid={!!errors.base_url}
              aria-describedby={describe("base_url", true)}
              onChange={(e) => {
                setBaseUrl(e.target.value)
                clear("base_url")
                staleTest()
              }}
            />
          </FormRow>

          <FormRow
            id="ep-api_key"
            label="API key"
            optional={!keyUrl}
            error={errors.api_key}
            hint={
              editing && endpoint?.has_api_key ? (
                removeKey ? (
                  "The saved key is removed when you save."
                ) : (
                  <>
                    Saved key <span className="font-mono">{endpoint.api_key_hint ?? "set"}</span>. Leave blank to keep it.
                  </>
                )
              ) : undefined
            }
          >
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
                className="pr-10 font-mono pointer-fine:text-xs"
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
            {!errors.api_key && !(editing && endpoint?.has_api_key) && keyUrl ? (
              <a
                href={keyUrl}
                target="_blank"
                rel="noreferrer noopener"
                className="inline-flex min-h-6 w-fit items-center gap-1 rounded-xs text-sm underline underline-offset-4 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none pointer-coarse:min-h-11"
              >
                Get a key
                <ExternalLink className="size-3.5 text-muted-foreground" aria-hidden="true" />
                <span className="sr-only">(opens in a new tab)</span>
              </a>
            ) : null}
            {editing && endpoint?.has_api_key ? (
              <Label htmlFor="ep-remove-key" className="flex min-h-7 w-fit items-center gap-2 text-sm font-normal pointer-coarse:min-h-11">
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
          </FormRow>

          <FormRow id="ep-default_concurrency" label="Default seqs" error={errors.default_concurrency} hint="Prefilled on new runs." align="end">
            <Input
              id="ep-default_concurrency"
              type="number"
              inputMode="numeric"
              min={1}
              max={4096}
              value={seqs}
              className="w-24 text-right tabular"
              aria-invalid={!!errors.default_concurrency}
              aria-describedby={describe("default_concurrency", true)}
              onChange={(e) => {
                setSeqs(e.target.value)
                clear("default_concurrency")
              }}
            />
          </FormRow>
        </FormGroup>

        <FormDisclosure
          title="Headers"
          summary={headers.length ? count(headers.length, "header", "headers") : "Optional"}
          open={headersOpen}
          onOpenChange={setHeadersOpen}
        >
          {headers.map((h, i) => (
            <FormItem key={h.id} className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] items-center gap-2">
              <Input
                aria-label={`Header ${i + 1} name`}
                value={h.key}
                placeholder="X-Org-Id"
                spellCheck={false}
                autoCapitalize="none"
                className="font-mono pointer-fine:text-xs"
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
                className="font-mono pointer-fine:text-xs"
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
            </FormItem>
          ))}
          <FormItem>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="-ml-1.5"
              onClick={() => setHeaders((rows) => [...rows, { id: nextId.current++, key: "", value: "" }])}
            >
              <Plus aria-hidden="true" />
              Add header
            </Button>
          </FormItem>
        </FormDisclosure>

        <div className="grid gap-2">
          <div className="flex flex-wrap items-center gap-3">
            <Button
              type="button"
              variant="secondary"
              disabled={testMutation.isPending || !baseUrl.trim()}
              onClick={() => testMutation.mutate()}
            >
              {testMutation.isPending ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
              {testMutation.isPending ? "Testing…" : "Test connection"}
            </Button>
            {!baseUrl.trim() ? <span className="text-xs text-muted-foreground">Enter a base URL to test it.</span> : null}
          </div>
          <div aria-live="polite" className="text-sm empty:hidden">
            {test?.ok ? (
              <div className="grid gap-1">
                <p className="flex flex-wrap items-center gap-x-1.5">
                  <Pill tone="positive">Connected</Pill>
                  <span className="text-muted-foreground tabular">
                    in {duration(test.latency_ms)} · {count(test.models.length, "model", "models")}
                  </span>
                </p>
                {test.models.length ? (
                  <p className="font-mono text-xs break-all text-muted-foreground">
                    {test.models.slice(0, 4).join(", ")}
                    {test.models.length > 4 ? `, and ${test.models.length - 4} more` : ""}
                  </p>
                ) : null}
              </div>
            ) : test ? (
              <div className="grid gap-1">
                <Pill tone="negative">{isKeyRejected(test.error) ? "Key rejected" : "Unreachable"}</Pill>
                <p className="text-negative-ink">{test.error ?? "The server did not answer."}</p>
              </div>
            ) : testError ? (
              <p className="text-negative-ink">{testError}</p>
            ) : null}
          </div>
        </div>

        {formError ? (
          <p role="alert" className="text-sm text-destructive">
            {formError}
          </p>
        ) : null}
      </div>

      <DialogFooter className="m-0 shrink-0 pt-4">
        <Button type="button" variant="secondary" onClick={onCancel} disabled={save.isPending}>
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
