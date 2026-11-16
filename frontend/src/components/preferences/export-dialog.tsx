import { useRef, useState, type FormEvent, type ReactNode } from "react"
import { Link } from "react-router"
import { keepPreviousData, useMutation, useQuery } from "@tanstack/react-query"
import { Download } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Field } from "@/components/shared/field"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { api, ApiError, qs } from "@/lib/api"
import { count, int } from "@/lib/format"
import type {
  ExportPreview,
  FileType,
  HubPushOut,
  PrefExportOptions,
  PrefFormat,
  PreferenceSet,
  ReasoningPlacement,
  Settings,
} from "@/lib/types"

const FORMATS: { value: PrefFormat; label: string; hint: string }[] = [
  { value: "dpo", label: "DPO", hint: "TRL prompt, chosen and rejected. One record per pair." },
  { value: "kto", label: "KTO", hint: "Prompt, completion and label. Two records per pair: chosen true, rejected false." },
]

const SHAPES: { value: "conversational" | "plain"; label: string; hint: string }[] = [
  { value: "conversational", label: "Conversational", hint: "Message lists, as TRL's chat template expects." },
  { value: "plain", label: "Plain text", hint: "Strings, with the prompt as its last user turn." },
]

const REASONING: { value: ReasoningPlacement; label: ReactNode }[] = [
  { value: "think_tags", label: <>Inline <code className="font-mono text-[13px]">&lt;think&gt;</code> tags</> },
  { value: "reasoning_content", label: <code className="font-mono text-[13px]">reasoning_content</code> },
  { value: "thinking", label: <code className="font-mono text-[13px]">thinking</code> },
  { value: "omit", label: "Leave out" },
]

const FILE_TYPES: { value: FileType; label: string }[] = [
  { value: "jsonl", label: "JSONL" },
  { value: "json", label: "JSON" },
  { value: "parquet", label: "Parquet" },
  { value: "csv", label: "CSV" },
]

const SKIP_REASON: Record<string, string> = {
  truncated: "truncated",
  failed: "failed",
  missing: "missing a response",
  no_reasoning: "without a thinking trace",
}

export function ExportPairsDialog({
  set,
  open,
  onOpenChange,
}: {
  set: PreferenceSet
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto overscroll-contain sm:max-w-3xl">
        {open ? <ExportBody set={set} /> : null}
      </DialogContent>
    </Dialog>
  )
}

