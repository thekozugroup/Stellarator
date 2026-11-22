import { useId, useRef, useState, type FormEvent, type ReactNode } from "react"
import { Link } from "react-router"
import { useMutation, useQuery, type UseQueryResult } from "@tanstack/react-query"
import { Download, ExternalLink } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { FormRow } from "@/components/shared/form-group"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { ApiError, api } from "@/lib/api"
import { count, int } from "@/lib/format"
import type { ExportPreview, FileType, HubPushIn, HubPushOut, ReasoningPlacement, Settings } from "@/lib/types"
import { cn } from "@/lib/utils"

/**
 * Inside the thick material the page's --muted is the same tone as the dialog (244 on 244 in light),
 * so groups and wells inside it take the ground in light and the raised tone in dark, as grouped
 * sections do on iOS sheets.
 */

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

/*
 * The options read as System Settings groups (DESIGN.md, Forms): a title in secondary text over a
 * --muted box with 16 px corners, its rows divided by hairlines inset 12 px from the box's edge.
 */

/** Inset hairline above every row but the first (as FormRow in shared/form-group). */
const ROW_SEPARATOR =
  "relative not-first:before:pointer-events-none not-first:before:absolute not-first:before:inset-x-3 not-first:before:top-0 not-first:before:h-px not-first:before:bg-border"

const GROUP_TITLE = "px-3 text-sm font-semibold text-muted-foreground"

/**
 * A FormRow with a hint under its field: once the label sits beside the field, align it with the
 * field's first line (32 px, 44 pt on touch) instead of the middle of the row.
 */
const ALIGN_TOP =
  "@md/fg:items-start @md/fg:[&>div:first-child]:min-h-8 @md/fg:pointer-coarse:[&>div:first-child]:min-h-11"

export function OptionGroup({ legend, children }: { legend: string; children: ReactNode }) {
  return (
    <fieldset className="grid min-w-0 gap-1.5">
      <legend className={cn(GROUP_TITLE, "mb-1.5")}>{legend}</legend>
      <div className="grid rounded-xl bg-muted">{children}</div>
    </fieldset>
  )
}

/**
 * One option as a whole-row label: the dot or box, its name and an optional one-line hint. The row
 * takes the click; on touch screens it is at least 44 px tall.
 */
