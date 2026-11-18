import { ModelText } from "@/components/shared/model-text"
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
  showModel,
  showSample,
  disabled,
  onMark,
}: {
  candidate: Candidate
  label: string
  position: number
  mark: Mark | null
  showModel: boolean
  showSample: boolean
  disabled?: boolean
  onMark: (mark: Mark) => void
}) {
  const truncated = candidate.finish_reason === "length"
  const headingId = `cand-${candidate.sample_id}`
  // Name only what tells the cards apart: the model when the set compares several, else the sample,
  // else the run.
  const sample = `Sample ${int(candidate.sample_idx + 1)}`
  const identity = showModel ? candidate.model : showSample ? sample : candidate.run_name
  const sampleNote = showModel && showSample ? sample : null
  return (
    <article
      aria-labelledby={headingId}
      data-mark={mark ?? undefined}
      className={cn(
        "min-w-0 rounded-lg border bg-card",
        mark === "chosen" && "border-copper ring-1 ring-copper",
        mark === "rejected" && "border-foreground/40",
      )}
    >
      <header className="sticky top-(--top-chrome) z-10 flex items-center gap-x-3 gap-y-2 rounded-t-lg border-b bg-card px-4 py-2 sm:flex-wrap sm:items-start sm:py-3">
        <div className="flex min-w-0 flex-[1_1_0] items-start gap-3 sm:flex-[1_1_13rem]">
          <h3 id={headingId} className="text-base leading-6 font-semibold tabular">
            <span className="sr-only">Response </span>
            {label}
          </h3>
          <div className="min-w-0 pt-px">
            <p
              className={cn("truncate text-[13px] leading-5", showModel && "font-mono")}
              title={showModel ? candidate.model : candidate.run_name}
            >
              {showModel ? middle(identity, 52) : identity}
            </p>
            {sampleNote ? <p className="truncate text-xs text-muted-foreground">{sampleNote}</p> : null}
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

      <div className="grid min-w-0 gap-4 px-4 py-4 [&>*]:min-w-0">
        {truncated ? (
          <div>
            <Pill tone="warning">Truncated</Pill>
          </div>
        ) : null}

        {candidate.reasoning ? (
          <Collapsible>
            <CollapsibleTrigger className="group/trace -mx-1 inline-flex min-h-9 items-center gap-1.5 rounded-md px-1 text-sm text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring">
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
                <ModelText text={candidate.reasoning} />
              </div>
            </CollapsibleContent>
          </Collapsible>
        ) : (
          <p className="text-sm text-muted-foreground">No thinking trace</p>
        )}

        {candidate.response?.trim() ? (
          <ModelText text={candidate.response} className="text-sm" />
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
  const word = kind === "chosen" ? "Choose" : "Reject"
  return (
    <button
      type="button"
      aria-pressed={pressed}
      aria-label={`${word} response ${label}`}
      aria-keyshortcuts={keyHint}
      title={keyHint ? `${word} (${keyHint})` : word}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "inline-flex h-9 items-center gap-1.5 rounded-md border px-3 text-sm font-medium whitespace-nowrap select-none pointer-coarse:h-11 pointer-coarse:px-3",
        "transition-[background-color,border-color,color] duration-150 active:scale-[0.97] disabled:cursor-not-allowed disabled:opacity-50",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
        // Pressed reads as selected by its border and icon, never as the heaviest fill on screen.
        // The card's own border carries the copper; the button just reads as pressed.
        pressed
          ? "border-foreground bg-muted text-foreground ring-1 ring-foreground"
          : "border-input bg-background text-foreground hover:bg-muted",
      )}
    >
      <Icon aria-hidden="true" className={cn("size-4", pressed ? "stroke-[2.5]" : "text-muted-foreground")} />
      {word}
    </button>
  )
}
