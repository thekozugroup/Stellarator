import type { Endpoint } from "@/lib/types"
import { Pill, StatusBadge, type Tone } from "@/components/shared/status-pill"
import { DASH, relative } from "@/lib/format"
import { cn } from "@/lib/utils"

/** The server answered but refused the key (the backend words it "The server rejected the API key (401)"). */
export const isKeyRejected = (error: string | null | undefined) => !!error && /^the server rejected the api key/i.test(error)

/** The server's own words after "The server rejected the API key (401)": ": User not found." becomes "401: User not found." */
export function serverReason(error: string | null | undefined) {
  if (!error) return null
  const m = error.match(/\((\d{3})\)\s*[:.\-–]?\s*(.*)$/s)
  if (!m) return null
  const body = m[2].trim()
  return body ? `${m[1]}: ${body}` : m[1]
}

/** The state of an endpoint as a word and a tone: Reachable (green), Unreachable or Key rejected (red), Not checked (plain). */
export function endpointState(endpoint: Endpoint): { word: string; tone: Tone; reason: string | null } {
  if (endpoint.last_ok === null) return { word: "Not checked", tone: "plain", reason: null }
  if (endpoint.last_ok) return { word: "Reachable", tone: "positive", reason: null }
  const rejected = isKeyRejected(endpoint.last_error)
  return {
    word: rejected ? "Key rejected" : "Unreachable",
    tone: "negative",
    reason: rejected ? serverReason(endpoint.last_error) : endpoint.last_error,
  }
}

/**
 * In lists: the dot and the word (DESIGN.md, Status), and for a failing endpoint the server's reason
 * under it in muted text. With `detail={false}` it is the word only, so it can sit in a "·" separated
 * line (the phone rows).
 */
export function EndpointStatus({ endpoint, detail = true, className }: { endpoint: Endpoint; detail?: boolean; className?: string }) {
  const { word, tone, reason } = endpointState(endpoint)
  if (!detail || !reason) return <Pill tone={tone} className={cn("align-middle", className)}>{word}</Pill>
  return (
    <span className={cn("grid justify-items-start gap-0.5", className)}>
      <Pill tone={tone}>{word}</Pill>
      <span title={endpoint.last_error ?? undefined} className="line-clamp-2 max-w-[34ch] text-xs text-muted-foreground">
        {reason}
      </span>
    </span>
  )
}

/** In a header (the details sheet): the status as a capsule in its tint. */
export function EndpointStatusBadge({ endpoint }: { endpoint: Endpoint }) {
  const { word, tone } = endpointState(endpoint)
  return <StatusBadge tone={tone}>{word}</StatusBadge>
}

export function CheckedAt({ endpoint }: { endpoint: Endpoint }) {
  if (!endpoint.last_checked_at) return <span className="text-muted-foreground">{DASH}</span>
  return <span title={new Date(endpoint.last_checked_at).toLocaleString()}>{relative(endpoint.last_checked_at)}</span>
}
