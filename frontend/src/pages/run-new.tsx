import { useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react"
import { Link, useNavigate, useSearchParams } from "react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { ChevronRight, RotateCw } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Field } from "@/components/shared/field"
import { Page } from "@/components/shared/page"
import { ModelCombobox } from "@/components/runs/model-combobox"
import { NumberStepper } from "@/components/runs/number-stepper"
import { SEGMENT_ON, SEQS_CHOICES, editEndpointHref, endpointTrouble, troubleField, runKeys, useEndpoints, usePromptSets, useRuns } from "@/components/runs/run-data"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { ApiError, api } from "@/lib/api"
import { count, dateTime, duration, int } from "@/lib/format"
import type { Endpoint, PromptSet, Run, RunCreate } from "@/lib/types"
import { cn } from "@/lib/utils"

// ------------------------------------------------------------------ form state

interface Form {
  endpoint: string
  model: string
  prompt_set: string
  concurrency: string
  samples: string
  temperature: string
  top_p: string
  max_tokens: string
  max_tokens_field: "max_tokens" | "max_completion_tokens"
  seed: string
  system_prompt: string
  user_template: string
  extra_body: string
  stream: boolean
  think_open: string
  think_close: string
  max_attempts: string
  timeout: string
  pause_after: string
  name: string
}

type FieldId = Exclude<keyof Form, "max_tokens_field" | "stream">
type Errors = Partial<Record<FieldId, string>>

const DEFAULTS: Form = {
  endpoint: "",
  model: "",
  prompt_set: "",
  concurrency: "16",
  samples: "1",
  temperature: "0.7",
  top_p: "",
  max_tokens: "8192",
  max_tokens_field: "max_tokens",
  seed: "",
  system_prompt: "",
  user_template: "",
  extra_body: "",
  stream: true,
  think_open: "<think>",
  think_close: "</think>",
  max_attempts: "4",
  timeout: "1800",
  pause_after: "10",
  name: "",
}

/** Field order on screen, for focusing the first error. */
const ORDER: FieldId[] = [
  "endpoint",
  "model",
  "prompt_set",
  "concurrency",
  "samples",
  "temperature",
  "top_p",
  "max_tokens",
  "seed",
  "system_prompt",
  "user_template",
  "extra_body",
  "think_open",
  "think_close",
  "max_attempts",
  "timeout",
  "pause_after",
  "name",
]
const PROMPT_FIELDS: FieldId[] = ["system_prompt", "user_template"]
const ADVANCED_FIELDS: FieldId[] = ["extra_body", "think_open", "think_close", "max_attempts", "timeout", "pause_after"]

/** Server field paths (from RunCreate) to the inputs that hold them. */
const SERVER_FIELD: Record<string, FieldId> = {
  endpoint_id: "endpoint",
  model: "model",
  prompt_set_id: "prompt_set",
  concurrency: "concurrency",
  name: "name",
  "params.samples_per_prompt": "samples",
  "params.temperature": "temperature",
  "params.top_p": "top_p",
  "params.max_tokens": "max_tokens",
  "params.seed": "seed",
  "params.system_prompt": "system_prompt",
  "params.user_template": "user_template",
  "params.extra_body": "extra_body",
  "params.think_open": "think_open",
  "params.think_close": "think_close",
  "params.max_attempts": "max_attempts",
  "params.request_timeout_s": "timeout",
  "params.pause_after_failures": "pause_after",
}

const ON = SEGMENT_ON

/** "30 min", "90 s", "1 h 30 min": a whole timeout in words. */
function seconds(s: number): string {
  if (s % 3600 === 0) return `${int(s / 3600)} h`
  if (s >= 3600 && s % 60 === 0) return `${int(Math.floor(s / 3600))} h ${int((s % 3600) / 60)} min`
  if (s % 60 === 0) return `${int(s / 60)} min`
  return duration(s * 1000)
}

const str = (v: number | null | undefined) => (v === null || v === undefined ? "" : String(v))

function fromRun(run: Run): Partial<Form> {
  const p = run.params
  return {
    endpoint: run.endpoint_id ? String(run.endpoint_id) : "",
    model: run.model,
    prompt_set: String(run.prompt_set_id),
    concurrency: String(run.concurrency),
    samples: String(p.samples_per_prompt),
    temperature: str(p.temperature),
    top_p: str(p.top_p),
    max_tokens: str(p.max_tokens),
    max_tokens_field: p.max_tokens_field,
    seed: str(p.seed),
    system_prompt: p.system_prompt ?? "",
    user_template: p.user_template ?? "",
    extra_body: Object.keys(p.extra_body ?? {}).length ? JSON.stringify(p.extra_body, null, 2) : "",
    stream: p.stream,
    think_open: p.think_open,
    think_close: p.think_close,
    max_attempts: String(p.max_attempts),
    timeout: String(p.request_timeout_s),
    pause_after: String(p.pause_after_failures),
  }
}

/**
 * The model to prefill for an endpoint: the one its most recent run used, or its only listed
 * model. Never the first of many, which would be a guess.
 */
function suggestedModel(ep: Endpoint, runs: Run[] | undefined): string {
  const last = runs?.find((r) => r.endpoint_id === ep.id)
  // A last-used model the endpoint no longer lists is likely wrong, unless it lists nothing.
  if (last && (ep.models.length === 0 || ep.models.includes(last.model))) return last.model
  return ep.models.length === 1 ? ep.models[0] : ""
}

function isInt(s: string) {
  return /^-?\d+$/.test(s.trim())
}

/** One field's rule. Empty optional fields are fine. */
function check(id: FieldId, f: Form): string | undefined {
  const v = f[id].trim()
  const num = Number(v)
  const intIn = (min: number, max: number, what: string) =>
    !isInt(v) || num < min || num > max ? `${what} must be a whole number from ${int(min)} to ${int(max)}.` : undefined
  switch (id) {
    case "endpoint":
      return v ? undefined : "Choose an endpoint."
    case "model":
      return v ? undefined : "Choose a model, or type its id."
    case "prompt_set":
      return v ? undefined : "Choose a prompt set."
    case "concurrency":
      return intIn(1, 4096, "Seqs")
    case "samples":
      return intIn(1, 64, "Samples per prompt")
    case "temperature":
      if (!v) return undefined
      return Number.isNaN(num) || num < 0 || num > 2 ? "Temperature must be a number from 0 to 2." : undefined
    case "top_p":
      if (!v) return undefined
      return Number.isNaN(num) || num <= 0 || num > 1 ? "Top p must be above 0 and at most 1." : undefined
    case "max_tokens":
      if (!v) return undefined
      return !isInt(v) || num < 1 ? "Max tokens must be a whole number of 1 or more." : undefined
    case "seed":
      if (!v) return undefined
      return isInt(v) ? undefined : "Seed must be a whole number."
    case "user_template":
      if (!v) return undefined
      return v.includes("{prompt}") ? undefined : "Add {prompt} where each prompt should go."
    case "extra_body": {
      if (!v) return undefined
      try {
        const parsed: unknown = JSON.parse(v)
        if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return "Enter a JSON object, like {\"key\": \"value\"}."
        return undefined
      } catch (e) {
        return `This is not valid JSON: ${(e as Error).message}`
      }
    }
    case "think_open":
    case "think_close":
      return v ? undefined : "Enter a tag, for example <think>."
    case "max_attempts":
      return intIn(1, 20, "Attempts")
    case "timeout":
      return Number.isNaN(num) || !v || num < 10 ? "The timeout must be at least 10 seconds." : undefined
    case "pause_after":
      return !isInt(v) || num < 0 ? "Enter 0 or a whole number of failures." : undefined
    default:
      return undefined
  }
}

function toBody(f: Form): RunCreate {
  const numOrNull = (s: string) => (s.trim() === "" ? null : Number(s))
  return {
    name: f.name.trim() || null,
    endpoint_id: Number(f.endpoint),
    model: f.model.trim(),
    prompt_set_id: Number(f.prompt_set),
    concurrency: Number(f.concurrency),
    params: {
      samples_per_prompt: Number(f.samples),
      temperature: numOrNull(f.temperature),
      top_p: numOrNull(f.top_p),
      max_tokens: numOrNull(f.max_tokens),
      max_tokens_field: f.max_tokens_field,
      seed: numOrNull(f.seed),
      system_prompt: f.system_prompt.trim() ? f.system_prompt : null,
      user_template: f.user_template.trim() ? f.user_template : null,
      extra_body: f.extra_body.trim() ? (JSON.parse(f.extra_body) as Record<string, unknown>) : {},
      stream: f.stream,
      think_open: f.think_open,
      think_close: f.think_close,
      max_attempts: Number(f.max_attempts),
      request_timeout_s: Number(f.timeout),
      pause_after_failures: Number(f.pause_after),
    },
  }
}

// ------------------------------------------------------------------ pieces

function Section({ id, title, children, className }: { id: string; title?: string; children: ReactNode; className?: string }) {
  if (!title) return <div className={cn("grid gap-5 pt-10 first:pt-0", className)}>{children}</div>
  return (
    <section aria-labelledby={id} className={cn("grid gap-5 pt-10 first:pt-0", className)}>
      <h2 id={id} className="-mb-1 text-base font-semibold">
        {title}
      </h2>
      {children}
    </section>
  )
}

function Disclosure({
  title,
  summary,
  open,
  onOpenChange,
  children,
}: {
  title: string
  summary: string
  open: boolean
  onOpenChange: (open: boolean) => void
  children: ReactNode
}) {
  return (
    <Collapsible open={open} onOpenChange={onOpenChange} className="pt-10">
      <h2 className="text-base font-semibold">
        <CollapsibleTrigger className="group -mx-1 flex items-center gap-1.5 rounded-md px-1 py-0.5 text-left hover:text-foreground">
          <ChevronRight
            className="size-4 text-muted-foreground transition-transform duration-200 ease-out group-data-[state=open]:rotate-90"
            aria-hidden="true"
          />
          {title}
        </CollapsibleTrigger>
      </h2>
      {!open ? <p className="mt-1 pl-6 text-sm text-muted-foreground">{summary}</p> : null}
      <CollapsibleContent className="grid gap-5 pt-4">{children}</CollapsibleContent>
    </Collapsible>
  )
}

function describe(id: string, error?: string, hint?: boolean) {
  return error ? `${id}-error` : hint ? `${id}-hint` : undefined
}

function FormSkeleton() {
  return (
    <div className="grid max-w-[40rem] gap-5" aria-busy="true" aria-label="Loading endpoints and prompt sets">
      {["Endpoint", "Model", "Prompt set"].map((l) => (
        <div key={l} className="grid gap-1.5">
          <span className="text-sm font-medium">{l}</span>
          <Bar className="h-8 w-full rounded-lg" />
        </div>
      ))}
    </div>
  )
}

// ------------------------------------------------------------------ page

interface Initial {
  form: Form
  promptOpen: boolean
  advancedOpen: boolean
}

/** The form's first values: a previous run's settings (Run again), or URL prefills and sensible picks. */
function initialState(eps: Endpoint[], ss: PromptSet[], runs: Run[] | undefined, source: Run | undefined, params: URLSearchParams): Initial {
  let form: Form = { ...DEFAULTS }
  let promptOpen = false
  let advancedOpen = false
  if (source) {
    form = { ...form, ...fromRun(source) }
    if (!eps.some((e) => String(e.id) === form.endpoint)) form.endpoint = ""
    if (!ss.some((s) => String(s.id) === form.prompt_set)) form.prompt_set = ""
    const p = source.params
    promptOpen = !!(p.system_prompt || p.user_template)
    const d = DEFAULTS
    advancedOpen =
      !!form.extra_body ||
      !p.stream ||
      form.think_open !== d.think_open ||
      form.think_close !== d.think_close ||
      form.max_attempts !== d.max_attempts ||
      form.timeout !== d.timeout ||
      form.pause_after !== d.pause_after
  } else {
    const ep = params.get("endpoint")
    const ps = params.get("prompt_set")
    const pickedEp = eps.find((e) => String(e.id) === ep) ?? (eps.length === 1 ? eps[0] : undefined)
    if (pickedEp) {
      form.endpoint = String(pickedEp.id)
      form.concurrency = String(pickedEp.default_concurrency)
      form.model = suggestedModel(pickedEp, runs)
    }
    const pickedSet = ss.find((s) => String(s.id) === ps) ?? (ss.length === 1 ? ss[0] : undefined)
    if (pickedSet) form.prompt_set = String(pickedSet.id)
  }
  return { form, promptOpen, advancedOpen }
}

export function NewRunPage() {
  const [params] = useSearchParams()
  const fromId = params.get("from")
  const endpoints = useEndpoints()
  const sets = usePromptSets()
  const runs = useRuns()
  const source = useQuery({
    queryKey: runKeys.detail(Number(fromId)),
    queryFn: () => api.get<Run>(`/api/runs/${fromId}`),
    enabled: !!fromId,
  })

  useEffect(() => {
    document.title = "New run · Stellarator"
    return () => {
      document.title = "Stellarator"
    }
  }, [])

  // The runs list only suggests a model, so a failure to load it does not block the form.
  const ready = endpoints.isSuccess && sets.isSuccess && !runs.isPending && (!fromId || source.isSuccess || source.isError)

  return (
    <Page title="New run" back={{ to: "/runs", label: "Runs" }}>
      {ready ? (
        <NewRunForm
          initial={initialState(endpoints.data, sets.data, runs.data, source.data, params)}
          sourceError={fromId && source.isError ? `Couldn't copy the settings of run ${fromId}. ${source.error.message} Defaults are filled in instead.` : null}
        />
      ) : endpoints.isError || sets.isError ? (
        <ErrorBlock
          className="max-w-[40rem]"
          message="Couldn't load your endpoints and prompt sets. Check that the Stellarator server is running."
          onRetry={() => {
            void endpoints.refetch()
            void sets.refetch()
          }}
        />
      ) : (
        <FormSkeleton />
      )}
    </Page>
  )
}

/** Mounted once its data has loaded, so its state starts from the right values. */
function NewRunForm({ initial, sourceError }: { initial: Initial; sourceError: string | null }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const endpoints = useEndpoints()
  const sets = usePromptSets()
  const runs = useRuns()

  const [form, setForm] = useState<Form>(initial.form)
  const [errors, setErrors] = useState<Errors>({})
  const [formError, setFormError] = useState<string | null>(null)
  const [promptOpen, setPromptOpen] = useState(initial.promptOpen)
  const [advancedOpen, setAdvancedOpen] = useState(initial.advancedOpen)
  const [focusRequest, setFocusRequest] = useState(0)
  const focusTarget = useRef<FieldId | null>(null)
  const [modelsNote, setModelsNote] = useState<{ ok: boolean; text: string } | null>(null)

  // Focus the first field with an error once its section is open.
  useEffect(() => {
    const id = focusTarget.current
    if (!id) return
    focusTarget.current = null
    const el = document.getElementById(id)
    if (el) {
      el.focus()
      el.scrollIntoView({ block: "center" })
    }
  }, [focusRequest])

  const endpoint: Endpoint | undefined = endpoints.data?.find((e) => String(e.id) === form.endpoint)
  const promptSet = sets.data?.find((s) => String(s.id) === form.prompt_set)

  const set = <K extends keyof Form>(key: K, value: Form[K]) => {
    setForm((f) => ({ ...f, [key]: value }))
    if (key in errors) setErrors((e) => ({ ...e, [key]: undefined }))
  }
  const blur = (id: FieldId) => () => {
    const msg = check(id, form)
    setErrors((e) => ({ ...e, [id]: msg }))
  }

  const chooseEndpoint = (id: string) => {
    const ep = endpoints.data?.find((e) => String(e.id) === id)
    setModelsNote(null)
    setForm((f) => ({
      ...f,
      endpoint: id,
      concurrency: ep ? String(ep.default_concurrency) : f.concurrency,
      // Keep a model the new endpoint also lists, or a typed one when it lists none.
      model: !ep || (f.model && (ep.models.includes(f.model) || ep.models.length === 0)) ? f.model : suggestedModel(ep, runs.data),
    }))
    setErrors((e) => ({ ...e, endpoint: undefined, model: undefined, concurrency: undefined }))
  }

  const refreshModels = useMutation({
    mutationFn: (id: number) => api.post<Endpoint>(`/api/endpoints/${id}/check`),
    onSuccess: (ep) => {
      queryClient.setQueryData<Endpoint[]>(["endpoints"], (list) => list?.map((e) => (e.id === ep.id ? ep : e)))
      if (ep.last_ok === false) {
        setModelsNote({ ok: false, text: `Couldn't list models. ${ep.last_error ?? "The endpoint did not answer."}` })
      } else {
        setModelsNote({ ok: true, text: `Found ${count(ep.models.length, "model", "models")}.` })
        if (ep.models.length === 1) setForm((f) => (f.model ? f : { ...f, model: ep.models[0] }))
      }
    },
    onError: (e: Error) => setModelsNote({ ok: false, text: `Couldn't list models. ${e.message}` }),
  })

  const start = useMutation({
    mutationFn: (body: RunCreate) => api.post<Run>("/api/runs", body),
    onSuccess: (run) => {
      void queryClient.invalidateQueries({ queryKey: runKeys.list })
      void queryClient.invalidateQueries({ queryKey: ["endpoints"] })
      void queryClient.invalidateQueries({ queryKey: ["prompt-sets"] })
      navigate(`/runs/${run.id}`)
    },
    onError: (e: Error) => {
      if (e instanceof ApiError && Object.keys(e.fields).length) {
        const mapped: Errors = {}
        const unmapped: string[] = []
        for (const [path, msg] of Object.entries(e.fields)) {
          const id = SERVER_FIELD[path]
          if (id) mapped[id] = msg
          else unmapped.push(msg)
        }
        showErrors(mapped)
        setFormError(unmapped.length ? unmapped.join(" ") : null)
      } else {
        setFormError(e.message)
      }
    },
  })

  function showErrors(found: Errors) {
    setErrors(found)
    const first = ORDER.find((id) => found[id])
    if (!first) return
    if (PROMPT_FIELDS.includes(first)) setPromptOpen(true)
    if (ADVANCED_FIELDS.includes(first)) setAdvancedOpen(true)
    focusTarget.current = first
    setFocusRequest((n) => n + 1)
  }

  function submit(e: FormEvent) {
    e.preventDefault()
    if (start.isPending) return
    setFormError(null)
    const found: Errors = {}
    for (const id of ORDER) {
      const msg = check(id, form)
      if (msg) found[id] = msg
    }
    if (Object.keys(found).length) {
      showErrors(found)
      return
    }
    setErrors({})
    start.mutate(toBody(form))
  }

  const samples = Number.parseInt(form.samples, 10)
  const total = useMemo(() => {
    if (!promptSet || !Number.isFinite(samples) || samples < 1) return null
    return `${count(promptSet.prompt_count, "prompt", "prompts")} × ${count(samples, "sample", "samples")} = ${count(
      promptSet.prompt_count * samples,
      "request",
      "requests",
    )}`
  }, [promptSet, samples])

  const defaultName = form.model && promptSet ? `${form.model} on ${promptSet.name}` : ""
  const modelHint = modelsNote ? modelsNote.text : endpoint && !endpoint.models.length ? "No models listed. Type a model id." : null
  const cloud = endpoint?.location === "cloud"
  const trouble = endpointTrouble(endpoint)
  // Prompt sets that share a name are told apart by when they were imported.
  const sharedNames = useMemo(() => {
    const seen = new Map<string, number>()
    for (const s of sets.data ?? []) seen.set(s.name, (seen.get(s.name) ?? 0) + 1)
    return seen
  }, [sets.data])
  const timeoutS = Number(form.timeout)

  const advancedSummary = [
    form.stream ? "Streaming" : "Not streaming",
    count(Number(form.max_attempts) || 0, "attempt", "attempts") + " per sample",
    Number.isFinite(timeoutS) && timeoutS > 0 ? `${seconds(timeoutS)} timeout` : null,
    Number(form.pause_after) > 0 ? `pause after ${int(Number(form.pause_after))} failures in a row` : "never pause on failures",
    form.extra_body.trim() ? "extra request body set" : null,
  ]
    .filter(Boolean)
    .join(", ")
  const promptSummary =
    form.system_prompt.trim() || form.user_template.trim()
      ? [form.system_prompt.trim() ? "System prompt replaced" : null, form.user_template.trim() ? "prompt template set" : null]
          .filter(Boolean)
          .join(", ")
      : "None"

  return (
    <form onSubmit={submit} noValidate className="max-w-[40rem]">
      {sourceError ? <ErrorBlock className="mb-6" message={sourceError} /> : null}

      <Section id="sec-model">
        <Field
          id="endpoint"
          label="Endpoint"
          error={errors.endpoint}
          hint={
            endpoint && trouble ? (
              <span className="text-sm">
                <span className="text-destructive">{trouble === "Key rejected" ? "Key rejected at last check." : "Unreachable at last check."}</span>{" "}
                <Link
                  to={editEndpointHref(endpoint.id, `/runs/new?endpoint=${endpoint.id}${form.prompt_set ? `&prompt_set=${form.prompt_set}` : ""}`, troubleField(trouble))}
                  className="inline-flex items-center text-foreground underline underline-offset-4"
                >
                  Edit endpoint
                </Link>
              </span>
            ) : endpoint ? (
              <span className="font-mono">{endpoint.base_url}</span>
            ) : undefined
          }
        >
          {endpoints.data?.length ? (
            <Select value={form.endpoint} onValueChange={chooseEndpoint}>
              <SelectTrigger
                id="endpoint"
                className="w-full min-w-0"
                aria-invalid={!!errors.endpoint || undefined}
                aria-describedby={describe("endpoint", errors.endpoint, !!endpoint)}
              >
                {/* The field shows the name alone; a failed check is said once, in the line below. */}
                <SelectValue placeholder="Choose an endpoint">{endpoint ? <span className="truncate">{endpoint.name}</span> : undefined}</SelectValue>
              </SelectTrigger>
              <SelectContent position="popper" className="max-w-(--radix-select-content-available-width)">
                {endpoints.data.map((e) => {
                  const t = endpointTrouble(e)
                  return (
                    <SelectItem key={e.id} value={String(e.id)}>
                      <span className="min-w-0 break-words">
                        {e.name} <span className={t ? "text-destructive" : "text-muted-foreground"}>· {t ?? (e.location === "cloud" ? "Cloud" : "Local")}</span>
                      </span>
                    </SelectItem>
                  )
                })}
              </SelectContent>
            </Select>
          ) : (
            <p id="endpoint" tabIndex={-1} className="text-sm text-muted-foreground">
              No endpoints yet.{" "}
              <Link to="/endpoints" className="text-foreground underline underline-offset-4">
                Add an endpoint
              </Link>
            </p>
          )}
        </Field>

        <div className="grid gap-1.5">
          <div className="flex items-end justify-between gap-3">
            <Label htmlFor="model" className="text-sm font-medium">
              Model
            </Label>
            {endpoint ? (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="-my-1 text-muted-foreground"
                disabled={refreshModels.isPending}
                onClick={() => refreshModels.mutate(endpoint.id)}
              >
                <RotateCw aria-hidden="true" />
                {refreshModels.isPending ? "Refreshing…" : "Refresh models"}
              </Button>
            ) : null}
          </div>
          <ModelCombobox
            id="model"
            value={form.model}
            onChange={(m) => set("model", m)}
            models={endpoint?.models ?? []}
            invalid={!!errors.model}
            describedBy={errors.model ? "model-error" : modelHint ? "model-hint" : undefined}
            disabled={!endpoint}
          />
          {errors.model ? (
            <p id="model-error" className="text-sm text-destructive">
              {errors.model}
            </p>
          ) : (
            <p id="model-hint" aria-live="polite" className={cn("text-xs", modelsNote && !modelsNote.ok ? "text-destructive" : "text-muted-foreground")}>
              {modelHint}
            </p>
          )}
        </div>

        <Field id="prompt_set" label="Prompt set" error={errors.prompt_set}>
          {sets.data?.length ? (
            <Select value={form.prompt_set} onValueChange={(v) => set("prompt_set", v)}>
              <SelectTrigger
                id="prompt_set"
                className="w-full min-w-0"
                aria-invalid={!!errors.prompt_set || undefined}
                aria-describedby={describe("prompt_set", errors.prompt_set)}
              >
                {/* The field shows the name and size; the import date only helps tell same-named sets apart in the list. */}
                <SelectValue placeholder="Choose a prompt set">
                  {promptSet ? (
                    <>
                      <span className="truncate">{promptSet.name}</span>
                      <span className="text-muted-foreground tabular">· {count(promptSet.prompt_count, "prompt", "prompts")}</span>
                    </>
                  ) : undefined}
                </SelectValue>
              </SelectTrigger>
              <SelectContent position="popper" className="max-w-(--radix-select-content-available-width)">
                {sets.data.map((s) => (
                  <SelectItem key={s.id} value={String(s.id)}>
                    <span className="min-w-0 break-words">
                      {s.name}{" "}
                      <span className="text-muted-foreground tabular">
                        · {count(s.prompt_count, "prompt", "prompts")}
                        {(sharedNames.get(s.name) ?? 0) > 1 ? ` · ${dateTime(s.created_at)}` : null}
                      </span>
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <p id="prompt_set" tabIndex={-1} className="text-sm text-muted-foreground">
              No prompt sets yet.{" "}
              <Link to="/prompts/import" className="text-foreground underline underline-offset-4">
                Import prompts
              </Link>
            </p>
          )}
        </Field>
      </Section>

      <Section id="sec-requests" title="Requests">
        <div className="grid gap-1.5">
          <Label htmlFor="concurrency" className="text-sm font-medium">
            Seqs
          </Label>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <NumberStepper
              id="concurrency"
              name="seqs"
              value={form.concurrency}
              onChange={(v) => set("concurrency", v)}
              onBlur={blur("concurrency")}
              min={1}
              max={4096}
              invalid={!!errors.concurrency}
              describedBy={errors.concurrency ? "concurrency-error" : "concurrency-hint"}
            />
            <ToggleGroup
              type="single"
              variant="outline"
              size="sm"
              spacing={0}
              value={SEQS_CHOICES.includes(Number(form.concurrency)) ? form.concurrency : ""}
              onValueChange={(v) => v && set("concurrency", v)}
              aria-label="Common seqs values"
              className="flex-wrap"
            >
              {SEQS_CHOICES.map((n) => (
                <ToggleGroupItem key={n} value={String(n)} className={cn("min-w-9 tabular", ON)}>
                  {n}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </div>
          {errors.concurrency ? (
            <p id="concurrency-error" className="text-sm text-destructive">
              {errors.concurrency}
            </p>
          ) : (
            <p id="concurrency-hint" className="text-xs text-muted-foreground">
              {cloud ? "Stay under your provider's rate limit." : "Match your server's --max-num-seqs."} Changeable while running.
            </p>
          )}
        </div>

        <div className="grid gap-1.5">
          <Label htmlFor="samples" className="text-sm font-medium">
            Samples per prompt
          </Label>
          <NumberStepper
            id="samples"
            name="samples per prompt"
            value={form.samples}
            onChange={(v) => set("samples", v)}
            onBlur={blur("samples")}
            min={1}
            max={64}
            invalid={!!errors.samples}
            describedBy={errors.samples ? "samples-error" : "samples-hint"}
          />
          {errors.samples ? (
            <p id="samples-error" className="text-sm text-destructive">
              {errors.samples}
            </p>
          ) : (
            <p id="samples-hint" className="text-xs text-muted-foreground">
              2 or more for preference pairs.
            </p>
          )}
        </div>

      </Section>

      <Section id="sec-sampling" title="Sampling">
        <div className="grid gap-5 sm:grid-cols-2">
          <Field id="temperature" label="Temperature" optional hint="0–2. Empty: server default." error={errors.temperature}>
            <Input
              id="temperature"
              inputMode="decimal"
              value={form.temperature}
              onChange={(e) => set("temperature", e.target.value)}
              onBlur={blur("temperature")}
              aria-invalid={!!errors.temperature || undefined}
              aria-describedby={describe("temperature", errors.temperature, true)}
              className="tabular"
            />
          </Field>
          <Field id="top_p" label="Top p" optional hint="0–1. Empty: server default." error={errors.top_p}>
            <Input
              id="top_p"
              inputMode="decimal"
              value={form.top_p}
              onChange={(e) => set("top_p", e.target.value)}
              onBlur={blur("top_p")}
              aria-invalid={!!errors.top_p || undefined}
              aria-describedby={describe("top_p", errors.top_p, true)}
              className="tabular"
            />
          </Field>
        </div>

        <div className="grid gap-1.5">
          <Label htmlFor="max_tokens" className="text-sm font-medium">
            Max tokens
          </Label>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <Input
              id="max_tokens"
              inputMode="numeric"
              value={form.max_tokens}
              onChange={(e) => set("max_tokens", e.target.value)}
              onBlur={blur("max_tokens")}
              aria-invalid={!!errors.max_tokens || undefined}
              aria-describedby={errors.max_tokens ? "max_tokens-error" : "max_tokens-hint"}
              className="w-32 tabular"
            />
            <span id="max_tokens_field-label" className="text-sm text-muted-foreground">
              sent as
            </span>
            <ToggleGroup
              type="single"
              variant="outline"
              size="sm"
              spacing={0}
              value={form.max_tokens_field}
              onValueChange={(v) => v && set("max_tokens_field", v as Form["max_tokens_field"])}
              aria-labelledby="max_tokens_field-label"
            >
              <ToggleGroupItem value="max_tokens" className={cn("font-mono text-xs", ON)}>
                max_tokens
              </ToggleGroupItem>
              <ToggleGroupItem value="max_completion_tokens" className={cn("font-mono text-xs", ON)}>
                max_completion_tokens
              </ToggleGroupItem>
            </ToggleGroup>
          </div>
          {errors.max_tokens ? (
            <p id="max_tokens-error" className="text-sm text-destructive">
              {errors.max_tokens}
            </p>
          ) : (
            <p id="max_tokens-hint" className="text-xs text-muted-foreground">
              Includes the thinking trace.
            </p>
          )}
        </div>

        <Field id="seed" label="Seed" optional error={errors.seed} className="sm:w-1/2">
          <Input
            id="seed"
            inputMode="numeric"
            value={form.seed}
            onChange={(e) => set("seed", e.target.value)}
            onBlur={blur("seed")}
            aria-invalid={!!errors.seed || undefined}
            aria-describedby={describe("seed", errors.seed)}
            className="tabular"
          />
        </Field>
      </Section>

      <Disclosure title="Prompt options" summary={promptSummary} open={promptOpen} onOpenChange={setPromptOpen}>
        <Field
          id="system_prompt"
          label="System prompt"
          optional
          hint="Replaces the set's system message."
          error={errors.system_prompt}
        >
          <Textarea
            id="system_prompt"
            rows={3}
            value={form.system_prompt}
            onChange={(e) => set("system_prompt", e.target.value)}
            placeholder="You are a careful assistant. Think step by step."
            aria-describedby={describe("system_prompt", errors.system_prompt, true)}
            className="min-h-20"
          />
        </Field>
        <Field
          id="user_template"
          label="Prompt template"
          optional
          hint="Must contain {prompt}."
          error={errors.user_template}
        >
          <Textarea
            id="user_template"
            rows={3}
            value={form.user_template}
            onChange={(e) => set("user_template", e.target.value)}
            onBlur={blur("user_template")}
            placeholder={"Solve this problem. Put the final answer on its own line.\n\n{prompt}"}
            aria-invalid={!!errors.user_template || undefined}
            aria-describedby={describe("user_template", errors.user_template, true)}
            className="min-h-20"
          />
        </Field>
      </Disclosure>

      <Disclosure title="Advanced" summary={advancedSummary} open={advancedOpen} onOpenChange={setAdvancedOpen}>
        <Field
          id="extra_body"
          label="Extra request body"
          optional
          hint="Merged into every request."
          error={errors.extra_body}
        >
          <Textarea
            id="extra_body"
            rows={3}
            value={form.extra_body}
            onChange={(e) => set("extra_body", e.target.value)}
            onBlur={blur("extra_body")}
            placeholder={'{"chat_template_kwargs": {"enable_thinking": true}}'}
            spellCheck={false}
            autoCapitalize="none"
            autoCorrect="off"
            aria-invalid={!!errors.extra_body || undefined}
            aria-describedby={describe("extra_body", errors.extra_body, true)}
            className="min-h-20 font-mono md:text-[13px]"
          />
        </Field>

        <div className="flex items-start gap-3">
          <Checkbox
            id="stream"
            checked={form.stream}
            onCheckedChange={(c) => set("stream", c === true)}
            aria-describedby="stream-hint"
            className="mt-0.5"
          />
          <div className="grid gap-1">
            <Label htmlFor="stream" className="text-sm font-medium">
              Stream responses
            </Label>
            <p id="stream-hint" className="text-xs text-muted-foreground">
              Needed for live progress and time to first token.
            </p>
          </div>
        </div>

        <div className="grid gap-5 sm:grid-cols-2">
          <Field id="think_open" label="Thinking opens with" error={errors.think_open}>
            <Input
              id="think_open"
              value={form.think_open}
              onChange={(e) => set("think_open", e.target.value)}
              onBlur={blur("think_open")}
              spellCheck={false}
              autoCapitalize="none"
              aria-invalid={!!errors.think_open || undefined}
              aria-describedby={describe("think_open", errors.think_open)}
              className="font-mono"
            />
          </Field>
          <Field id="think_close" label="Thinking closes with" error={errors.think_close}>
            <Input
              id="think_close"
              value={form.think_close}
              onChange={(e) => set("think_close", e.target.value)}
              onBlur={blur("think_close")}
              spellCheck={false}
              autoCapitalize="none"
              aria-invalid={!!errors.think_close || undefined}
              aria-describedby={describe("think_close", errors.think_close)}
              className="font-mono"
            />
          </Field>
          <p className="-mt-3 text-xs text-muted-foreground sm:col-span-2">
            Only for inline &lt;think&gt; traces.
          </p>
        </div>

        <div className="grid gap-5 sm:grid-cols-3">
          <Field id="max_attempts" label="Attempts per sample" error={errors.max_attempts}>
            <Input
              id="max_attempts"
              inputMode="numeric"
              value={form.max_attempts}
              onChange={(e) => set("max_attempts", e.target.value)}
              onBlur={blur("max_attempts")}
              aria-invalid={!!errors.max_attempts || undefined}
              aria-describedby={describe("max_attempts", errors.max_attempts)}
              className="tabular"
            />
          </Field>
          <Field id="timeout" label="Request timeout (s)" error={errors.timeout}>
            <Input
              id="timeout"
              inputMode="numeric"
              value={form.timeout}
              onChange={(e) => set("timeout", e.target.value)}
              onBlur={blur("timeout")}
              aria-invalid={!!errors.timeout || undefined}
              aria-describedby={describe("timeout", errors.timeout)}
              className="tabular"
            />
          </Field>
          <Field id="pause_after" label="Pause after failures in a row" hint="0 never pauses." error={errors.pause_after}>
            <Input
              id="pause_after"
              inputMode="numeric"
              value={form.pause_after}
              onChange={(e) => set("pause_after", e.target.value)}
              onBlur={blur("pause_after")}
              aria-invalid={!!errors.pause_after || undefined}
              aria-describedby={describe("pause_after", errors.pause_after, true)}
              className="tabular"
            />
          </Field>
        </div>
      </Disclosure>

      <Section id="sec-name">
        <Field id="name" label="Run name" optional error={errors.name}>
          <Input
            id="name"
            value={form.name}
            onChange={(e) => set("name", e.target.value)}
            placeholder={defaultName || "Qwen3 distill, round 2"}
            maxLength={300}
            aria-describedby={describe("name", errors.name)}
          />
        </Field>
      </Section>

      <div className="mt-8 grid gap-4 border-t pt-6">
        {formError ? <ErrorBlock message={formError} /> : null}
        <div className="flex flex-wrap items-center gap-3">
          <Button type="submit" size="lg" aria-disabled={start.isPending || undefined} className="px-4">
            {start.isPending ? "Starting…" : "Start run"}
          </Button>
          <Button asChild variant="ghost" size="lg">
            <Link to="/runs">Cancel</Link>
          </Button>
          <span className="text-sm text-muted-foreground tabular" aria-live="polite">
            {total}
          </span>
        </div>
      </div>
    </form>
  )
}
