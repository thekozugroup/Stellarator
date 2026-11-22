import { CopyButton } from "@/components/shared/copy-button"
import { middle } from "@/lib/format"
import { cn } from "@/lib/utils"

/**
 * A base URL in mono, shortened in the middle, full value on hover and in the copy button. The copy
 * button overlaps the line above and below by 6 px so it does not make a list row taller.
 */
export function BaseUrl({ url, className }: { url: string; className?: string }) {
  return (
    <span className={cn("flex min-w-0 items-center gap-0.5 [&>button]:-my-1.5", className)}>
      <span title={url} className="min-w-0 truncate font-mono text-xs text-muted-foreground">
        <span className="hidden sm:inline">{middle(url, 46)}</span>
        <span className="sm:hidden">{middle(url, 30)}</span>
      </span>
      <CopyButton value={url} label="Copy base URL" />
    </span>
  )
}
