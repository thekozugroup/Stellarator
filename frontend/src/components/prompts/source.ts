import type { PromptSet } from "@/lib/types"

function str(v: unknown): string | null {
  return typeof v === "string" && v.trim() ? v.trim() : null
}

/** "Hugging Face · openai/gsm8k · main/train", "File · prompts.md", "Pasted". */
export function describeSource(set: Pick<PromptSet, "source_kind" | "source">): string {
  const s = set.source ?? {}
  if (set.source_kind === "huggingface") {
    const config = str(s.config)
    const split = str(s.split)
    const where = [config, split].filter(Boolean).join("/")
    return ["Hugging Face", str(s.dataset), where].filter(Boolean).join(" · ")
  }
  if (set.source_kind === "file") return ["File", str(s.filename)].filter(Boolean).join(" · ")
  return "Pasted"
}

export interface SourceParts {
  /** "Hugging Face", "File" or "Pasted". */
  kind: string
  /** The dataset ID or file name: data, set in mono. Null for pasted text. */
  id: string | null
  /** "main/train" for a Hugging Face set. */
  where: string | null
}

/** describeSource split into the words and the data, so a screen can set the data in mono. */
export function sourceParts(set: Pick<PromptSet, "source_kind" | "source">): SourceParts {
  const s = set.source ?? {}
  if (set.source_kind === "huggingface") {
    const where = [str(s.config), str(s.split)].filter(Boolean).join("/")
    return { kind: "Hugging Face", id: str(s.dataset), where: where || null }
  }
  if (set.source_kind === "file") return { kind: "File", id: str(s.filename), where: null }
  return { kind: "Pasted", id: null, where: null }
}
