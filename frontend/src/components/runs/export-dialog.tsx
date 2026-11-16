import { useEffect, useId, useRef, useState, type ReactNode } from "react"
import { Link } from "react-router"
import { keepPreviousData, useMutation, useQuery } from "@tanstack/react-query"
import { Download, ExternalLink } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Field } from "@/components/shared/field"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { ApiError, api, qs } from "@/lib/api"
import { count, int } from "@/lib/format"
import type { ExportOptions, ExportPreview, FileType, HubPushIn, HubPushOut, ReasoningPlacement, Run, SFTFormat } from "@/lib/types"
import { cn } from "@/lib/utils"
import { SEGMENT_ON, useSettings } from "./run-data"

const FORMATS: { value: SFTFormat; label: string; body: ReactNode }[] = [
  {
    value: "messages",
    label: "Chat messages",
    body: (
      <>
        OpenAI and Hugging Face <code className="font-mono text-[0.92em]">messages</code>. For SFT with TRL, Axolotl or Unsloth.
      </>
    ),
  },
  {
    value: "sharegpt",
    label: "ShareGPT",
    body: (
      <>
        <code className="font-mono text-[0.92em]">conversations</code> with from and value. For LLaMA-Factory and Axolotl.
      </>
    ),
  },
  { value: "alpaca", label: "Alpaca", body: "Instruction, input and output fields." },
  { value: "prompt_completion", label: "Prompt-completion", body: "Prompt and completion fields, for TRL." },
  { value: "raw", label: "Raw", body: "Every field, with tokens and timings, for your own pipeline." },
]

const PLACEMENTS: { value: ReasoningPlacement; label: ReactNode; body: string }[] = [
  {
    value: "think_tags",
    label: (
      <>
        Inline <code className="font-mono text-[0.92em]">&lt;think&gt;</code> tags
      </>
    ),
    body: "The trace goes before the answer, inside the assistant text.",
  },
  {
    value: "reasoning_content",
    label: (
      <>
        <code className="font-mono text-[0.92em]">reasoning_content</code> field
      </>
    ),
    body: "A separate field, as vLLM and DeepSeek return it.",
  },
  {
    value: "thinking",
    label: (
      <>
        <code className="font-mono text-[0.92em]">thinking</code> field
      </>
    ),
    body: "A separate field, as gpt-oss chat templates expect.",
  },
  { value: "omit", label: "Leave out", body: "Export the answer only." },
]

const FILE_TYPES: { value: FileType; label: string }[] = [
  { value: "jsonl", label: "JSONL" },
  { value: "json", label: "JSON" },
  { value: "parquet", label: "Parquet" },
  { value: "csv", label: "CSV" },
]

const SKIP_WORDS: Record<string, string> = {
  truncated: "truncated",
  failed: "failed",
  pending: "not finished",
  no_reasoning: "without a thinking trace",
  extra_samples: "extra samples per prompt",
  empty_response: "empty",
}

const DEFAULT_OPTIONS: ExportOptions = {
  format: "messages",
  file_type: "jsonl",
  reasoning: "think_tags",
  include_system: true,
  exclude_truncated: true,
  require_reasoning: false,
  first_sample_only: false,
}

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value)
  useEffect(() => {
    const id = window.setTimeout(() => setV(value), ms)
    return () => window.clearTimeout(id)
  }, [value, ms])
  return v
}

function skippedText(skipped: Record<string, number>): string {
  const parts = Object.entries(skipped)
    .filter(([, n]) => n > 0)
    .map(([k, n]) => `${int(n)} ${SKIP_WORDS[k] ?? k.replace(/_/g, " ")}`)
  return parts.length ? `Skipped: ${parts.join(", ")}.` : "Nothing skipped."
}

function Choice({
  id,
  value,
  label,
  body,
}: {
  id: string
  value: string
  label: ReactNode
  body: ReactNode
}) {
  return (
    <div className="flex items-start gap-3">
      <RadioGroupItem id={id} value={value} className="mt-0.5" />
      <Label htmlFor={id} className="grid gap-0.5 font-normal leading-snug">
        <span className="text-sm font-medium">{label}</span>
        <span className="text-xs text-muted-foreground">{body}</span>
      </Label>
    </div>
  )
}

