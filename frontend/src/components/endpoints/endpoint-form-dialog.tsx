import { useRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { ChevronRight, Eye, EyeOff, Loader2, Plus, X } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Field } from "@/components/shared/field"
import { ApiError, api } from "@/lib/api"
import { count, duration } from "@/lib/format"
import type { Endpoint, EndpointIn, EndpointLocation, EndpointTestOut } from "@/lib/types"

const SEQS: Record<EndpointLocation, number> = { local: 16, cloud: 8 }
const FIELD_ORDER = ["name", "base_url", "api_key", "default_concurrency", "headers"]

type HeaderRow = { id: number; key: string; value: string }

/** Add or edit one endpoint. `endpoint` set means edit. Values survive a failed save. */
export function EndpointFormDialog({
  open,
  endpoint,
  onOpenChange,
}: {
  open: boolean
  endpoint: Endpoint | null
  onOpenChange: (open: boolean) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-lg">
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
  const [location, setLocation] = useState<EndpointLocation>((endpoint?.location as EndpointLocation) ?? "local")
  const [apiKey, setApiKey] = useState("")
  const [showKey, setShowKey] = useState(false)
  const [removeKey, setRemoveKey] = useState(false)
  const [seqs, setSeqs] = useState(String(endpoint?.default_concurrency ?? SEQS.local))
  const [seqsTouched, setSeqsTouched] = useState(editing)
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
      location,
      default_concurrency: n,
      headers: headerObject(),
    }
    if (apiKey) body.api_key = apiKey
    else if (removeKey) body.api_key = ""
    save.mutate(body)
  }

  const clear = (field: string) => setErrors((e) => (e[field] ? { ...e, [field]: "" } : e))
  const describe = (field: string, hint: boolean) => (errors[field] ? `ep-${field}-error` : hint ? `ep-${field}-hint` : undefined)

  return (
    <form onSubmit={submit} noValidate className="grid gap-4">
      <DialogHeader>
        <DialogTitle>{editing ? "Edit endpoint" : "Add endpoint"}</DialogTitle>
        <DialogDescription>A model server that speaks the OpenAI chat completions API.</DialogDescription>
      </DialogHeader>

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
        hint="The OpenAI-compatible API root. From Docker, reach this machine at http://host.docker.internal:PORT/v1."
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
          className="font-mono text-[13px]"
          aria-invalid={!!errors.base_url}
          aria-describedby={describe("base_url", true)}
          onChange={(e) => {
            setBaseUrl(e.target.value)
            clear("base_url")
            staleTest()
          }}
        />
      </Field>

      <fieldset className="grid gap-1.5">
        <legend className="mb-1.5 text-sm font-medium">Where it runs</legend>
        <RadioGroup
          value={location}
          className="grid-cols-1 gap-1 sm:grid-cols-2"
          onValueChange={(v) => {
            const loc = v as EndpointLocation
            setLocation(loc)
            if (!seqsTouched) setSeqs(String(SEQS[loc]))
          }}
        >
          {(
            [
              ["local", "Local hardware"],
              ["cloud", "Cloud API"],
            ] as const
          ).map(([value, label]) => (
            <Label key={value} htmlFor={`ep-loc-${value}`} className="flex min-h-9 items-center gap-2.5 rounded-md border px-3 text-sm font-normal">
              <RadioGroupItem id={`ep-loc-${value}`} value={value} />
              {label}
            </Label>
          ))}
        </RadioGroup>
        <p className="text-xs text-muted-foreground">This only changes the suggested seqs: 16 for local hardware, 8 for a hosted API.</p>
      </fieldset>

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
            placeholder={
              editing && endpoint?.has_api_key ? `Leave blank to keep the saved key (${endpoint.api_key_hint ?? "saved"})` : "sk-012345…"
            }
            autoComplete="off"
            spellCheck={false}
            autoCapitalize="none"
            className="pr-10 font-mono text-[13px]"
            aria-invalid={!!errors.api_key}
            aria-describedby={errors.api_key ? "ep-api_key-error" : undefined}
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
        {errors.api_key ? <p id="ep-api_key-error" className="text-sm text-destructive">{errors.api_key}</p> : null}
        {editing && endpoint?.has_api_key ? (
          <Label htmlFor="ep-remove-key" className="flex min-h-8 items-center gap-2 text-sm font-normal">
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
        hint="How many sequences run at once when you start a run on this endpoint. You can change it per run."
        className="max-w-40"
      >
        <Input
          id="ep-default_concurrency"
          type="number"
          inputMode="numeric"
          min={1}
          max={4096}
          value={seqs}
          className="tabular"
          aria-invalid={!!errors.default_concurrency}
          aria-describedby={describe("default_concurrency", true)}
          onChange={(e) => {
            setSeqs(e.target.value)
            setSeqsTouched(true)
            clear("default_concurrency")
          }}
        />
      </Field>

      <Collapsible open={headersOpen} onOpenChange={setHeadersOpen}>
        <CollapsibleTrigger className="flex min-h-8 items-center gap-1.5 rounded-md text-sm font-medium focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none">
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
                  className="font-mono text-[13px]"
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
                  className="font-mono text-[13px]"
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
            <p className="text-destructive">{test.error ?? "The server did not answer."}</p>
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

      <DialogFooter>
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
