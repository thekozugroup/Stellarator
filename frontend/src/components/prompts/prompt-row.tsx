import { useLayoutEffect, useRef, useState } from "react"
import { oneLine, promptText } from "@/lib/format"
import type { Prompt } from "@/lib/types"
import { int } from "@/lib/format"

function metaLine(meta: Record<string, unknown>): string {
  return Object.entries(meta)
    .map(([k, v]) => `${k}: ${oneLine(typeof v === "string" ? v : JSON.stringify(v), 60)}`)
    .join(" · ")
}

/** One prompt: index, the user text clamped to three lines (expandable), system indicator, meta. */
export function PromptRow({ prompt }: { prompt: Prompt }) {
  const [open, setOpen] = useState(false)
  const [overflows, setOverflows] = useState(false)
  const ref = useRef<HTMLParagraphElement>(null)
  const text = promptText(prompt.messages)
  const system = prompt.messages.find((m) => m.role === "system")
  const meta = metaLine(prompt.meta ?? {})
  const others = prompt.messages.filter((m) => m !== system && m.content !== text)

  useLayoutEffect(() => {
    const el = ref.current
    if (el && !open) setOverflows(el.scrollHeight > el.clientHeight + 1)
  }, [text, open])

  const expandable = overflows || open || !!system || others.length > 0

  return (
    <li className="grid grid-cols-[3.25rem_1fr] gap-x-2 py-3.5 sm:grid-cols-[4rem_1fr]">
      <span className="pt-px font-mono text-[13px] text-muted-foreground tabular">#{int(prompt.idx + 1)}</span>
      <div className="min-w-0">
        {open && system ? (
          <div className="mb-2">
            <p className="text-xs text-muted-foreground">System message</p>
            <p className="model-text rounded-md bg-muted px-3 py-2 text-sm">{system.content}</p>
          </div>
        ) : null}
        {open && others.length ? (
          <div className="mb-2 grid gap-2">
            {others.map((m, i) => (
              <div key={i}>
                <p className="text-xs text-muted-foreground capitalize">{m.role}</p>
                <p className="model-text text-sm">{m.content}</p>
              </div>
            ))}
          </div>
        ) : null}
        {open && (system || others.length) ? <p className="text-xs text-muted-foreground">User</p> : null}
        <p ref={ref} className={`model-text text-sm ${open ? "" : "line-clamp-3"}`}>
          {text || <span className="text-muted-foreground">This prompt has no user text.</span>}
        </p>
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
              className="-my-1.5 inline-flex min-h-8 items-center rounded-sm text-foreground underline underline-offset-4 focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
            >
              {open ? "Show less" : "Show full prompt"}
            </button>
          ) : null}
        </div>
      </div>
    </li>
  )
}

export function PromptRowsSkeleton({ rows = 6 }: { rows?: number }) {
  return (
    <ul aria-busy="true" aria-label="Loading prompts" className="divide-y border-y">
      {Array.from({ length: rows }, (_, i) => (
        <li key={i} className="grid grid-cols-[3.25rem_1fr] gap-x-2 py-3.5 sm:grid-cols-[4rem_1fr]">
          <span className="mt-0.5 inline-block h-4 w-8 rounded bg-muted" />
          <div className="grid gap-1.5">
            <span className="inline-block h-4 w-full max-w-[70ch] rounded bg-muted" />
            <span className="inline-block h-4 w-4/5 max-w-[56ch] rounded bg-muted" />
            <span className="inline-block h-4 w-2/5 max-w-[30ch] rounded bg-muted" />
          </div>
        </li>
      ))}
    </ul>
  )
}
