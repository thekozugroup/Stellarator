import { useLayoutEffect, useRef, useState } from "react"
import { Panel } from "@/components/shared/page"
import { Bar, EmptyBlock, ErrorBlock } from "@/components/shared/block-states"
import { Button } from "@/components/ui/button"
import { int } from "@/lib/format"
import type { ImportPreview, Message } from "@/lib/types"
import { cn } from "@/lib/utils"

const PREVIEW_ROWS = 20

function PromptRow({ n, messages }: { n: number; messages: Message[] }) {
  const system = messages.find((m) => m.role === "system")
  const rest = messages.filter((m) => m !== system)
  const multi = rest.length > 1
  const [open, setOpen] = useState(false)
  const [overflowing, setOverflowing] = useState(false)
  const body = useRef<HTMLDivElement>(null)
  const sig = messages.map((m) => m.content).join("\u0000")

  useLayoutEffect(() => {
    const el = body.current
    if (el && !open) setOverflowing(el.scrollHeight > el.clientHeight + 1)
  }, [sig, open])

  return (
    <li className="grid grid-cols-[2.5rem_minmax(0,1fr)] gap-x-2 rounded-sm bg-background px-3 py-3">
      <span className="pt-0.5 font-mono tabular text-muted-foreground">{int(n)}</span>
      <div className="min-w-0 space-y-1.5">
        {system ? (
          <p className="line-clamp-2 text-xs text-muted-foreground">
            <span className="font-medium">System</span> <span className="model-text">{system.content}</span>
          </p>
        ) : null}
        <div ref={body} className={cn("space-y-1.5 text-sm", !open && "max-h-[9.6em] overflow-hidden")}>
          {rest.map((m, i) => (
            <p key={i} className="model-text">
              {multi ? <span className="mr-1.5 text-xs font-medium text-muted-foreground">{m.role}</span> : null}
              {m.content}
            </p>
          ))}
        </div>
        {overflowing || open ? (
          <Button
            type="button"
            variant="ghost"
            size="xs"
            className="-ml-2 text-muted-foreground"
            aria-expanded={open}
            onClick={() => setOpen((v) => !v)}
          >
            {open ? "Show less" : "Show all"}
          </Button>
        ) : null}
      </div>
    </li>
  )
}

export function PreviewPanel({
  preview,
  hasSource,
  loading,
  updating,
  error,
  onRetry,
  emptyHint,
  showDropped = true,
}: {
  preview: ImportPreview | undefined
  hasSource: boolean
  loading: boolean
  updating: boolean
  error: string | null
  onRetry: () => void
  emptyHint?: string
  /** False in Simple mode, whose summary already says what is left out. */
  showDropped?: boolean
}) {
  const shown = preview?.prompts.length ?? 0
  const est = preview?.estimated_prompts ?? null
  let summary: string | null = null
  if (preview) {
    if (est !== null && est > shown) summary = `First ${int(shown)} of ~${int(est)} prompts`
    else if (est !== null) summary = `${int(est)} ${est === 1 ? "prompt" : "prompts"}`
    else summary = `First ${int(Math.min(shown, PREVIEW_ROWS))} prompts`
  }

  return (
    <Panel
      title="Preview"
      description={summary ?? undefined}
      actions={
        updating ? (
          <span role="status" className="text-xs text-muted-foreground">
            Updating…
          </span>
        ) : null
      }
      bodyClassName="p-2 pt-1"
      className="min-w-0"
    >
      {preview && preview.warnings.length > 0 ? (
        <ul className="mb-2 space-y-1 rounded-sm bg-coral-tint px-3 py-2.5 text-sm text-coral-ink">
          {preview.warnings.map((w, i) => (
            <li key={i}>{w}</li>
          ))}
        </ul>
      ) : null}

      {showDropped && preview && preview.dropped.length > 0 ? (
        <p className="mb-2 px-2 text-xs text-muted-foreground">Responses left out: {preview.dropped.join(", ")}.</p>
      ) : null}

      {preview && preview.columns.length > 0 ? (
        <p className="mb-2 px-2 text-xs text-muted-foreground">
          Columns:{" "}
          {preview.columns.map((c, i) => (
            <span key={c.name}>
              {i > 0 ? ", " : null}
              <span className="font-mono">{c.name}</span>
            </span>
          ))}
        </p>
      ) : null}

      {error && !preview ? (
        <div>
          <ErrorBlock message={error} onRetry={hasSource ? onRetry : undefined} />
        </div>
      ) : preview ? (
        preview.prompts.length === 0 ? (
          <EmptyBlock className="px-2">No prompts came out of this source with these settings. Try another split or column.</EmptyBlock>
        ) : (
          <ol className={cn("grid gap-2 transition-opacity duration-200", updating && "opacity-70")}>
            {preview.prompts.map((m, i) => (
              <PromptRow key={i} n={i + 1} messages={m} />
            ))}
          </ol>
        )
      ) : loading ? (
        <div aria-busy="true" aria-label="Loading preview" className="grid gap-2">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="grid grid-cols-[2.5rem_minmax(0,1fr)] gap-x-2 rounded-sm bg-background px-3 py-3">
              <Bar className="h-[1.25rem] w-6" />
              <div className="space-y-2">
                <Bar className="h-4 w-full" />
                <Bar className="h-4 w-4/5" />
                <Bar className="h-4 w-2/5" />
              </div>
            </div>
          ))}
        </div>
      ) : (
        <EmptyBlock className="px-2">{emptyHint ?? "No source yet."}</EmptyBlock>
      )}
    </Panel>
  )
}