function Choice({ id, value, label, hint }: { id: string; value: string; label: ReactNode; hint?: string }) {
  return (
    <div className="flex items-start gap-2.5">
      <RadioGroupItem id={id} value={value} className="mt-0.5" aria-describedby={hint ? `${id}-hint` : undefined} />
      <div className="grid gap-0.5">
        <Label htmlFor={id} className="font-normal">
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

function Group({ legend, children }: { legend: string; children: ReactNode }) {
  return (
    <fieldset className="grid min-w-0 gap-2">
      <legend className="mb-1 text-sm font-medium">{legend}</legend>
      {children}
    </fieldset>
  )
}

function ExportBody({ set }: { set: PreferenceSet }) {
  const [options, setOptions] = useState<PrefExportOptions>({
    format: "dpo",
    file_type: "jsonl",
    reasoning: "think_tags",
    include_system: true,
    conversational: true,
  })
  const update = (patch: Partial<PrefExportOptions>) => setOptions((o) => ({ ...o, ...patch }))

  const preview = useQuery({
    queryKey: ["preference-export-preview", set.id, options],
    queryFn: () => api.post<ExportPreview>(`/api/preference-sets/${set.id}/export/preview`, options),
    placeholderData: keepPreviousData,
  })
  const downloadUrl = `/api/preference-sets/${set.id}/export${qs({ ...options })}`
  const empty = preview.data?.count === 0

  return (
    <div className="grid gap-5">
      <DialogHeader>
        <DialogTitle>Export pairs</DialogTitle>
        <DialogDescription>
          {int(set.decided)} of {int(set.prompt_count)} prompts in {set.name} have a pair. Only decided prompts
          are exported.
        </DialogDescription>
      </DialogHeader>

      <div className="grid gap-6 md:grid-cols-[minmax(0,17rem)_minmax(0,1fr)]">
        <div className="grid content-start gap-5">
          <Group legend="Format">
            <RadioGroup value={options.format} onValueChange={(v) => update({ format: v as PrefFormat })} className="gap-3">
              {FORMATS.map((f) => (
                <Choice key={f.value} id={`exp-format-${f.value}`} value={f.value} label={f.label} hint={f.hint} />
              ))}
            </RadioGroup>
          </Group>

          <Group legend="Shape">
            <RadioGroup
              value={options.conversational ? "conversational" : "plain"}
              onValueChange={(v) => update({ conversational: v === "conversational" })}
              className="gap-3"
            >
              {SHAPES.map((s) => (
                <Choice key={s.value} id={`exp-shape-${s.value}`} value={s.value} label={s.label} hint={s.hint} />
              ))}
            </RadioGroup>
          </Group>

          <Group legend="Thinking trace">
            <RadioGroup
              value={options.reasoning}
              onValueChange={(v) => update({ reasoning: v as ReasoningPlacement })}
              className="gap-2.5"
            >
              {REASONING.map((r) => (
                <Choice key={r.value} id={`exp-reasoning-${r.value}`} value={r.value} label={r.label} />
              ))}
            </RadioGroup>
          </Group>

          <div className="flex items-center gap-2.5">
            <Checkbox
              id="exp-system"
              checked={options.include_system}
              onCheckedChange={(v) => update({ include_system: v === true })}
            />
            <Label htmlFor="exp-system" className="font-normal">
              Include system prompts
            </Label>
          </div>

          <Group legend="File type">
            <ToggleGroup
              type="single"
              variant="outline"
              spacing={0}
              value={options.file_type}
              onValueChange={(v) => v && update({ file_type: v as FileType })}
              aria-label="File type"
            >
              {FILE_TYPES.map((t) => (
                <ToggleGroupItem key={t.value} value={t.value} className="px-3 data-[state=on]:font-semibold">
                  {t.label}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </Group>
        </div>

        <div className="grid min-w-0 content-start gap-3">
          <h3 className="text-sm font-medium">Preview</h3>
          {preview.isPending ? (
            <div className="grid gap-2" aria-label="Loading preview…">
              <Bar className="h-5 w-40" />
              <Bar className="h-64 w-full" />
            </div>
          ) : preview.isError ? (
            <ErrorBlock message={`Couldn't build the preview. ${preview.error.message}`} onRetry={() => preview.refetch()} />
          ) : (
            <>
              <p className="text-sm" aria-live="polite">
                <span className="tabular font-medium">{count(preview.data.count, "record", "records")}</span>
                <span className="text-muted-foreground">
                  {" "}
                  in <span className="font-mono text-[13px]">{preview.data.filename}</span>
                </span>
              </p>
              {Object.entries(preview.data.skipped).some(([, n]) => n > 0) ? (
                <p className="text-xs text-muted-foreground">
                  Left out:{" "}
                  {Object.entries(preview.data.skipped)
                    .filter(([, n]) => n > 0)
                    .map(([reason, n]) => `${int(n)} ${SKIP_REASON[reason] ?? reason.replace(/_/g, " ")}`)
                    .join(", ")}
                  .
                </p>
              ) : null}
              {preview.data.example ? (
                <div className="grid gap-1.5">
                  <p className="text-xs text-muted-foreground">First record, exactly as written</p>
                  <pre
                    tabIndex={0}
                    aria-label="First record"
                    className="max-h-80 overflow-auto rounded-md bg-muted px-3 py-2.5 font-mono text-[13px] leading-5 whitespace-pre-wrap break-words"
                  >
                    {JSON.stringify(preview.data.example, null, 2)}
                  </pre>
                </div>
              ) : (
                <p className="rounded-md bg-muted px-3 py-4 text-sm text-muted-foreground">
                  No pairs to export yet. Pick a chosen and a rejected response for at least one prompt.
                </p>
              )}
            </>
          )}

          <div className="flex flex-wrap items-center gap-3 pt-1">
            {empty || preview.isPending || preview.isError ? (
              <>
                <Button disabled aria-describedby="exp-download-reason">
                  <Download aria-hidden="true" />
                  Download file
                </Button>
                <span id="exp-download-reason" className="text-xs text-muted-foreground">
                  {empty ? "Nothing to download until a prompt is decided." : "Waiting for the preview."}
                </span>
              </>
            ) : (
              <Button asChild>
                <a href={downloadUrl} download={preview.data?.filename}>
                  <Download aria-hidden="true" />
                  Download {options.file_type.toUpperCase()}
                </a>
              </Button>
            )}
          </div>
        </div>
      </div>

      <HubPush setId={set.id} options={options} disabled={empty} />
    </div>
  )
}

function HubPush({ setId, options, disabled }: { setId: number; options: PrefExportOptions; disabled: boolean }) {
  const settings = useQuery({ queryKey: ["settings"], queryFn: () => api.get<Settings>("/api/settings") })
  const [repo, setRepo] = useState("")
  const [isPrivate, setIsPrivate] = useState(true)
  const [repoError, setRepoError] = useState<string | null>(null)
  const repoRef = useRef<HTMLInputElement>(null)

  const push = useMutation({
    mutationFn: () =>
      api.post<HubPushOut>(`/api/preference-sets/${setId}/export/hub`, {
        options,
        target: { repo_id: repo.trim(), private: isPrivate, split: "train" },
      }),
    onError: (err) => {
      const msg = err instanceof ApiError ? (err.fields["target.repo_id"] ?? err.fields.repo_id) : undefined
      if (msg) {
        setRepoError(msg)
        repoRef.current?.focus()
      }
    },
  })

  function submit(e: FormEvent) {
    e.preventDefault()
    if (push.isPending) return
    const value = repo.trim()
    if (!/^[\w.-]+\/[\w.-]+$/.test(value)) {
      setRepoError("Enter the dataset as owner/name, for example your-name/math-dpo-pairs.")
      repoRef.current?.focus()
      return
    }
    setRepoError(null)
    push.mutate()
  }

  return (
    <section aria-labelledby="hub-heading" className="grid gap-3 border-t pt-4">
      <h3 id="hub-heading" className="text-sm font-medium">
        Push to Hugging Face
      </h3>
      {settings.isPending ? (
        <Bar className="h-8 w-full max-w-md" />
      ) : settings.isError ? (
        <ErrorBlock message={`Couldn't check for a Hugging Face token. ${settings.error.message}`} onRetry={() => settings.refetch()} />
      ) : !settings.data.hf_token_set ? (
        <p className="text-sm text-muted-foreground">
          To push the pairs as a dataset, add a Hugging Face token in{" "}
          <Link to="/settings" className="font-medium text-foreground underline">
            Settings
          </Link>
          .
        </p>
      ) : (
        <form onSubmit={submit} noValidate className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
          <Field id="hub-repo" label="Dataset repository" error={repoError} hint="Created if it doesn't exist. The pairs go to the train split.">
            <Input
              id="hub-repo"
              ref={repoRef}
              value={repo}
              onChange={(e) => {
                setRepo(e.target.value)
                if (repoError) setRepoError(null)
              }}
              placeholder="your-name/math-dpo-pairs"
              autoComplete="off"
              spellCheck={false}
              autoCapitalize="none"
              aria-invalid={repoError ? true : undefined}
              aria-describedby={repoError ? "hub-repo-error" : "hub-repo-hint"}
            />
          </Field>
          <div className="flex items-center gap-3 sm:pt-6">
            <div className="flex items-center gap-2">
              <Checkbox id="hub-private" checked={isPrivate} onCheckedChange={(v) => setIsPrivate(v === true)} />
              <Label htmlFor="hub-private" className="font-normal">
                Private
              </Label>
            </div>
            <Button type="submit" variant="outline" disabled={push.isPending || disabled}>
              {push.isPending ? "Pushing…" : "Push dataset"}
            </Button>
          </div>
          {push.isSuccess ? (
            <p className="text-sm sm:col-span-2" role="status">
              Pushed {count(push.data.count, "record", "records")} to{" "}
              <a href={push.data.url} target="_blank" rel="noreferrer" className="font-medium underline">
                {repo.trim()}
              </a>
              .
            </p>
          ) : null}
          {push.isError && !repoError ? <ErrorBlock className="sm:col-span-2" message={push.error.message} /> : null}
        </form>
      )}
    </section>
  )
}
