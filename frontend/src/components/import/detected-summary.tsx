import { cn } from "@/lib/utils"
import type { SummaryRow } from "./helpers"

/** Simple mode's read-only account of how the source will be imported (see importSummary). */
export function DetectedSummary({ rows }: { rows: SummaryRow[] }) {
  if (rows.length === 0) return null
  return (
    <section aria-labelledby="detected-heading" className="mt-4 grid gap-4">
      <h2 id="detected-heading" className="text-base font-semibold">
        Detected
      </h2>
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5 text-sm">
        {rows.map((r) => (
          <div key={r.term} className="contents">
            <dt className="text-muted-foreground">{r.term}</dt>
            <dd className={cn("min-w-0 break-words", r.warn && "text-warning-ink")}>
              {r.parts.map((p, i) =>
                typeof p === "string" ? (
                  p
                ) : (
                  <span key={i} className="font-mono text-[13px]">
                    {p.code}
                  </span>
                ),
              )}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  )
}
