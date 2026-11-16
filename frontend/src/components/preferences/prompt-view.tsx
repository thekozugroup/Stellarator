import { ModelText } from "@/components/shared/model-text"
import { useState } from "react"
import type { Message } from "@/lib/types"
import { cn } from "@/lib/utils"

const ROLE_LABEL: Record<string, string> = { system: "System", user: "User", assistant: "Assistant" }

function text(content: unknown): string {
  if (typeof content === "string") return content
  if (Array.isArray(content)) {
    // OpenAI content parts: keep the text parts, name the rest.
    return content
      .map((part) => (part && typeof part === "object" && "text" in part ? String(part.text) : `[${String(part?.type ?? "part")}]`))
      .join("\n")
  }
  return JSON.stringify(content, null, 2)
}

const LONG = 1400 // characters before the prompt is shown folded

/** The prompt as sent, one block per message with its role. Long prompts fold to keep the responses in view. */
export function PromptView({ messages }: { messages: Message[] }) {
  const total = messages.reduce((n, m) => n + text(m.content).length, 0)
  const long = total > LONG
  const [open, setOpen] = useState(false)
  const folded = long && !open
  return (
    <div className="rounded-lg bg-muted">
      <div className={cn("relative grid gap-4 px-4 py-3", folded && "max-h-72 overflow-hidden")}>
        {messages.map((m, n) => (
          <div key={n} className="grid gap-1">
            {messages.length > 1 ? <p className="text-xs text-muted-foreground">{ROLE_LABEL[m.role] ?? m.role}</p> : null}
            <ModelText text={text(m.content)} className="text-sm" />
          </div>
        ))}
      </div>
      {long ? (
        <div className="border-t border-background px-4 py-2">
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-expanded={open}
            className="min-h-8 rounded-md text-sm font-medium text-foreground underline-offset-2 hover:underline"
          >
            {open ? "Show less" : "Show all"}
          </button>
        </div>
      ) : null}
    </div>
  )
}
