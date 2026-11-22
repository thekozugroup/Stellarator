import { cn } from "@/lib/utils"
import type { RunStatus, SampleStatus } from "@/lib/types"

/**
 * Status (DESIGN.md, Components). Two forms of the same thing:
 * - `Pill`, in lists and tables: a 6 px dot in the state's color and the word. Normal states
 *   ("Done", "Completed", "Pending") are a plain muted word with no dot.
 * - `StatusBadge`, in a detail screen's header: a 24 px capsule in the state's tint with the dot
 *   and the word in the state's ink.
 * Color never carries meaning alone; the word is always there.
 *
 * Tones: positive (green: running, live, answering), attention (coral: paused, truncated),
 * negative (red: failed), accent (periwinkle: thinking), neutral (a gray dot: waiting).
 */
export type Tone = "plain" | "neutral" | "accent" | "positive" | "negative" | "attention"

const dotClass: Record<Exclude<Tone, "plain">, string> = {
  neutral: "bg-muted-foreground",
  accent: "bg-periwinkle",
  positive: "bg-positive",
  negative: "bg-destructive",
  attention: "bg-coral",
}

const badgeClass: Record<Tone, string> = {
  plain: "bg-secondary text-muted-foreground",
  neutral: "bg-secondary text-foreground",
  accent: "bg-periwinkle-tint text-periwinkle-ink",
  positive: "bg-positive-tint text-positive-ink",
  negative: "bg-negative-tint text-negative-ink",
  attention: "bg-coral-tint text-coral-ink",
}

function Dot({ tone }: { tone: Exclude<Tone, "plain"> }) {
  return <span aria-hidden="true" className={cn("size-1.5 flex-none rounded-full", dotClass[tone])} />
}

export function Pill({ tone = "plain", children, className }: { tone?: Tone; children: React.ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex flex-none items-center gap-1.5 text-sm whitespace-nowrap",
        tone === "plain" ? "text-muted-foreground" : "text-foreground",
        className,
      )}
    >
      {tone !== "plain" ? <Dot tone={tone} /> : null}
      {children}
    </span>
  )
}

export function StatusBadge({ tone = "plain", children, className }: { tone?: Tone; children: React.ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex h-7 flex-none items-center gap-1.5 rounded-full px-3 text-sm font-medium whitespace-nowrap",
        badgeClass[tone],
        className,
      )}
    >
      {tone !== "plain" ? <Dot tone={tone} /> : null}
      {children}
    </span>
  )
}

export const runTone: Record<RunStatus, Tone> = {
  running: "positive",
  pausing: "attention",
  paused: "attention",
  completed: "plain",
}

export const runLabel: Record<RunStatus, string> = {
  running: "Running",
  pausing: "Pausing",
  paused: "Paused",
  completed: "Completed",
}

export function RunStatusPill({ status }: { status: RunStatus; failed?: number }) {
  return <Pill tone={runTone[status]}>{runLabel[status]}</Pill>
}

/** A run's status in its header: Completed reads as done (green), the rest as in the lists. */
export function RunStatusBadge({ status }: { status: RunStatus }) {
  return <StatusBadge tone={status === "completed" ? "positive" : runTone[status]}>{runLabel[status]}</StatusBadge>
}

const sampleTone: Record<SampleStatus, Tone> = {
  pending: "plain",
  running: "positive",
  done: "plain",
  failed: "negative",
}

const sampleLabel: Record<SampleStatus, string> = {
  pending: "Pending",
  running: "In flight",
  done: "Done",
  failed: "Failed",
}

export function SampleStatusPill({ status, finishReason }: { status: SampleStatus; finishReason?: string | null }) {
  if (status === "done" && finishReason === "length") return <Pill tone="attention">Truncated</Pill>
  return <Pill tone={sampleTone[status]}>{sampleLabel[status]}</Pill>
}
