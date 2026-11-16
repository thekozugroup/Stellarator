import { useId, useRef, useState, type FormEvent, type ReactNode } from "react"
import { Link } from "react-router"
import { useMutation, useQuery, type UseQueryResult } from "@tanstack/react-query"
import { Download, ExternalLink } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Field } from "@/components/shared/field"
import { SEGMENT_ON } from "@/components/runs/run-data"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogClose, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { ApiError, api } from "@/lib/api"
import { count, int } from "@/lib/format"
import type { ExportPreview, FileType, HubPushIn, HubPushOut, ReasoningPlacement, Settings } from "@/lib/types"
import { cn } from "@/lib/utils"

// ------------------------------------------------------------------ shared words

export const FILE_TYPES: { value: FileType; label: string }[] = [
  { value: "jsonl", label: "JSONL" },
  { value: "json", label: "JSON" },
  { value: "parquet", label: "Parquet" },
  { value: "csv", label: "CSV" },
]

const mono = (s: string) => <code className="font-mono text-[0.92em]">{s}</code>

export const PLACEMENT_LABEL: Record<ReasoningPlacement, ReactNode> = {
  think_tags: <>Inline {mono("<think>")} tags</>,
  reasoning_content: <>{mono("reasoning_content")} field</>,
  thinking: <>{mono("thinking")} field</>,
  omit: "Leave out",
}

/** Skip reasons from the preview, in words. */
const SKIP_WORDS: Record<string, string> = {
  truncated: "truncated",
  failed: "failed",
  pending: "not finished",
  missing: "missing a response",
  no_reasoning: "without a thinking trace",
  extra_samples: "extra samples per prompt",
  empty_response: "empty",
}

/** "Skipped: 10 failed, 3 truncated." or nothing when nothing was skipped. */
function skippedText(skipped: Record<string, number>): string | null {
  const parts = Object.entries(skipped)
    .filter(([, n]) => n > 0)
    .map(([k, n]) => `${int(n)} ${SKIP_WORDS[k] ?? k.replace(/_/g, " ")}`)
  return parts.length ? `Skipped: ${parts.join(", ")}.` : null
}

// ------------------------------------------------------------------ option pieces

export function OptionGroup({ legend, children }: { legend: string; children: ReactNode }) {
  return (
    <fieldset className="grid min-w-0 gap-3 pointer-coarse:gap-0">
      <legend className="mb-3 text-sm font-semibold pointer-coarse:mb-1">{legend}</legend>
      {children}
    </fieldset>
  )
}

/**
 * One option as a whole-row label: the dot or box, its name and an optional one-line hint. The row
 * takes the tap, and on touch screens it is at least 44 px tall.
 */
function OptionRow({ id, label, hint, control }: { id: string; label: ReactNode; hint?: ReactNode; control: ReactNode }) {
  return (
    <label htmlFor={id} className="flex items-start gap-3 pointer-coarse:min-h-11 pointer-coarse:py-3 has-disabled:opacity-50">
      {control}
      <span className="grid gap-0.5">
        <span id={`${id}-label`} className="text-sm leading-snug">
          {label}
        </span>
        {hint ? (
          <span id={`${id}-hint`} className="text-xs text-muted-foreground">
            {hint}
          </span>
        ) : null}
      </span>
    </label>
  )
}

/** Radio choices with a label and an optional one-line hint each. */
export function ChoiceGroup<T extends string>({
  legend,
  value,
  onChange,
  choices,
}: {
  legend: string
  value: T
  onChange: (v: T) => void
  choices: { value: T; label: ReactNode; hint?: ReactNode }[]
}) {
  const uid = useId()
  return (
    <OptionGroup legend={legend}>
      <RadioGroup value={value} onValueChange={(v) => onChange(v as T)} className="gap-3 pointer-coarse:gap-0">
        {choices.map((c) => {
          const id = `${uid}-${c.value}`
          return (
            <OptionRow
              key={c.value}
              id={id}
              label={<span className="font-medium">{c.label}</span>}
              hint={c.hint}
              control={
                <RadioGroupItem
                  id={id}
                  value={c.value}
                  className="mt-0.5"
                  aria-labelledby={`${id}-label`}
                  aria-describedby={c.hint ? `${id}-hint` : undefined}
                />
              }
            />
          )
        })}
      </RadioGroup>
    </OptionGroup>
  )
}

