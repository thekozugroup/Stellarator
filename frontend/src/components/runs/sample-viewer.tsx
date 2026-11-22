import { useRef, useState, type KeyboardEvent, type ReactNode } from "react"
import { ChevronRight } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { CopyButton } from "@/components/shared/copy-button"
import { ModelText, TextModeToggle } from "@/components/shared/model-text"
import { StatusBadge } from "@/components/shared/status-pill"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { DASH, count, dateTime, duration, int } from "@/lib/format"
import type { Sample } from "@/lib/types"
import { cn } from "@/lib/utils"
import { ROLE_LABEL, finishReasonText, sampleLabel, useSample } from "./run-data"
import { StepPair } from "./step-pair"

const SHORT_TRACE = 1500

/**
 * A card in the sheet (DESIGN.md, Shape): --muted with 16 px corners and 8 px padding, a header
 * row (title, mono meta, copy) and the model text in a --background well with 8 px corners,
 * concentric with the card.
 */
const CARD = "rounded-xl bg-muted p-2"
const WELL = "rounded-sm bg-background px-3 py-2.5"

function CardHeader({ children, copy }: { children: ReactNode; copy?: { value: string; label: string } }) {
  return (
    <div className="flex min-h-8 items-center justify-between gap-3 pr-0.5 pl-2">
      {children}
      {copy ? <CopyButton value={copy.value} label={copy.label} /> : null}
    </div>
  )
}

function Part({ title, meta, copy, children }: { title: string; meta?: ReactNode; copy?: { value: string; label: string }; children: ReactNode }) {
  return (
    <section className={CARD}>
      <CardHeader copy={copy}>
        <h3 className="text-sm font-semibold">
          {title}
          {meta ? <span className="ml-2 font-mono text-xs font-normal text-muted-foreground">{meta}</span> : null}
        </h3>
      </CardHeader>
      <div className={cn(WELL, "mt-1")}>{children}</div>
    </section>
  )
}

