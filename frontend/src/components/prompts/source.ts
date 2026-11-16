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
