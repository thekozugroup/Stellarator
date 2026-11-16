/** Locale-aware formatting. Never render raw numbers, floats or dates. */

const nf0 = new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 })
const nf1 = new Intl.NumberFormat(undefined, { minimumFractionDigits: 1, maximumFractionDigits: 1 })
const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 })
const plural = new Intl.PluralRules(undefined)

export const DASH = "–"

/** 1,284 */
export function int(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return DASH
  return nf0.format(n)
}

/** 12.5 */
export function dec1(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return DASH
  return nf1.format(n)
}

/** 12.7k. Only for capacity lines and tight spots, never in tables. */
export function short(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return DASH
  return compact.format(n)
}

/** "1 prompt", "1,284 prompts" */
export function count(n: number, one: string, many: string): string {
  return `${int(n)} ${plural.select(n) === "one" ? one : many}`
}

/** 42.0% from a ratio; DASH when the denominator is 0. */
export function pct(part: number, whole: number): string {
  if (!whole) return DASH
  return `${nf1.format((part / whole) * 100)}%`
}

/** "820 ms", "4.2 s", "3 min 12 s", "2 h 14 min" */
export function duration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || Number.isNaN(ms)) return DASH
  if (ms < 1000) return `${nf0.format(ms)} ms`
  const s = ms / 1000
  if (s < 60) return `${nf1.format(s)} s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m} min ${Math.round(s % 60)} s`
  const h = Math.floor(m / 60)
  if (h < 48) return `${h} h ${m % 60} min`
  return `${Math.floor(h / 24)} d ${h % 24} h`
}

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" })
const dateFmt = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" })
const dateTimeFmt = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
})
const timeFmt = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit", second: "2-digit" })

/** "just now", "5 minutes ago", "yesterday", then an absolute date after a week. */
export function relative(iso: string | null | undefined): string {
  if (!iso) return DASH
  const then = new Date(iso).getTime()
  const diff = (then - Date.now()) / 1000
  const abs = Math.abs(diff)
  if (abs < 45) return "just now"
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute")
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour")
  if (abs < 7 * 86400) return rtf.format(Math.round(diff / 86400), "day")
  return dateFmt.format(new Date(iso))
}

/** "Nov 16, 9:14 PM" */
export function dateTime(iso: string | null | undefined): string {
  if (!iso) return DASH
  return dateTimeFmt.format(new Date(iso))
}

/** "9:14:05 PM" */
export function time(iso: string | null | undefined): string {
  if (!iso) return DASH
  return timeFmt.format(new Date(iso))
}

/** File size: "2.3 MB" */
export function bytes(n: number): string {
  if (n < 1024) return `${int(n)} B`
  const units = ["KB", "MB", "GB"]
  let v = n / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${nf1.format(v)} ${units[i]}`
}

/** The user turn of a prompt, for previews. */
export function promptText(messages: { role: string; content: string }[]): string {
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === "user") return messages[i].content
  }
  return messages[messages.length - 1]?.content ?? ""
}

/** Shorten to one line of at most `max` characters. */
export function oneLine(s: string | null | undefined, max = 140): string {
  if (!s) return ""
  const flat = s.replace(/\s+/g, " ").trim()
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat
}

/** Keep the start and end: "Qwen/Qwen3-235B-A22B…Instruct-FP8" */
export function middle(s: string, max = 40): string {
  if (s.length <= max) return s
  const keep = max - 1
  const head = Math.ceil(keep * 0.6)
  return `${s.slice(0, head)}…${s.slice(s.length - (keep - head))}`
}