function Check({
  id,
  checked,
  onChange,
  label,
  hint,
  disabled,
}: {
  id: string
  checked: boolean
  onChange: (v: boolean) => void
  label: string
  hint?: string
  disabled?: boolean
}) {
  return (
    <div className="flex items-start gap-3">
      <Checkbox
        id={id}
        checked={checked}
        disabled={disabled}
        onCheckedChange={(c) => onChange(c === true)}
        aria-describedby={hint ? `${id}-hint` : undefined}
        className="mt-0.5"
      />
      <div className="grid gap-0.5">
        <Label htmlFor={id} className="text-sm font-normal leading-snug">
          {label}
        </Label>
        {hint ? (
          <p id={`${id}-hint`} className="text-xs text-muted-foreground">
            {hint}
          </p>
        ) : null}
      </div>
    </div>
  )
}

function HubPush({ run, options }: { run: Run; options: ExportOptions }) {
  const settings = useSettings()
  const [target, setTarget] = useState<HubPushIn>({ repo_id: "", private: true, split: "train" })
  const [errors, setErrors] = useState<{ repo_id?: string; split?: string }>({})
  const repoRef = useRef<HTMLInputElement>(null)
  const push = useMutation({
    mutationFn: () => api.post<HubPushOut>(`/api/runs/${run.id}/export/hub`, { options, target }),
    onError: (e: Error) => {
      if (e instanceof ApiError) {
        const f = e.fields
        const next = { repo_id: f["target.repo_id"], split: f["target.split"] }
        setErrors(next)
        if (next.repo_id) repoRef.current?.focus()
      }
    },
  })

  useEffect(() => {
    repoRef.current?.focus()
  }, [settings.isSuccess])

  if (settings.isPending) return <Bar className="h-8 w-full" />
  if (settings.isError) return <ErrorBlock message={`Couldn't check for a Hugging Face token. ${settings.error.message}`} onRetry={() => void settings.refetch()} />
  if (!settings.data.hf_token_set) {
    return (
      <p className="text-sm text-muted-foreground">
        Pushing needs a Hugging Face token with write access.{" "}
        <Link to="/settings" className="text-foreground underline underline-offset-4">
          Add a token in Settings
        </Link>
      </p>
    )
  }

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (push.isPending) return
    const repo = target.repo_id.trim()
    const next: typeof errors = {}
    if (!/^[\w.-]+\/[\w.-]+$/.test(repo)) next.repo_id = "Enter the repo id as owner/name, like your-name/qwen3-math-distill."
    if (!/^[\w.-]+$/.test(target.split.trim())) next.split = "Use letters, numbers, dots, dashes and underscores."
    setErrors(next)
    if (next.repo_id) {
      repoRef.current?.focus()
      return
    }
    if (next.split) return
    push.mutate()
  }

  const fieldErr = push.error instanceof ApiError && Object.keys(push.error.fields).length > 0

  return (
    <form onSubmit={submit} noValidate className="grid gap-4">
      <div className="grid items-start gap-4 sm:grid-cols-[minmax(0,1fr)_10rem]">
        <Field id="hub-repo" label="Dataset repo" hint="Created if it does not exist." error={errors.repo_id}>
          <Input
            ref={repoRef}
            id="hub-repo"
            value={target.repo_id}
            onChange={(e) => setTarget((t) => ({ ...t, repo_id: e.target.value }))}
            placeholder="your-name/qwen3-math-distill"
            spellCheck={false}
            autoCapitalize="none"
            autoCorrect="off"
            aria-invalid={!!errors.repo_id || undefined}
            aria-describedby={errors.repo_id ? "hub-repo-error" : "hub-repo-hint"}
            className="font-mono pointer-coarse:h-11"
          />
        </Field>
        <Field id="hub-split" label="Split" error={errors.split}>
          <Input
            id="hub-split"
            value={target.split}
            onChange={(e) => setTarget((t) => ({ ...t, split: e.target.value }))}
            spellCheck={false}
            autoCapitalize="none"
            aria-invalid={!!errors.split || undefined}
            aria-describedby={errors.split ? "hub-split-error" : undefined}
            className="font-mono pointer-coarse:h-11"
          />
        </Field>
      </div>
      <Check
        id="hub-private"
        checked={target.private}
        onChange={(v) => setTarget((t) => ({ ...t, private: v }))}
        label="Private dataset"
        hint="Only you and your organization can see it."
      />
      {push.isError && !fieldErr ? <ErrorBlock message={push.error.message} /> : null}
      {push.isSuccess ? (
        <p role="status" className="rounded-lg bg-positive-tint px-3 py-2 text-sm text-positive-ink">
          Pushed {count(push.data.count, "record", "records")} to{" "}
          <a href={push.data.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 font-medium underline underline-offset-4">
            {push.data.url.replace(/^https?:\/\/(www\.)?huggingface\.co\/datasets\//, "")}
            <ExternalLink className="size-3.5" aria-hidden="true" />
            <span className="sr-only">(opens in a new tab)</span>
          </a>
        </p>
      ) : null}
      <div>
        <Button type="submit" variant="outline" aria-disabled={push.isPending || undefined} className="pointer-coarse:h-11">
          {push.isPending ? "Pushing…" : "Push dataset"}
        </Button>
      </div>
    </form>
  )
}

/**
 * Export a run as a training dataset. Every option change refreshes a preview of the record count,
 * what is skipped and why, and the first record exactly as it will be written.
 */
export function ExportDialog({ run, open, onOpenChange }: { run: Run; open: boolean; onOpenChange: (open: boolean) => void }) {
  const [options, setOptions] = useState<ExportOptions>(DEFAULT_OPTIONS)
  const [hubOpen, setHubOpen] = useState(false)
  const hubRef = useRef<HTMLDivElement>(null)
  const uid = useId()
  const debounced = useDebounced(options, 250)
  const preview = useQuery({
    queryKey: ["runs", run.id, "export-preview", debounced],
    queryFn: () => api.post<ExportPreview>(`/api/runs/${run.id}/export/preview`, debounced),
    enabled: open,
    placeholderData: keepPreviousData,
  })
  const set = <K extends keyof ExportOptions>(k: K, v: ExportOptions[K]) => setOptions((o) => ({ ...o, [k]: v }))
  const href = `/api/runs/${run.id}/export${qs({ ...options })}`
  const stale = preview.isFetching || debounced !== options
  const singleSample = run.params.samples_per_prompt === 1

  useEffect(() => {
    if (hubOpen) hubRef.current?.scrollIntoView({ block: "nearest" })
  }, [hubOpen])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[60rem]">
        <DialogHeader className="border-b px-4 py-4 pr-12 sm:px-6">
          <DialogTitle className="text-base font-semibold">Export dataset</DialogTitle>
          <DialogDescription>Write this run's samples in the format your trainer expects.</DialogDescription>
        </DialogHeader>

        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
          <div className="grid gap-8 px-4 py-5 sm:px-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
            <div className="grid content-start gap-7">
              <fieldset className="grid gap-3">
                <legend className="mb-3 text-sm font-semibold">Format</legend>
                <RadioGroup value={options.format} onValueChange={(v) => set("format", v as SFTFormat)} className="gap-3">
                  {FORMATS.map((f) => (
                    <Choice key={f.value} id={`${uid}-fmt-${f.value}`} value={f.value} label={f.label} body={f.body} />
                  ))}
                </RadioGroup>
              </fieldset>

              <fieldset className="grid gap-3">
                <legend className="mb-3 text-sm font-semibold">Thinking trace</legend>
                <RadioGroup value={options.reasoning} onValueChange={(v) => set("reasoning", v as ReasoningPlacement)} className="gap-3">
                  {PLACEMENTS.map((p) => (
                    <Choice key={p.value} id={`${uid}-rsn-${p.value}`} value={p.value} label={p.label} body={p.body} />
                  ))}
                </RadioGroup>
              </fieldset>

              <div className="grid gap-3">
                <span id={`${uid}-ft`} className="text-sm font-semibold">
                  File type
                </span>
                <ToggleGroup
                  type="single"
                  variant="outline"
                  spacing={0}
                  value={options.file_type}
                  onValueChange={(v) => v && set("file_type", v as FileType)}
                  aria-labelledby={`${uid}-ft`}
                >
                  {FILE_TYPES.map((t) => (
                    <ToggleGroupItem key={t.value} value={t.value} className={cn("px-3 pointer-coarse:h-11", SEGMENT_ON)}>
                      {t.label}
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>
              </div>

              <fieldset className="grid gap-3">
                <legend className="mb-3 text-sm font-semibold">Samples</legend>
                <Check
                  id={`${uid}-trunc`}
                  checked={options.exclude_truncated}
                  onChange={(v) => set("exclude_truncated", v)}
                  label="Skip truncated responses"
                  hint="They hit the max tokens limit and stop mid-answer."
                />
                <Check
                  id={`${uid}-reason`}
                  checked={options.require_reasoning}
                  onChange={(v) => set("require_reasoning", v)}
                  label="Only samples with a thinking trace"
                />
                <Check
                  id={`${uid}-first`}
                  checked={options.first_sample_only}
                  onChange={(v) => set("first_sample_only", v)}
                  label="First sample per prompt only"
                  disabled={singleSample}
                  hint={singleSample ? "This run takes one sample per prompt." : `This run takes ${int(run.params.samples_per_prompt)} per prompt.`}
                />
                <Check
                  id={`${uid}-system`}
                  checked={options.include_system}
                  onChange={(v) => set("include_system", v)}
                  label="Include system prompt"
                />
              </fieldset>
            </div>

            <div className="grid min-w-0 content-start gap-3 lg:sticky lg:top-5 lg:self-start" aria-busy={stale || undefined}>
              <h3 className="text-sm font-semibold">Preview</h3>
              {preview.isError && !preview.data ? (
                <ErrorBlock message={`Couldn't build the preview. ${preview.error.message}`} onRetry={() => void preview.refetch()} />
              ) : !preview.data ? (
                <div className="grid gap-2" aria-label="Loading preview">
                  <Bar className="h-7 w-40" />
                  <Bar className="w-56" />
                  <Bar className="h-64 w-full rounded-lg" />
                </div>
              ) : (
                <div className={cn("grid gap-3 transition-opacity duration-150", stale && "opacity-60")}>
                  <div aria-live="polite">
                    <p className="text-xl font-medium tabular">{count(preview.data.count, "record", "records")}</p>
                    <p className="text-sm text-muted-foreground tabular">{skippedText(preview.data.skipped)}</p>
                  </div>
                  {preview.isError ? <ErrorBlock message={`Couldn't refresh the preview. ${preview.error.message}`} onRetry={() => void preview.refetch()} /> : null}
                  <div className="grid gap-1.5">
                    <p className="text-xs text-muted-foreground">First record</p>
                    {preview.data.example ? (
                      <pre
                        tabIndex={0}
                        aria-label="First record, as JSON"
                        className="max-h-[24rem] overflow-auto rounded-lg bg-muted p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]"
                      >
                        {JSON.stringify(preview.data.example, null, 2)}
                      </pre>
                    ) : (
                      <p className="rounded-lg bg-muted px-3 py-6 text-sm text-muted-foreground">
                        No samples match these options yet. Finished samples appear here as the run goes.
                      </p>
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>

          {hubOpen ? (
            <div ref={hubRef} className="border-t px-4 py-5 sm:px-6">
              <h3 className="mb-4 text-sm font-semibold">Push to Hugging Face</h3>
              <HubPush run={run} options={options} />
            </div>
          ) : null}
        </div>

        <div className="flex flex-col-reverse gap-2 border-t bg-muted/50 px-4 py-3 sm:flex-row sm:items-center sm:justify-end sm:px-6">
          {/* On narrow screens the preview sits below the options, so its answer is repeated here. */}
          {preview.data ? (
            <p className="order-last text-sm text-muted-foreground tabular sm:order-first sm:mr-auto lg:hidden" aria-hidden="true">
              <span className="font-medium text-foreground">{count(preview.data.count, "record", "records")}</span>.{" "}
              {skippedText(preview.data.skipped)}
            </p>
          ) : null}
          <Button type="button" variant="outline" aria-expanded={hubOpen} onClick={() => setHubOpen((o) => !o)} className="pointer-coarse:h-11">
            {hubOpen ? "Hide Hugging Face push" : "Push to Hugging Face"}
          </Button>
          {preview.data && preview.data.count > 0 ? (
            <Button asChild className="min-w-0 pointer-coarse:h-11">
              <a href={href} download={preview.data.filename} title={preview.data.filename}>
                <Download aria-hidden="true" />
                <span className="truncate">
                  Download <span className="font-mono">{preview.data.filename}</span>
                </span>
              </a>
            </Button>
          ) : (
            <Button type="button" disabled className="pointer-coarse:h-11" title="No records match these options.">
              <Download aria-hidden="true" />
              Download
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
