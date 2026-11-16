import { useState, type KeyboardEvent, type ReactNode } from "react"
import { ChevronLeft, ChevronRight } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { CopyButton } from "@/components/shared/copy-button"
import { SampleStatusPill } from "@/components/shared/status-pill"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Kbd } from "@/components/ui/kbd"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { count, dateTime, duration, int } from "@/lib/format"
import type { Sample } from "@/lib/types"
import { ROLE_LABEL, finishReasonText, sampleLabel, useSample } from "./run-data"

const SHORT_TRACE = 1500

function Part({ title, meta, copy, children }: { title: string; meta?: ReactNode; copy?: { value: string; label: string }; children: ReactNode }) {
  return (
    <section className="grid gap-2">
      <div className="flex min-h-7 items-center justify-between gap-3">
        <h3 className="text-sm font-semibold">
          {title}
          {meta ? <span className="ml-2 font-normal text-muted-foreground tabular">{meta}</span> : null}
        </h3>
        {copy ? <CopyButton value={copy.value} label={copy.label} /> : null}
      </div>
      {children}
    </section>
  )
}

function Thinking({ sample }: { sample: Sample }) {
  const text = sample.reasoning ?? ""
  const [open, setOpen] = useState(text.length < SHORT_TRACE)
  const meta = sample.reasoning_tokens !== null ? count(sample.reasoning_tokens, "token", "tokens") : `${int(text.length)} characters`
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="flex min-h-7 items-center justify-between gap-3">
        <h3 className="text-sm font-semibold">
          <CollapsibleTrigger className="group -mx-1 flex items-center gap-1.5 rounded-md px-1 text-left">
            <ChevronRight
              aria-hidden="true"
              className="size-4 text-muted-foreground transition-transform duration-200 ease-out group-data-[state=open]:rotate-90"
            />
            Thinking trace
            <span className="font-normal text-muted-foreground tabular">{meta}</span>
          </CollapsibleTrigger>
        </h3>
        <CopyButton value={text} label="Copy thinking trace" />
      </div>
      <CollapsibleContent>
        <div className="mt-2 rounded-lg bg-muted px-3 py-2.5 text-muted-foreground">
          <p className="model-text">{text}</p>
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
  const rows: [string, ReactNode][] = [
    ["Finish reason", finish ? <span className={finish.warn ? "text-warning-ink" : undefined}>{finish.text}</span> : "–"],
    ["Tokens", tokenParts.length ? tokenParts.join(" · ") : "–"],
    ["Latency", duration(sample.latency_ms)],
    ["Time to first token", duration(sample.ttft_ms)],
    ["Attempts", int(sample.attempts)],
    ["Completed", dateTime(sample.completed_at)],
  ]
  return (
    <dl className="grid grid-cols-[minmax(0,10rem)_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
      {rows.map(([k, v]) => (
        <div key={k} className="contents">
          <dt className="text-muted-foreground">{k}</dt>
          <dd className="tabular">{v}</dd>
        </div>
      ))}
      {sample.error ? (
        <div className="contents">
          <dt className="text-muted-foreground">Error</dt>
          <dd className="text-destructive [overflow-wrap:anywhere]">{sample.error}</dd>
        </div>
      ) : null}
    </dl>
  )
}

function Body({ sample }: { sample: Sample }) {
  const promptText = sample.messages.map((m) => (sample.messages.length > 1 ? `${ROLE_LABEL[m.role] ?? m.role}:\n${m.content}` : m.content)).join("\n\n")
  return (
    <div className="grid gap-6">
      <Part title="Prompt" copy={{ value: promptText, label: "Copy prompt" }}>
        <div className="grid gap-3">
          {sample.messages.map((m, i) => (
            <div key={i} className="grid gap-1">
              <span className="text-xs text-muted-foreground">{ROLE_LABEL[m.role] ?? m.role}</span>
              <p className="model-text">{m.content}</p>
            </div>
          ))}
        </div>
      </Part>

      {sample.reasoning ? <Thinking key={sample.id} sample={sample} /> : null}

      <Part
        title="Response"
        meta={sample.completion_tokens !== null ? count(sample.completion_tokens, "token", "tokens") : undefined}
        copy={sample.response ? { value: sample.response, label: "Copy response" } : undefined}
      >
        {sample.response ? (
          <p className="model-text">{sample.response}</p>
        ) : sample.status === "failed" ? (
          <p className="text-sm text-muted-foreground">No response. The request failed after {count(sample.attempts, "attempt", "attempts")}; the error is below.</p>
        ) : sample.status === "done" ? (
          <p className="text-sm text-muted-foreground">The model returned an empty response.</p>
        ) : (
          <p className="text-sm text-muted-foreground">Not finished yet. The response appears here when the request completes.</p>
        )}
      </Part>

      <section className="grid gap-2 border-t pt-5">
        <h3 className="text-sm font-semibold">Details</h3>
        <Facts sample={sample} />
      </section>
    </div>
  )
}

function Loading() {
  return (
    <div className="grid gap-6" aria-busy="true" aria-label="Loading sample">
      {["Prompt", "Response"].map((t) => (
        <div key={t} className="grid gap-2">
          <h3 className="text-sm font-semibold">{t}</h3>
          <Bar className="w-full" />
          <Bar className="w-11/12" />
          <Bar className="w-2/3" />
        </div>
      ))}
    </div>
  )
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
}: {
  sampleId: number | null
  samplesPerPrompt: number
  prevId: number | null
  nextId: number | null
  onStep: (id: number) => void
  onClose: () => void
}) {
  const sample = useSample(sampleId)
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
        className="gap-0 p-0 data-[side=right]:w-full data-[side=right]:sm:max-w-[45rem]"
      >
        <SheetHeader className="gap-2 border-b px-4 py-3 pr-14 sm:px-6">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <SheetTitle className="text-base font-semibold">
              Sample{" "}
              <span className="font-mono text-[15px]">
                {s ? sampleLabel(s.prompt_idx, s.sample_idx, samplesPerPrompt) : ""}
              </span>
            </SheetTitle>
            {s ? <SampleStatusPill status={s.status} finishReason={s.finish_reason} /> : null}
          </div>
          <SheetDescription className="sr-only">
            The prompt, thinking trace and response of one sample. Press J for the next sample and K for the previous one.
          </SheetDescription>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={prevId === null}
              onClick={() => prevId !== null && onStep(prevId)}
              className="pointer-coarse:h-11"
            >
              <ChevronLeft aria-hidden="true" />
              Previous
              <Kbd className="ml-1 hidden sm:inline-flex">K</Kbd>
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={nextId === null}
              onClick={() => nextId !== null && onStep(nextId)}
              className="pointer-coarse:h-11"
            >
              Next
              <ChevronRight aria-hidden="true" />
              <Kbd className="ml-1 hidden sm:inline-flex">J</Kbd>
            </Button>
          </div>
        </SheetHeader>
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-5 sm:px-6">
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
