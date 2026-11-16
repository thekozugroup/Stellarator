import { Component, lazy, Suspense, useSyncExternalStore, type ReactNode } from "react"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/utils"

/**
 * Model text (prompts, thinking traces, responses), shown either rendered as Markdown or raw,
 * exactly as the model wrote it. The choice is one per viewer, remembered in this browser, and
 * shared by every panel on the page so they stay in step.
 */

export type TextMode = "rendered" | "raw"

const KEY = "stellarator:text-mode"
const listeners = new Set<() => void>()

function read(): TextMode {
  try {
    return window.localStorage.getItem(KEY) === "raw" ? "raw" : "rendered"
  } catch {
    return "rendered"
  }
}

let current: TextMode = typeof window === "undefined" ? "rendered" : read()

export function setTextMode(mode: TextMode) {
  current = mode
  try {
    window.localStorage.setItem(KEY, mode)
  } catch {
    /* storage blocked: keep it for this page only */
  }
  listeners.forEach((l) => l())
}

export function useTextMode(): TextMode {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => current,
    () => "rendered",
  )
}

/** "Rendered | Raw" switch. Put it in the header of the panel that shows model text. */
export function TextModeToggle({ className }: { className?: string }) {
  const mode = useTextMode()
  return (
    <ToggleGroup
      type="single"
      size="sm"
      variant="outline"
      spacing={0}
      value={mode}
      onValueChange={(v) => v && setTextMode(v as TextMode)}
      aria-label="Show model text"
      className={className}
    >
      <ToggleGroupItem value="rendered" className="px-2.5 text-xs">
        Rendered
      </ToggleGroupItem>
      <ToggleGroupItem value="raw" className="px-2.5 text-xs">
        Raw
      </ToggleGroupItem>
    </ToggleGroup>
  )
}

const Markdown = lazy(() => import("./markdown"))

/** If the Markdown renderer ever throws on odd input, fall back to the raw text. */
class RawFallback extends Component<{ text: string; className?: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  componentDidUpdate(prev: { text: string }) {
    if (prev.text !== this.props.text && this.state.failed) this.setState({ failed: false })
  }
  render() {
    if (this.state.failed) return <div className={cn("model-text", this.props.className)}>{this.props.text}</div>
    return this.props.children
  }
}

export function ModelText({
  text,
  className,
  mode: forced,
}: {
  text: string | null | undefined
  className?: string
  /** Override the viewer's choice, e.g. "raw" for a one-line preview. */
  mode?: TextMode
}) {
  const chosen = useTextMode()
  const mode = forced ?? chosen
  const value = text ?? ""
  if (mode === "raw") return <div className={cn("model-text", className)}>{value}</div>
  return (
    <RawFallback text={value} className={className}>
      <Suspense fallback={<div className={cn("model-text", className)}>{value}</div>}>
        <Markdown text={value} className={className} />
      </Suspense>
    </RawFallback>
  )
}
