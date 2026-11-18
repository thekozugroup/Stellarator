import type { ImportFormat, ImportPreview, SplitMode, Turns } from "@/lib/types"

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

/** True for a complete "owner/name", the shape Simple mode loads without a click. */
export function isDatasetId(s: string): boolean {
  return /^[A-Za-z0-9][\w.-]*\/[A-Za-z0-9][\w.-]*$/.test(s)
}

/** The split Stellarator reads by default: "train", else the first starting with "train", else the first. */
export function defaultSplit(names: string[]): string | undefined {
  return names.find((n) => n === "train") ?? names.find((n) => n.startsWith("train")) ?? names[0]
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

// ------------------------------------------------------------------ Simple and Advanced

/**
 * Simple shows the source and what was detected; Advanced shows every setting. Both edit the same
 * state, so switching never resets a choice. The choice is remembered in this browser.
 */
export type ImportMode = "simple" | "advanced"

const MODE_KEY = "stellarator:import-mode"

export function readImportMode(): ImportMode {
  try {
    return window.localStorage.getItem(MODE_KEY) === "advanced" ? "advanced" : "simple"
  } catch {
    return "simple"
  }
}

export function saveImportMode(mode: ImportMode) {
  try {
    window.localStorage.setItem(MODE_KEY, mode)
  } catch {
    /* storage blocked: the choice lasts while the page is open */
  }
}

// ------------------------------------------------------------------ explanations

/** The one- or two-sentence explanations behind the info buttons on the import screen. */
export const INFO = {
  mode: "Simple picks the columns and settings for you and lists what it picked. Advanced shows every setting; switching keeps your choices.",
  dataset:
    "Paste a link to the dataset's page or type its name as owner/name. Gated and private datasets need a Hugging Face token in Settings.",
  config:
    "Some datasets come in several configs, such as versions or languages, each with its own splits. Stellarator picks the one named default, or else the first.",
  split:
    "The part of the dataset to import, such as train or test. Stellarator picks train, or else the first split whose name starts with train, such as train_sft.",
  format: "How the file is read. Auto goes by the file extension, or by the contents when there is none; pick a format if it guesses wrong.",
  paste: "Each prompt becomes one user message. How the text is divided is detected for you; Advanced lets you choose.",
  promptColumn:
    "The column sent to the model as the prompt. Columns that hold model output, such as response, completion or chosen, are never picked for you.",
  systemColumn: "Its text becomes the system message before each prompt. A system prompt set on a run replaces it.",
  turns:
    "First user turn keeps the system message and the first user message, with no text from another model. Whole conversation keeps every turn up to the last user message, including earlier replies written by the dataset's model.",
  splitText:
    "How the text is divided into prompts. Automatic looks for --- lines first, then headings, list items, blank lines and line breaks; otherwise the whole text is one prompt.",
  offsetRows: "Skips this many rows from the start. Rows are counted as they are read, before duplicates are removed.",
  offsetPrompts: "Skips this many prompts from the start, counted before duplicates are removed.",
  limitRows: "Reads at most this many rows. Duplicates are removed afterward, so the set can end up smaller.",
  limitPrompts: "Imports at most this many prompts. Duplicates are removed afterward, so the set can end up smaller.",
  dedupe: "Keeps the first of any prompts whose final user message is exactly the same text.",
  name: "Shown in the prompt set list and when you choose prompts for a run. You can rename the set later from its page.",
} as const

// ------------------------------------------------------------------ Simple mode summary

/** Part of a summary value: plain text, or a name from the data (column, split, config) set in mono. */
export type SummaryPart = string | { code: string }

export interface SummaryRow {
  term: string
  parts: SummaryPart[]
  /** Something to fix before importing. */
  warn?: boolean
}

export interface SummaryState {
  source: "huggingface" | "file" | "paste"
  turns: Turns
  allSplits: boolean
  format: ImportFormat
  offset: number | null
  limit: number | null
  dedupe: boolean
  /** The name the user typed, or null while the set keeps its automatic name. */
  name: string | null
}

export type SummaryPreview = Pick<
  ImportPreview,
  "columns" | "prompt_field" | "system_field" | "split_mode" | "has_conversations" | "dropped" | "hf_splits" | "hf_config" | "hf_split" | "prompts"
>

const nf0 = new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 })

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1)
}

/**
 * What Simple mode shows in place of the settings: where the prompts come from, the split, the
 * system prompt and what is left out. Settings changed in Advanced follow, so Simple never hides a
 * choice that differs from the recommended one.
 */
export function importSummary(p: SummaryPreview, st: SummaryState): SummaryRow[] {
  const rows: SummaryRow[] = []
  const tabular = p.columns.length > 0

  if (!tabular) {
    const how = splitHint(p.split_mode)
    if (how) rows.push({ term: "Prompts", parts: [capitalize(how)] })
  } else if (!p.prompt_field) {
    rows.push({ term: "Prompts", parts: ["No prompt column found. Choose one in Advanced."], warn: true })
  } else if (p.has_conversations) {
    rows.push({ term: "Prompts", parts: [{ code: p.prompt_field }, st.turns === "first_user" ? ", first user turn" : ", whole conversation"] })
  } else {
    rows.push({ term: "Prompts", parts: [{ code: p.prompt_field }, " column"] })
  }

  if (st.source === "huggingface" && p.hf_splits.length > 0) {
    const configs = new Set(p.hf_splits.map((s) => s.config))
    if (configs.size > 1 && p.hf_config) rows.push({ term: "Config", parts: [{ code: p.hf_config }] })
    if (st.allSplits) rows.push({ term: "Split", parts: ["All splits"] })
    else if (p.hf_split) rows.push({ term: "Split", parts: [{ code: p.hf_split }] })
  }

  if (tabular && p.prompt_field) {
    if (p.system_field) rows.push({ term: "System prompt", parts: [{ code: p.system_field }, " column"] })
    else if (p.prompts.some((m) => m[0]?.role === "system")) rows.push({ term: "System prompt", parts: ["From the conversation"] })
    else rows.push({ term: "System prompt", parts: ["None"] })
  }

  if (p.dropped.length > 0) {
    const parts: SummaryPart[] = []
    p.dropped.forEach((d, i) => {
      if (i > 0) parts.push(", ")
      const turns = /^assistant turns in (.+)$/.exec(d)
      if (turns) parts.push("assistant turns in ", { code: turns[1] })
      else parts.push({ code: d })
    })
    rows.push({ term: "Left out", parts })
  }

  if (st.source === "file" && st.format !== "auto") rows.push({ term: "Format", parts: [formatLabel(st.format)] })
  if (st.offset) rows.push({ term: tabular ? "Skip rows" : "Skip prompts", parts: [nf0.format(st.offset)] })
  if (st.limit) rows.push({ term: "Import at most", parts: [nf0.format(st.limit)] })
  if (!st.dedupe) rows.push({ term: "Duplicate prompts", parts: ["Kept"] })
  if (st.name) rows.push({ term: "Name", parts: [st.name] })
  return rows
}
