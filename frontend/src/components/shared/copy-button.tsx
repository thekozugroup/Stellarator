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
  const area = document.createElement("textarea")
  area.value = value
  area.setAttribute("readonly", "")
  area.style.position = "fixed"
  area.style.opacity = "0"
  area.style.pointerEvents = "none"
  document.body.appendChild(area)
  area.select()
  let ok = false
  try {
    ok = document.execCommand("copy")
  } catch {
    ok = false
  }
  area.remove()
  return ok
}

export function CopyButton({ value, label = "Copy" }: { value: string; label?: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
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
