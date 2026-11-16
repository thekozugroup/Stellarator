import { cn } from "@/lib/utils"
import type { RunStatus, SampleStatus } from "@/lib/types"

type Tone = "neutral" | "copper" | "positive" | "negative" | "warning"

const toneClass: Record<Tone, string> = {
  neutral: "bg-muted text-muted-foreground",
  copper: "bg-copper-tint text-copper-ink",
  positive: "bg-positive-tint text-positive-ink",
  negative: "bg-negative-tint text-negative-ink",
  warning: "bg-warning-tint text-warning-ink",
}

/** A pale tint with dark text of the same hue, always carrying a word. Never shrinks. */
export function Pill({ tone = "neutral", children, className }: { tone?: Tone; children: React.ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex h-5 flex-none items-center rounded-full px-2 text-xs font-medium whitespace-nowrap",
        toneClass[tone],
        className,
      )}
    >
      {children}
    </span>
  )
}

const runTone: Record<RunStatus, Tone> = {
  running: "copper",
  pausing: "warning",
  paused: "warning",
  completed: "neutral",
}

const runLabel: Record<RunStatus, string> = {
  running: "Running",
  pausing: "Pausing",
  paused: "Paused",
  completed: "Completed",
}

export function RunStatusPill({ status, failed = 0 }: { status: RunStatus; failed?: number }) {
  if (status === "completed" && failed > 0) return <Pill tone="negative">Completed with failures</Pill>
  return <Pill tone={runTone[status]}>{runLabel[status]}</Pill>
}

const sampleTone: Record<SampleStatus, Tone> = {
  pending: "neutral",
  running: "copper",
  done: "neutral",
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