export function CheckOption({
  checked,
  onChange,
  label,
  hint,
  disabled,
}: {
  checked: boolean
  onChange: (v: boolean) => void
  label: string
  hint?: string
  disabled?: boolean
}) {
  const id = useId()
  return (
    <OptionRow
      id={id}
      label={label}
      hint={hint}
      control={
        <Checkbox
          id={id}
          checked={checked}
          disabled={disabled}
          onCheckedChange={(c) => onChange(c === true)}
          aria-labelledby={`${id}-label`}
          aria-describedby={hint ? `${id}-hint` : undefined}
          className="mt-0.5"
        />
      }
    />
  )
}

export function FileTypeGroup({ value, onChange }: { value: FileType; onChange: (v: FileType) => void }) {
  const id = useId()
  return (
    <div className="grid gap-3">
      <span id={id} className="text-sm font-semibold">
        File type
      </span>
      <ToggleGroup
        type="single"
        variant="outline"
        spacing={0}
        value={value}
        onValueChange={(v) => v && onChange(v as FileType)}
        aria-labelledby={id}
      >
        {FILE_TYPES.map((t) => (
          <ToggleGroupItem key={t.value} value={t.value} className={cn("px-3", SEGMENT_ON)}>
            {t.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </div>
  )
}

// ------------------------------------------------------------------ Hugging Face push

function HubPush({
  push,
  repoExample,
  disabledReasonId,
}: {
  push: (target: HubPushIn) => Promise<HubPushOut>
  repoExample: string
  /** Set when there is nothing to push: the id of the sentence that says so. */
  disabledReasonId?: string
}) {
  const settings = useQuery({ queryKey: ["settings"], queryFn: () => api.get<Settings>("/api/settings") })
  const uid = useId()
  const repoId = `${uid}-repo`
  const splitId = `${uid}-split`
  const [target, setTarget] = useState<HubPushIn>({ repo_id: "", private: true, split: "train" })
  const [errors, setErrors] = useState<{ repo_id?: string; split?: string }>({})
  const repoRef = useRef<HTMLInputElement>(null)
  const splitRef = useRef<HTMLInputElement>(null)
  const mutation = useMutation({
    mutationFn: () => push({ ...target, repo_id: target.repo_id.trim(), split: target.split.trim() }),
    onError: (e: Error) => {
      if (!(e instanceof ApiError)) return
      const next = { repo_id: e.fields["target.repo_id"] ?? e.fields.repo_id, split: e.fields["target.split"] }
      setErrors(next)
      if (next.repo_id) repoRef.current?.focus()
      else if (next.split) splitRef.current?.focus()
    },
  })

  if (settings.isPending) return <Bar className="h-8 w-full max-w-md" />
  if (settings.isError) {
    return <ErrorBlock message={`Couldn't check for a Hugging Face token. ${settings.error.message}`} onRetry={() => void settings.refetch()} />
  }
  if (!settings.data.hf_token_set) {
    return (
      <p className="text-sm text-muted-foreground">
        Pushing needs a Hugging Face token with write access.{" "}
        <Link to="/settings" className="inline-flex items-center text-foreground underline underline-offset-4">
          Add a token in Settings
        </Link>
      </p>
    )
  }

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (mutation.isPending || disabledReasonId) return
    const next: typeof errors = {}
    if (!/^[\w.-]+\/[\w.-]+$/.test(target.repo_id.trim())) next.repo_id = `Enter the repository as owner/name, like ${repoExample}.`
    if (!/^[\w.-]+$/.test(target.split.trim())) next.split = "Use letters, numbers, dots, dashes and underscores."
    setErrors(next)
    if (next.repo_id) return repoRef.current?.focus()
    if (next.split) return splitRef.current?.focus()
    mutation.mutate()
  }
  const fieldError = mutation.error instanceof ApiError && Object.keys(mutation.error.fields).length > 0

  return (
    <form onSubmit={submit} noValidate className="grid gap-4">
      <div className="grid items-start gap-4 sm:grid-cols-[minmax(0,1fr)_10rem]">
        <Field id={repoId} label="Dataset repository" hint="Created if it does not exist." error={errors.repo_id}>
          <Input
            ref={repoRef}
            id={repoId}
            value={target.repo_id}
            onChange={(e) => {
              setTarget((t) => ({ ...t, repo_id: e.target.value }))
              if (errors.repo_id) setErrors((x) => ({ ...x, repo_id: undefined }))
            }}
            placeholder={repoExample}
            autoComplete="off"
            spellCheck={false}
            autoCapitalize="none"
            autoCorrect="off"
            aria-invalid={!!errors.repo_id || undefined}
            aria-describedby={errors.repo_id ? `${repoId}-error` : `${repoId}-hint`}
            className="font-mono"
          />
        </Field>
        <Field id={splitId} label="Split" error={errors.split}>
          <Input
            ref={splitRef}
            id={splitId}
            value={target.split}
            onChange={(e) => {
              setTarget((t) => ({ ...t, split: e.target.value }))
              if (errors.split) setErrors((x) => ({ ...x, split: undefined }))
            }}
            spellCheck={false}
            autoCapitalize="none"
            aria-invalid={!!errors.split || undefined}
            aria-describedby={errors.split ? `${splitId}-error` : undefined}
            className="font-mono"
          />
        </Field>
      </div>
      <CheckOption checked={target.private} onChange={(v) => setTarget((t) => ({ ...t, private: v }))} label="Private dataset" />
      {mutation.isError && !fieldError ? <ErrorBlock message={mutation.error.message} /> : null}
      {mutation.isSuccess ? (
        <p role="status" className="rounded-lg bg-positive-tint px-3 py-2 text-sm text-positive-ink">
          Pushed {count(mutation.data.count, "record", "records")} to{" "}
          <a href={mutation.data.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 font-medium underline underline-offset-4">
            {mutation.data.url.replace(/^https?:\/\/(www\.)?huggingface\.co\/datasets\//, "")}
            <ExternalLink className="size-3.5" aria-hidden="true" />
            <span className="sr-only">(opens in a new tab)</span>
          </a>
        </p>
      ) : null}
      <div>
        <Button
          type="submit"
          variant="outline"
          disabled={!!disabledReasonId}
          aria-describedby={disabledReasonId}
          aria-disabled={mutation.isPending || undefined}
        >
          {mutation.isPending ? "Pushing…" : "Push dataset"}
        </Button>
      </div>
    </form>
  )
}

// ------------------------------------------------------------------ the shell

/**
 * One layout for every export: the dialog's own options on the left, the result on the right (the
 * record count, what is skipped, the first record exactly as written), the Hugging Face push inline
 * below, and Cancel and Download in the footer. On phones the first record comes before the
 * options and the count sits in the footer, so the effect of each option stays in view.
 */
export function ExportDialogShell({
  title,
  open,
  onOpenChange,
  preview,
  stale = false,
  emptyText,
  downloadHref,
  fileType,
  push,
  repoExample,
  children,
}: {
  title: string
  open: boolean
  onOpenChange: (open: boolean) => void
  preview: UseQueryResult<ExportPreview>
  /** The options changed and the preview has not caught up yet. */
  stale?: boolean
  /** Shown in place of the first record when nothing matches. */
  emptyText: string
  downloadHref: string
  fileType: FileType
  push: (target: HubPushIn) => Promise<HubPushOut>
  repoExample: string
  /** The dialog's own options. */
  children: ReactNode
}) {
  const uid = useId()
  const emptyId = `${uid}-empty`
  const data = preview.data
  const skipped = data ? skippedText(data.skipped) : null
  const nothing = data?.count === 0
  const typeLabel = FILE_TYPES.find((t) => t.value === fileType)?.label ?? fileType.toUpperCase()
  // The dialog is opened from a plain button, not a Radix trigger, so focus is returned by hand.
  const returnTo = useRef<HTMLElement | null>(null)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        aria-describedby={undefined}
        onOpenAutoFocus={() => {
          returnTo.current = document.activeElement as HTMLElement | null
        }}
        onCloseAutoFocus={(e) => {
          e.preventDefault()
          returnTo.current?.focus()
        }}
        className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[56rem]">
        <DialogHeader className="border-b px-4 py-4 pr-14 sm:px-6">
          <DialogTitle className="text-base font-semibold">{title}</DialogTitle>
        </DialogHeader>

        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
          <div className="grid gap-8 px-4 py-5 sm:px-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
            <section
              aria-label="Preview"
              aria-busy={stale || undefined}
              className="grid min-w-0 content-start gap-3 lg:sticky lg:top-0 lg:col-start-2 lg:row-start-1 lg:self-start"
            >
              {preview.isError && !data ? (
                <ErrorBlock message={`Couldn't build the preview. ${preview.error.message}`} onRetry={() => void preview.refetch()} />
              ) : !data ? (
                <div className="grid gap-2" aria-label="Loading preview">
                  <Bar className="hidden h-7 w-40 lg:inline-block" />
                  <Bar className="h-40 w-full rounded-lg lg:h-64" />
                </div>
              ) : (
                <div className={cn("grid gap-3 transition-opacity duration-150", stale && "opacity-60")}>
                  <div aria-live="polite" className="hidden lg:block">
                    <p className="text-xl font-medium tabular">{count(data.count, "record", "records")}</p>
                    {skipped ? <p className="text-sm text-muted-foreground tabular">{skipped}</p> : null}
                  </div>
                  {preview.isError ? (
                    <ErrorBlock message={`Couldn't refresh the preview. ${preview.error.message}`} onRetry={() => void preview.refetch()} />
                  ) : null}
                  {data.example ? (
                    <div className="grid gap-1.5">
                      <p className="text-xs text-muted-foreground">First record</p>
                      <pre
                        tabIndex={0}
                        aria-label="First record, as JSON"
                        className="max-h-48 overflow-auto rounded-lg bg-muted p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere] lg:max-h-[24rem]"
                      >
                        {JSON.stringify(data.example, null, 2)}
                      </pre>
                    </div>
                  ) : (
                    <p id={emptyId} className="rounded-lg bg-muted px-3 py-6 text-sm text-muted-foreground">
                      {emptyText}
                    </p>
                  )}
                </div>
              )}
            </section>

            <div className="grid min-w-0 content-start gap-7 lg:col-start-1 lg:row-start-1">{children}</div>
          </div>

          <section aria-labelledby={`${uid}-hub`} className="grid gap-4 border-t px-4 py-5 sm:px-6">
            <h3 id={`${uid}-hub`} className="text-sm font-semibold">
              Push to Hugging Face
            </h3>
            <HubPush push={push} repoExample={repoExample} disabledReasonId={nothing ? emptyId : undefined} />
          </section>
        </div>

        <div className="flex flex-wrap items-center justify-end gap-x-2 gap-y-2 border-t bg-muted/50 px-4 py-3 sm:px-6">
          {data ? (
            <p aria-live="polite" className="basis-full text-sm text-muted-foreground tabular sm:mr-auto sm:basis-auto lg:hidden">
              <span className="font-medium text-foreground">{count(data.count, "record", "records")}</span>
              {skipped ? `. ${skipped}` : null}
            </p>
          ) : null}
          <DialogClose asChild>
            <Button type="button" variant="outline">
              Cancel
            </Button>
          </DialogClose>
          {data && data.count > 0 ? (
            <Button asChild>
              <a href={downloadHref} download={data.filename} title={data.filename}>
                <Download aria-hidden="true" />
                Download {typeLabel}
              </a>
            </Button>
          ) : (
            <Button type="button" disabled aria-describedby={nothing ? emptyId : undefined}>
              <Download aria-hidden="true" />
              Download {typeLabel}
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