function OptionRow({ id, label, hint, control }: { id: string; label: ReactNode; hint?: ReactNode; control: ReactNode }) {
  return (
    <label
      htmlFor={id}
      className={cn(ROW_SEPARATOR, "flex min-h-10 cursor-pointer items-start gap-3 px-3 py-2.5 pointer-coarse:min-h-11 has-disabled:cursor-default has-disabled:opacity-50")}
    >
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
      <RadioGroup value={value} onValueChange={(v) => onChange(v as T)} className="gap-0">
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

/** The file type as a full-width segmented control under its title. */
export function FileTypeGroup({ value, onChange }: { value: FileType; onChange: (v: FileType) => void }) {
  const id = useId()
  return (
    <div className="grid gap-1.5">
      <span id={id} className={cn(GROUP_TITLE, "mb-1.5")}>
        File type
      </span>
      <ToggleGroup
        type="single"
        spacing={0}
        value={value}
        onValueChange={(v) => v && onChange(v as FileType)}
        aria-labelledby={id}
        className="w-full"
      >
        {FILE_TYPES.map((t) => (
          <ToggleGroupItem key={t.value} value={t.value} className="flex-1 px-3">
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

  const frame = (body: ReactNode, after?: ReactNode) => (
    <section aria-labelledby={`${uid}-hub`} className="grid min-w-0 gap-1.5">
      <h3 id={`${uid}-hub`} className={cn(GROUP_TITLE, "mb-1.5")}>
        Push to Hugging Face
      </h3>
      {body}
      {after}
    </section>
  )

  if (settings.isPending) return frame(<Bar className="h-24 w-full rounded-xl" />)
  if (settings.isError) {
    return frame(<ErrorBlock message={`Couldn't check for a Hugging Face token. ${settings.error.message}`} onRetry={() => void settings.refetch()} />)
  }
  if (!settings.data.hf_token_set) {
    return frame(
      <p className="rounded-xl bg-muted px-3 py-3 text-sm text-muted-foreground">
        Pushing needs a Hugging Face token with write access.{" "}
        <Link to="/settings" className="inline-flex items-center rounded-xs text-foreground underline underline-offset-4 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none">
          Add a token in Settings
        </Link>
      </p>,
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

  return frame(
    <form onSubmit={submit} noValidate className="grid gap-3">
      <div className="@container/fg grid rounded-xl bg-muted">
        <FormRow id={repoId} label="Dataset repository" hint="Created if it does not exist." error={errors.repo_id} className={ALIGN_TOP}>
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
            className="font-mono pointer-fine:text-xs"
          />
        </FormRow>
        <FormRow id={splitId} label="Split" error={errors.split} className={ALIGN_TOP}>
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
            className="max-w-40 font-mono pointer-fine:text-xs"
          />
        </FormRow>
        <CheckOption checked={target.private} onChange={(v) => setTarget((t) => ({ ...t, private: v }))} label="Private dataset" />
      </div>
      {mutation.isError && !fieldError ? <ErrorBlock message={mutation.error.message} /> : null}
      {mutation.isSuccess ? (
        <p role="status" className="rounded-xl bg-positive-tint px-4 py-3 text-sm text-positive-ink">
          Pushed {count(mutation.data.count, "record", "records")} to{" "}
          <a href={mutation.data.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 font-mono text-xs font-medium underline underline-offset-4">
            {mutation.data.url.replace(/^https?:\/\/(www\.)?huggingface\.co\/datasets\//, "")}
            <ExternalLink className="size-3.5" aria-hidden="true" />
            <span className="sr-only">(opens in a new tab)</span>
          </a>
        </p>
      ) : null}
      <div>
        <Button
          type="submit"
          variant="secondary"
          disabled={!!disabledReasonId}
          aria-describedby={disabledReasonId}
          aria-disabled={mutation.isPending || undefined}
        >
          {mutation.isPending ? "Pushing…" : "Push dataset"}
        </Button>
      </div>
    </form>,
  )
}

// ------------------------------------------------------------------ the shell

/**
 * One layout for every export: the dialog's own options on the left, the result on the right (the
 * record count, what is skipped, the first record exactly as written), the Hugging Face push inline
 * below, and Cancel and Download at the end with no footer band. On phones the first record comes
 * before the options and the count sits above the buttons, so the effect of each option stays in
 * view.
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
        className={"flex max-h-[calc(100dvh-2rem)] flex-col sm:max-w-[56rem]"}
      >
        <DialogHeader className="pr-8">
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>

        {/* The scroll area runs to the dialog's edges so its scrollbar and focus rings are not clipped. */}
        <div className="-mx-5 min-h-0 flex-1 overflow-y-auto overscroll-contain px-5 pb-1">
          <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
            <section
              aria-label="Preview"
              aria-busy={stale || undefined}
              className="grid min-w-0 content-start gap-3 lg:sticky lg:top-0 lg:col-start-2 lg:row-start-1 lg:self-start"
            >
              {preview.isError && !data ? (
                <ErrorBlock message={`Couldn't build the preview. ${preview.error.message}`} onRetry={() => void preview.refetch()} />
              ) : !data ? (
                <div className="grid gap-3" aria-label="Loading preview">
                  <Bar className="hidden h-8 w-40 lg:inline-block" />
                  <Bar className="h-40 w-full rounded-xl lg:h-64" />
                </div>
              ) : (
                <div className={cn("grid gap-3 transition-opacity duration-150", stale && "opacity-60")}>
                  {/* The answer of the preview as the figure (DESIGN.md, Typography): how many records. */}
                  <div aria-live="polite" className="hidden px-1 lg:block">
                    <p className="flex items-baseline gap-1.5">
                      <span className="text-figure">{int(data.count)}</span>
                      <span className="text-sm text-muted-foreground">{data.count === 1 ? "record" : "records"}</span>
                    </p>
                    {skipped ? <p className="mt-1 text-sm text-muted-foreground tabular">{skipped}</p> : null}
                  </div>
                  {preview.isError ? (
                    <ErrorBlock message={`Couldn't refresh the preview. ${preview.error.message}`} onRetry={() => void preview.refetch()} />
                  ) : null}
                  {data.example ? (
                    <div className="grid gap-1.5">
                      <p className={GROUP_TITLE}>First record</p>
                      <pre
                        tabIndex={0}
                        aria-label="First record, as JSON"
                        className="max-h-48 overflow-auto rounded-xl bg-muted px-4 py-3 font-mono text-xs leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere] outline-none focus-visible:ring-2 focus-visible:ring-ring lg:max-h-[24rem]"
                      >
                        {JSON.stringify(data.example, null, 2)}
                      </pre>
                    </div>
                  ) : (
                    <p id={emptyId} className="rounded-xl bg-muted px-4 py-6 text-sm text-muted-foreground">
                      {emptyText}
                    </p>
                  )}
                </div>
              )}
            </section>

            <div className="grid min-w-0 content-start gap-6 lg:col-start-1 lg:row-start-1">{children}</div>
          </div>

          <div className="mt-8">
            <HubPush push={push} repoExample={repoExample} disabledReasonId={nothing ? emptyId : undefined} />
          </div>
        </div>

        {/* Phones: the count sits above the buttons, since the right-hand column is gone. */}
        {data ? (
          <p aria-live="polite" className="text-sm text-muted-foreground tabular lg:hidden">
            <span className="font-medium text-foreground">{count(data.count, "record", "records")}</span>
            {skipped ? `. ${skipped}` : null}
          </p>
        ) : null}
        <DialogFooter className="pt-0">
          <DialogClose asChild>
            <Button type="button" variant="secondary">
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
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
