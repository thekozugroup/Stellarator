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
 * One response to the prompt (DESIGN.md, Cards). A --muted card with 16 px corners and no border;
 * the thinking trace and the response sit in --background wells with 8 px corners, inset 8 px from
 * the card so the corners are concentric. The letter is the response's name on screen and its key
 * (A is 1, B is 2).
 *
 * The chosen response carries the accent: a 2 px periwinkle ring and a CHOSEN tag. The rejected one
 * steps back: its text turns muted and it carries a REJECTED tag. The header stays in view while a
 * long response scrolls, so the choice is always one click away.
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
  const rejected = mark === "rejected"
  return (
    <article
      aria-labelledby={headingId}
      data-mark={mark ?? undefined}
      className={cn(
        "min-w-0 rounded-xl bg-muted transition-shadow duration-200 ease-out",
        mark === "chosen" && "ring-2 ring-periwinkle",
      )}
    >
      {/* Sticky under the toolbar; the same fill as the card so text scrolls under it, and the
          card's top corners. */}
      <header className="sticky top-(--top-chrome) z-10 flex items-center gap-x-3 gap-y-2 rounded-t-xl bg-muted py-2 pr-2 pl-5 sm:flex-wrap sm:py-3 sm:pr-3">
        <div className="flex min-w-0 flex-[1_1_0] items-center gap-3 sm:flex-[1_1_12rem]">
          <h3
            id={headingId}
            className={cn(
              "grid size-7 flex-none place-items-center rounded-sm bg-background text-sm font-semibold tabular",
              rejected && "text-muted-foreground",
            )}
          >
            <span className="sr-only">Response </span>
            {label}
          </h3>
          <div className="grid min-w-0 gap-0.5">
            <p
              className={cn("truncate text-sm leading-5", showModel && "font-mono text-xs", rejected && "text-muted-foreground")}
              title={showModel ? candidate.model : candidate.run_name}
            >
              {showModel ? middle(identity, 52) : identity}
            </p>
            {sampleNote || truncated ? (
              <p className="flex min-w-0 items-center gap-x-2 text-xs text-muted-foreground">
                {sampleNote ? <span className="truncate">{sampleNote}</span> : null}
                {truncated ? (
                  <Pill tone="attention" className="text-xs" >
                    Truncated
                  </Pill>
                ) : null}
              </p>
            ) : null}
          </div>
          {mark ? <MarkTag mark={mark} /> : null}
        </div>
        <div
          role="group"
          aria-label={`Mark response ${label}`}
          className="flex h-8 flex-none items-center rounded-md bg-secondary p-0.5 pointer-coarse:h-auto"
        >
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
            pressed={rejected}
            disabled={disabled}
            onClick={() => onMark("rejected")}
            keyHint={position <= 9 ? `Shift+${position}` : undefined}
            label={label}
          />
        </div>
      </header>

      <div className={cn("grid min-w-0 gap-2 px-2 pb-2 [&>*]:min-w-0", rejected && "text-muted-foreground")}>
        {candidate.reasoning ? (
          <Collapsible className="rounded-sm bg-background">
            <CollapsibleTrigger className="group/trace flex min-h-9 w-full items-center gap-1.5 rounded-sm px-3 text-left text-sm text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-muted">
              <ChevronRight
                aria-hidden="true"
                className="size-4 flex-none transition-transform duration-150 ease-out group-data-[state=open]/trace:rotate-90"
              />
              <span>
                Thinking trace · <span className="tabular">{traceSize(candidate)}</span>
              </span>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="max-h-[32rem] overflow-y-auto overscroll-contain border-t px-3 pt-2.5 pb-3 text-sm text-muted-foreground">
                <ModelText text={candidate.reasoning} />
              </div>
            </CollapsibleContent>
          </Collapsible>
        ) : (
          <p className="px-3 py-1 text-sm text-muted-foreground">No thinking trace</p>
        )}

        <div className="rounded-sm bg-background px-3 py-3">
          {candidate.response?.trim() ? (
            <ModelText text={candidate.response} className="text-sm" />
          ) : (
            <p className="text-sm text-muted-foreground">The model returned an empty response.</p>
          )}
        </div>

        {candidate.completion_tokens != null ? (
          <p className="px-3 pt-0.5 pb-1 text-xs text-muted-foreground tabular">
            {count(candidate.completion_tokens, "completion token", "completion tokens")}
          </p>
        ) : null}
      </div>
    </article>
  )
}

/** CHOSEN in the accent tint, REJECTED in the raised gray: a data tag (DESIGN.md, Typography). */
function MarkTag({ mark }: { mark: Mark }) {
  return (
    <span
      className={cn(
        "label-mono inline-flex h-6 flex-none items-center rounded-full px-2.5",
        mark === "chosen" ? "bg-periwinkle-tint text-periwinkle-ink" : "bg-secondary text-muted-foreground",
      )}
    >
      {mark === "chosen" ? "Chosen" : "Rejected"}
    </span>
  )
}

/**
 * Choose and Reject as the two segments of one control (DESIGN.md, Segmented controls): the pressed
 * one raised on the track with ink text, the other muted. Each is a toggle button (aria-pressed) so
 * either can be pressed on its own; the card around it carries the accent.
 */
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
        "inline-flex h-7 items-center justify-center gap-1.5 rounded-sm px-3 text-sm whitespace-nowrap select-none pointer-coarse:min-w-11",
        "transition-[background-color,color,box-shadow] duration-150 active:scale-[0.97] disabled:cursor-not-allowed disabled:opacity-50",
        "outline-none focus-visible:z-10 focus-visible:ring-2 focus-visible:ring-ring",
        pressed
          ? "bg-background font-medium text-foreground shadow-xs dark:bg-accent"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      <Icon aria-hidden="true" className={cn("size-4", pressed && "stroke-[2.5]", pressed && kind === "chosen" && "text-periwinkle")} />
      {/* Phones show the icons only; the accessible name keeps the word. */}
      <span className="max-sm:hidden">{word}</span>
    </button>
  )
}
