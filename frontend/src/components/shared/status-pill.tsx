import { cn } from "@/lib/utils"
import type { RunStatus, SampleStatus } from "@/lib/types"

/**
 * Status as a word, with a small dot at full strength when the state needs attention or is live.
 * Normal states ("Done", "Completed", "Pending") are plain gray text: a mark is for exceptions.
 * Color never carries meaning alone; the word is always there.
 */
type Tone = "plain" | "neutral" | "copper" | "positive" | "negative" | "warning"

const dotClass: Record<Exclude<Tone, "plain">, string> = {
  neutral: "bg-muted-foreground",
  copper: "bg-copper",
  positive: "bg-positive",
  negative: "bg-destructive",
  warning: "bg-warning",
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
      {tone !== "plain" ? <span aria-hidden="true" className={cn("size-1.5 flex-none rounded-full", dotClass[tone])} /> : null}
      {children}
    </span>
  )
}

const runTone: Record<RunStatus, Tone> = {
  running: "positive",
  pausing: "warning",
  paused: "warning",
  completed: "plain",
}

const runLabel: Record<RunStatus, string> = {
  running: "Running",
  pausing: "Pausing",
  paused: "Paused",
  completed: "Completed",
}

export function RunStatusPill({ status }: { status: RunStatus; failed?: number }) {
  return <Pill tone={runTone[status]}>{runLabel[status]}</Pill>
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
  if (status === "done" && finishReason === "length") return <Pill tone="warning">Truncated</Pill>
  return <Pill tone={sampleTone[status]}>{sampleLabel[status]}</Pill>
}
