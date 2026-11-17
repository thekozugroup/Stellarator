import { Pill } from "@/components/shared/status-pill"
import type { PromptSetStatus } from "@/lib/types"

const label: Record<PromptSetStatus, string> = { importing: "Importing", ready: "Ready", failed: "Failed" }

/** Importing is live (green dot, as for a running run); Failed is a problem (red dot); Ready is plain text. */
export function SetStatus({ status, className }: { status: PromptSetStatus; className?: string }) {
  return (
    <Pill className={className} tone={status === "importing" ? "positive" : status === "failed" ? "negative" : "plain"}>
      {label[status]}
    </Pill>
  )
}

export const statusLabel = label

/** Poll every second while any set is importing. */
export function importingInterval(sets: { status: PromptSetStatus }[] | { status: PromptSetStatus } | undefined): number | false {
  if (!sets) return false
  return (Array.isArray(sets) ? sets.some((s) => s.status === "importing") : sets.status === "importing") ? 1000 : false
}