function Thinking({ sample }: { sample: Sample }) {
  const text = sample.reasoning ?? ""
  const [open, setOpen] = useState(text.length < SHORT_TRACE)
  const meta = sample.reasoning_tokens !== null ? count(sample.reasoning_tokens, "token", "tokens") : `${int(text.length)} characters`
  return (
    <Collapsible open={open} onOpenChange={setOpen} className={CARD}>
      <CardHeader copy={{ value: text, label: "Copy thinking trace" }}>
        <h3 className="text-sm font-semibold">
          <CollapsibleTrigger className="group -mx-1 flex items-center gap-1.5 rounded-xs px-1 text-left">
            <ChevronRight
              aria-hidden="true"
              className="size-4 text-muted-foreground transition-transform duration-200 ease-out group-data-[state=open]:rotate-90"
            />
            Thinking trace
            <span className="font-mono text-xs font-normal text-muted-foreground">{meta}</span>
          </CollapsibleTrigger>
        </h3>
      </CardHeader>
      <CollapsibleContent>
        <div className={cn(WELL, "mt-1 text-muted-foreground")}>
          <ModelText text={text} />
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

function Facts({ sample }: { sample: Sample }) {
  const finish = finishReasonText(sample.finish_reason)
  const tokenParts = [
    sample.prompt_tokens !== null ? `${int(sample.prompt_tokens)} prompt` : null,
    sample.completion_tokens !== null ? `${int(sample.completion_tokens)} completion` : null,
    sample.reasoning_tokens !== null ? `${int(sample.reasoning_tokens)} reasoning` : null,
  ].filter(Boolean)
  const failed = sample.status === "failed"
  // [label, value, known, mono]: measurements and times are data, in mono; the finish reason is words.
  const all: [string, ReactNode, boolean, boolean][] = [
    ["Finish reason", finish ? <span className={finish.warn ? "text-coral-ink" : undefined}>{finish.text}</span> : DASH, !!finish, false],
    ["Tokens", tokenParts.length ? tokenParts.join(" · ") : DASH, tokenParts.length > 0, true],
    ["Latency", duration(sample.latency_ms), sample.latency_ms !== null, true],
    ["Time to first token", duration(sample.ttft_ms), sample.ttft_ms !== null, true],
    ["Attempts", int(sample.attempts), true, true],
    [failed ? "Failed" : "Completed", dateTime(sample.completed_at), !!sample.completed_at, true],
  ]
  // A failed request has no finish reason, tokens or timings: leave those rows out instead of a column of dashes.
  const rows = all.filter(([, , known]) => known || !failed)
  return (
    <dl className="grid grid-cols-[minmax(0,10rem)_minmax(0,1fr)] items-baseline gap-x-4 gap-y-2 px-2 pt-1 pb-2 text-sm">
      {rows.map(([k, v, , mono]) => (
        <div key={k} className="contents">
          <dt className="text-muted-foreground">{k}</dt>
          <dd className={mono ? "font-mono text-xs leading-5" : undefined}>{v}</dd>
        </div>
      ))}
    </dl>
  )
}

function Body({ sample }: { sample: Sample }) {
  const promptText = sample.messages.map((m) => (sample.messages.length > 1 ? `${ROLE_LABEL[m.role] ?? m.role}:\n${m.content}` : m.content)).join("\n\n")
  return (
    <div className="grid gap-2">
      <Part title="Prompt" copy={{ value: promptText, label: "Copy prompt" }}>
        <div className="grid gap-3">
          {sample.messages.map((m, i) => (
            <div key={i} className="grid gap-1">
              {/* A lone user message needs no role label. */}
              {sample.messages.length > 1 ? <span className="text-xs text-muted-foreground">{ROLE_LABEL[m.role] ?? m.role}</span> : null}
              <ModelText text={m.content} />
            </div>
          ))}
        </div>
      </Part>

      {sample.reasoning ? <Thinking key={sample.id} sample={sample} /> : null}

      <Part title="Response" copy={sample.response ? { value: sample.response, label: "Copy response" } : undefined}>
        {sample.response ? (
          <ModelText text={sample.response} />
        ) : sample.status === "failed" ? (
          <p className="text-sm text-negative-ink [overflow-wrap:anywhere]">{sample.error ?? "The request failed."}</p>
        ) : sample.status === "done" && sample.finish_reason === "length" ? (
          <p className="text-sm text-coral-ink">No answer: max tokens hit while thinking.</p>
        ) : sample.status === "done" ? (
          <p className="text-sm text-muted-foreground">The model returned an empty response.</p>
        ) : (
          <p className="text-sm text-muted-foreground">Not finished yet.</p>
        )}
      </Part>

      <section className={CARD}>
        <CardHeader>
          <h3 className="text-sm font-semibold">Details</h3>
        </CardHeader>
        <Facts sample={sample} />
      </section>
    </div>
  )
}

function Loading() {
  return (
    <div className="grid gap-2" aria-busy="true" aria-label="Loading sample">
      {["Prompt", "Response"].map((t) => (
        <section key={t} className={CARD}>
          <CardHeader>
            <h3 className="text-sm font-semibold">{t}</h3>
          </CardHeader>
          <div className={cn(WELL, "mt-1 grid gap-2")}>
            <Bar className="w-full" />
            <Bar className="w-11/12" />
            <Bar className="w-2/3" />
          </div>
        </section>
      ))}
    </div>
  )
}

/** The sample's status in the sheet's header: a capsule in the state's tint (DESIGN.md, Status). */
function SampleStatusBadge({ sample }: { sample: Sample }) {
  if (sample.status === "done" && sample.finish_reason === "length") return <StatusBadge tone="attention">Truncated</StatusBadge>
  if (sample.status === "failed") return <StatusBadge tone="negative">Failed</StatusBadge>
  if (sample.status === "running") return <StatusBadge tone="positive">In flight</StatusBadge>
  return <StatusBadge>{sample.status === "done" ? "Done" : "Pending"}</StatusBadge>
}

/**
 * One sample in full, in a side sheet whose state lives in the URL (?sample=ID). J and K step
 * through the rows of the current table page.
 */
export function SampleViewer({
  sampleId,
  samplesPerPrompt,
  prevId,
  nextId,
  onStep,
  onClose,
  onCloseAutoFocus,
}: {
  sampleId: number | null
  samplesPerPrompt: number
  prevId: number | null
  nextId: number | null
  onStep: (id: number) => void
  onClose: () => void
  /** Where focus goes when the sheet closes: the row of the sample last shown. */
  onCloseAutoFocus: (e: Event) => void
}) {
  const sample = useSample(sampleId)
  const bodyRef = useRef<HTMLDivElement>(null)
  const s = sample.data && sample.data.id === sampleId ? sample.data : undefined

  function onKeyDown(e: KeyboardEvent) {
    const t = e.target as HTMLElement
    if (e.metaKey || e.ctrlKey || e.altKey || t.closest("input, textarea, [contenteditable=true]")) return
    if (e.key === "j" && nextId !== null) {
      e.preventDefault()
      onStep(nextId)
    } else if (e.key === "k" && prevId !== null) {
      e.preventDefault()
      onStep(prevId)
    }
  }

  return (
    <Sheet open={sampleId !== null} onOpenChange={(o) => !o && onClose()}>
      <SheetContent
        side="right"
        onKeyDown={onKeyDown}
        onCloseAutoFocus={onCloseAutoFocus}
        onOpenAutoFocus={(e) => {
          // Start in the reading area, so J and K work at once and no control is picked for the user.
          // It is a scroll container, not a control, so it takes focus without a ring.
          e.preventDefault()
          bodyRef.current?.focus()
        }}
        className="gap-0 p-0 data-[side=right]:w-full data-[side=right]:sm:max-w-[45rem]"
      >
        <SheetHeader className="gap-2 border-b px-4 pt-3.5 pb-3 pr-14 sm:px-5">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <SheetTitle className="text-base font-semibold">
              Sample{" "}
              <span className="font-mono text-sm">
                {s ? sampleLabel(s.prompt_idx, s.sample_idx, samplesPerPrompt) : ""}
              </span>
            </SheetTitle>
            {s ? <SampleStatusBadge sample={s} /> : null}
          </div>
          <SheetDescription className="sr-only">
            The prompt, thinking trace and response of one sample. Press J for the next sample and K for the previous one.
          </SheetDescription>
          <div className="flex flex-wrap items-center gap-2">
            <StepPair
              prev={{ label: "Previous sample", shortcut: "k", disabled: prevId === null, onClick: () => prevId !== null && onStep(prevId) }}
              next={{ label: "Next sample", shortcut: "j", disabled: nextId === null, onClick: () => nextId !== null && onStep(nextId) }}
            />
            <TextModeToggle className="ml-auto" />
          </div>
        </SheetHeader>
        <div
          ref={bodyRef}
          tabIndex={-1}
          aria-label="Sample"
          className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-3 outline-none sm:p-4" /* taste-ignore: outline-removed -- tabIndex -1 scroll area focused only by script on open, never by Tab */
        >
          {sample.isError ? (
            <ErrorBlock message={`Couldn't load this sample. ${sample.error.message}`} onRetry={() => void sample.refetch()} />
          ) : s ? (
            <Body sample={s} />
          ) : (
            <Loading />
          )}
        </div>
      </SheetContent>
    </Sheet>
  )
}
