import type { ImportFormat, SplitMode } from "@/lib/types"

/** Accept "owner/name", "datasets/owner/name" or a full huggingface.co URL; return "owner/name". */
export function parseDataset(input: string): string {
  let s = input.trim()
  if (!s) return ""
  s = s.replace(/^https?:\/\//i, "").replace(/^(www\.)?huggingface\.co\//i, "")
  s = s.replace(/[?#].*$/, "")
  s = s.replace(/^datasets\//, "")
  const parts = s.split("/").filter(Boolean)
  // owner/name, optionally followed by /viewer, /tree/main, /blob/...
  return parts.slice(0, 2).join("/")
}

export const ACCEPT = ".txt,.md,.markdown,.csv,.tsv,.json,.jsonl,.ndjson,.parquet"

export const FORMAT_OPTIONS: { value: ImportFormat; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "text", label: "Text" },
  { value: "markdown", label: "Markdown" },
  { value: "csv", label: "CSV" },
  { value: "tsv", label: "TSV" },
  { value: "json", label: "JSON" },
  { value: "jsonl", label: "JSONL" },
  { value: "parquet", label: "Parquet" },
]

export function formatLabel(f: string): string {
  const hit = FORMAT_OPTIONS.find((o) => o.value === f)
  if (hit) return hit.label
  if (f === "huggingface") return "Hugging Face"
  return f
}

export const SPLIT_OPTIONS: { value: SplitMode; label: string; hint: string }[] = [
  { value: "auto", label: "Automatic", hint: "" },
  { value: "line", label: "One per line", hint: "one prompt per non-empty line" },
  { value: "blank_line", label: "Blank line between prompts", hint: "split on blank lines" },
  { value: "separator", label: "Separator lines (---)", hint: "split on --- lines" },
  { value: "heading", label: "Markdown headings", hint: "split on Markdown headings" },
  { value: "list_item", label: "List items", hint: "one prompt per list item" },
  { value: "whole", label: "Whole text is one prompt", hint: "the whole text is one prompt" },
]

export function splitHint(mode: SplitMode | null): string | null {
  const hit = SPLIT_OPTIONS.find((o) => o.value === mode)
  return hit?.hint || null
}

export function stripExtension(name: string): string {
  return name.replace(/\.[^./]+$/, "")
}

/** "" -> null; digits only -> number; anything else -> undefined (invalid). */
export function parseCount(s: string): number | null | undefined {
  const t = s.replace(/[,\s]/g, "")
  if (t === "") return null
  if (!/^\d+$/.test(t)) return undefined
  return Number(t)
}
