import { CopyButton } from "@/components/shared/copy-button"
import { middle } from "@/lib/format"

/** A base URL in mono, shortened in the middle, full value on hover and in the copy button. */
export function BaseUrl({ url }: { url: string }) {
  return (
    <span className="flex min-w-0 items-center gap-0.5">
      <span title={url} className="min-w-0 truncate font-mono text-[13px] text-muted-foreground">
        <span className="hidden sm:inline">{middle(url, 46)}</span>
        <span className="sm:hidden">{middle(url, 30)}</span>
      </span>
      <CopyButton value={url} label="Copy base URL" />
    </span>
  )
}
