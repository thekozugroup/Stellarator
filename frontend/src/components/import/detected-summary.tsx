import { GroupBox } from "@/components/shared/form-group"
import { cn } from "@/lib/utils"
import type { SummaryRow } from "./helpers"

/** Simple mode's read-only account of how the source will be imported (see importSummary). */
export function DetectedSummary({ rows }: { rows: SummaryRow[] }) {
  if (rows.length === 0) return null
  return (
    <GroupBox title="Detected">
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 p-4 text-sm">
        {rows.map((r) => (
          <div key={r.term} className="contents">
            <dt className="text-muted-foreground">{r.term}</dt>
            <dd className={cn("min-w-0 break-words", r.warn && "text-coral-ink")}>
              {r.parts.map((p, i) =>
                typeof p === "string" ? (
                  p
                ) : (
                  <span key={i} className="font-mono text-xs">
                    {p.code}
                  </span>
                ),
              )}
            </dd>
          </div>
        ))}
      </dl>
    </GroupBox>
  )
}
