import { ModelText, useTextMode } from "@/components/shared/model-text"
import { Bar } from "@/components/shared/block-states"
import { cn } from "@/lib/utils"
import { useLayoutEffect, useRef, useState } from "react"
import { oneLine, promptText } from "@/lib/format"
import type { Prompt } from "@/lib/types"
import { int } from "@/lib/format"

function metaLine(meta: Record<string, unknown>): string {
  return Object.entries(meta)
    .map(([k, v]) => `${k}: ${oneLine(typeof v === "string" ? v : JSON.stringify(v), 60)}`)
    .join(" · ")
}

/** One prompt: index, the user text (Rendered or Raw, clamped to about three lines), system indicator, meta. */
export function PromptRow({ prompt }: { prompt: Prompt }) {
  const [open, setOpen] = useState(false)
  const [overflows, setOverflows] = useState(false)
  const clamp = useRef<HTMLDivElement>(null)
  const text = promptText(prompt.messages)
  const system = prompt.messages.find((m) => m.role === "system")
  const meta = metaLine(prompt.meta ?? {})
  const others = prompt.messages.filter((m) => m !== system && m.content !== text)
  const mode = useTextMode()

  // The Markdown renderer loads lazily and changes height, so measure again on every resize.
  useLayoutEffect(() => {
    const el = clamp.current
    if (!el || open) return
    const measure = () => setOverflows(el.scrollHeight > el.clientHeight + 6)
    measure()
    const ro = new ResizeObserver(measure)
    if (el.firstElementChild) ro.observe(el.firstElementChild)
    return () => ro.disconnect()
  }, [text, open, mode])

  const expandable = overflows || open || !!system || others.length > 0

  return (
    <li className="grid grid-cols-[3rem_1fr] gap-x-2 border-b px-4 py-3 last:border-b-0">
      <span className="pt-0.5 font-mono text-muted-foreground tabular">{int(prompt.idx + 1)}</span>
      <div className="min-w-0">
        {open && system ? (
          <div className="mb-2">
            <p className="text-xs text-muted-foreground">System message</p>
            <ModelText text={system.content} className="mt-1 rounded-sm bg-muted px-3 py-2 text-sm" />
          </div>
        ) : null}
        {open && others.length ? (
          <div className="mb-2 grid gap-2">
            {others.map((m, i) => (
              <div key={i}>
                <p className="text-xs text-muted-foreground capitalize">{m.role}</p>
                <ModelText text={m.content} className="text-sm" />
              </div>
            ))}
          </div>
        ) : null}
        {open && (system || others.length) ? <p className="text-xs text-muted-foreground">User</p> : null}
        {!text ? (
          <p className="text-sm text-muted-foreground">This prompt has no user text.</p>
        ) : open ? (
          <ModelText text={text} className="text-sm" />
        ) : (
          <div
            ref={clamp}
            className={cn("max-h-24 overflow-hidden", overflows && "[mask-image:linear-gradient(to_bottom,black_70%,transparent)]")} /* taste-ignore: gradient -- overflow fade mask, no color */
          >
            <ModelText text={text} className="text-sm [&>*:first-child]:mt-0" />
          </div>
        )}
        <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          {system && !open ? <span>Has a system message</span> : null}
          {meta ? (
            <span title={meta} className="max-w-full truncate">
              {meta}
            </span>
          ) : null}
          {expandable ? (
            <button
              type="button"
              aria-expanded={open}
              onClick={() => setOpen((o) => !o)}
              className="-my-1.5 inline-flex min-h-8 items-center rounded-sm text-foreground underline underline-offset-4 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none pointer-coarse:min-h-11"
            >
              {open ? "Show less" : "Show all"}
            </button>
          ) : null}
        </div>
      </div>
    </li>
  )
}

export function PromptRowsSkeleton({ rows = 6 }: { rows?: number }) {
  return (
    <ul aria-busy="true" aria-label="Loading prompts">
      {Array.from({ length: rows }, (_, i) => (
        <li key={i} className="grid grid-cols-[3rem_1fr] gap-x-2 border-b px-4 py-3 last:border-b-0">
          <Bar className="mt-0.5 h-4 w-6" />
          <div className="grid gap-1.5">
            <Bar className="h-4 w-full max-w-[70ch]" />
            <Bar className="h-4 w-4/5 max-w-[56ch]" />
            <Bar className="h-4 w-2/5 max-w-[30ch]" />
          </div>
        </li>
      ))}
    </ul>
  )
}
