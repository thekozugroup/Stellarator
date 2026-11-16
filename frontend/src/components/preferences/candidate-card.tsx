import { Check, ChevronRight, X } from "lucide-react"
import { Pill } from "@/components/shared/status-pill"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { count, int, middle } from "@/lib/format"
import type { Candidate } from "@/lib/types"
import { cn } from "@/lib/utils"

export type Mark = "chosen" | "rejected"

const wordCount = (s: string) => (s.trim() ? s.trim().split(/\s+/).length : 0)

function traceSize(c: Candidate): string {
  if (c.reasoning_tokens != null) return count(c.reasoning_tokens, "token", "tokens")
  return count(wordCount(c.reasoning ?? ""), "word", "words")
}

/**
 * One response to the prompt. The letter is its name on screen and its key (A is 1, B is 2).
 * The header stays in view while a long response scrolls, so the choice is always one click away.
 */
export function CandidateCard({
  candidate,
  label,
  position,
  mark,
  showSample,
  disabled,
  onMark,
}: {
  candidate: Candidate
  label: string
  position: number
  mark: Mark | null
  showSample: boolean
  disabled?: boolean
  onMark: (mark: Mark) => void
}) {
  const truncated = candidate.finish_reason === "length"
  const headingId = `cand-${candidate.sample_id}`
  const sampleNote = showSample ? `, sample ${int(candidate.sample_idx + 1)}` : ""
  return (
    <article
      aria-labelledby={headingId}
      data-mark={mark ?? undefined}
      className={cn(
        "min-w-0 rounded-lg border bg-card",
        mark === "chosen" && "border-copper ring-1 ring-copper",
        mark === "rejected" && "border-input",
      )}
    >
      <header className="sticky top-14 z-10 flex flex-wrap items-start gap-x-3 gap-y-2 rounded-t-lg border-b bg-card px-4 py-3 md:top-0">
        <div className="flex min-w-0 flex-1 items-start gap-3">
          <h3 id={headingId} className="text-base leading-6 font-semibold tabular">
            <span className="sr-only">Response </span>
            {label}
          </h3>
          <div className="min-w-0 pt-px">
            <p className="truncate font-mono text-[13px] leading-5" title={candidate.model}>
              {middle(candidate.model, 52)}
            </p>
            <p className="truncate text-xs text-muted-foreground" title={`${candidate.run_name}${sampleNote}`}>
              {candidate.run_name}
              {sampleNote}
            </p>
          </div>
        </div>
        <div role="group" aria-label={`Mark response ${label}`} className="flex flex-none gap-1.5">
          <MarkButton
            kind="chosen"
            pressed={mark === "chosen"}
            disabled={disabled}
            onClick={() => onMark("chosen")}
            keyHint={position <= 9 ? String(position) : undefined}
            label={label}
          />
          <MarkButton
            kind="rejected"
            pressed={mark === "rejected"}
            disabled={disabled}
            onClick={() => onMark("rejected")}
            keyHint={position <= 9 ? `Shift+${position}` : undefined}
            label={label}
          />
        </div>
      </header>

      <div className="grid gap-4 px-4 py-4">
        {truncated ? (
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <Pill tone="warning">Truncated</Pill>
            <span className="text-muted-foreground">Stopped at the token limit before finishing.</span>
          </div>
        ) : null}

        {candidate.reasoning ? (
          <Collapsible>
            <CollapsibleTrigger className="group/trace -mx-1 inline-flex min-h-8 items-center gap-1.5 rounded-md px-1 text-sm text-muted-foreground hover:text-foreground">
              <ChevronRight
                aria-hidden="true"
                className="size-4 transition-transform duration-150 ease-out group-data-[state=open]/trace:rotate-90"
              />
              <span>
                Thinking trace · <span className="tabular">{traceSize(candidate)}</span>
              </span>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="mt-2 max-h-[32rem] overflow-y-auto overscroll-contain rounded-md bg-muted px-3 py-2.5 text-sm text-muted-foreground">
                <div className="model-text">{candidate.reasoning}</div>
              </div>
            </CollapsibleContent>
          </Collapsible>
        ) : (
          <p className="text-sm text-muted-foreground">No thinking trace</p>
        )}

        {candidate.response?.trim() ? (
          <div className="model-text text-sm">{candidate.response}</div>
        ) : (
          <p className="text-sm text-muted-foreground">The model returned an empty response.</p>
        )}

        {candidate.completion_tokens != null ? (
          <p className="tabular text-xs text-muted-foreground">{count(candidate.completion_tokens, "completion token", "completion tokens")}</p>
        ) : null}
      </div>
    </article>
  )
}

function MarkButton({
  kind,
  pressed,
  disabled,
  onClick,
  keyHint,
  label,
}: {
  kind: Mark
  pressed: boolean
  disabled?: boolean
  onClick: () => void
  keyHint?: string
  label: string
}) {
  const Icon = kind === "chosen" ? Check : X
  const word = kind === "chosen" ? "Chosen" : "Rejected"
  return (
    <button
      type="button"
      aria-pressed={pressed}
      aria-label={`${word}: response ${label}`}
      aria-keyshortcuts={keyHint}
      title={keyHint ? `${word} (${keyHint})` : word}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "inline-flex h-9 items-center gap-1.5 rounded-md border px-3 text-sm font-medium whitespace-nowrap select-none",
        "transition-[background-color,border-color,color] duration-150 active:scale-[0.97] disabled:cursor-not-allowed disabled:opacity-50",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
        pressed
          ? kind === "chosen"
            ? "border-copper bg-copper-tint text-copper-ink"
            : "border-foreground bg-muted text-foreground"
          : "border-input bg-background text-foreground hover:bg-muted",
      )}
    >
      <Icon aria-hidden="true" className={cn("size-4", !pressed && "text-muted-foreground")} />
      {word}
    </button>
  )
}
