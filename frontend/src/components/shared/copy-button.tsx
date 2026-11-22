import { useState } from "react"
import { Check, Copy } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"

/**
 * Copy text. navigator.clipboard only exists on secure origins (https or localhost), and this app
 * is usually opened over plain http on a LAN address, so fall back to a hidden textarea.
 */
export async function copyText(value: string): Promise<boolean> {
  try {
    if (window.isSecureContext && navigator.clipboard) {
      await navigator.clipboard.writeText(value)
      return true
    }
  } catch {
    /* fall through to the textarea */
  }
  const previous = document.activeElement as HTMLElement | null
  const area = document.createElement("textarea")
  area.value = value
  // Not readonly: iOS Safari will not select text in a readonly field. 16 px avoids a zoom.
  area.style.position = "fixed"
  area.style.top = "0"
  area.style.left = "0"
  area.style.fontSize = "16px"
  area.style.opacity = "0"
  area.style.pointerEvents = "none"
  document.body.appendChild(area)
  area.focus({ preventScroll: true })
  area.select()
  area.setSelectionRange(0, value.length)
  let ok = false
  try {
    ok = document.execCommand("copy")
  } catch {
    ok = false
  }
  area.remove()
  previous?.focus({ preventScroll: true })
  return ok
}

export function CopyButton({ value, label = "Copy" }: { value: string; label?: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      className="text-muted-foreground hover:text-foreground"
      aria-label={copied ? "Copied" : label}
      title={copied ? "Copied" : label}
      onClick={async () => {
        if (await copyText(value)) {
          setCopied(true)
          window.setTimeout(() => setCopied(false), 1500)
        } else {
          toast.error("Couldn't copy. Select the text and copy it by hand.")
        }
      }}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
    </Button>
  )
}
