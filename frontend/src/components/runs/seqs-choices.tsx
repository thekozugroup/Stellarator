import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/utils"
import { SEQS_CHOICES } from "./run-data"

/**
 * The common seqs values as one segmented control that never wraps: the segments share the row, so
 * eight of them fit in 262 px (a phone at 320 px inside a grouped form). The touch rule in
 * index.css gives segments a 44 pt minimum width, which eight cannot all have at that width, so
 * they keep the 44 pt height and give up the minimum width.
 */
export function SeqsChoices({ value, onChange, className }: { value: number; onChange: (n: number) => void; className?: string }) {
  return (
    <ToggleGroup
      type="single"
      variant="outline"
      size="sm"
      spacing={0}
      value={SEQS_CHOICES.includes(value) ? String(value) : ""}
      onValueChange={(v) => v && onChange(Number(v))}
      aria-label="Common seqs values"
      className={cn("w-full max-w-full min-w-0 sm:w-fit", className)}
    >
      {SEQS_CHOICES.map((n) => (
        <ToggleGroupItem key={n} value={String(n)} style={{ minWidth: 0 }} className="min-w-0 flex-1 px-1 tabular sm:px-2.5">
          {n}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}
