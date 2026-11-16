import { useEffect, useState, type ReactNode } from "react"
import { keepPreviousData, useQuery } from "@tanstack/react-query"
import { CheckOption, ChoiceGroup, ExportDialogShell, FileTypeGroup, OptionGroup, PLACEMENT_LABEL } from "@/components/export/export-dialog-shell"
import { api, qs } from "@/lib/api"
import type { ExportOptions, ExportPreview, HubPushIn, HubPushOut, ReasoningPlacement, Run, SFTFormat } from "@/lib/types"

const mono = (s: string) => <code className="font-mono text-[0.92em]">{s}</code>

const FORMATS: { value: SFTFormat; label: string; hint: string }[] = [
  { value: "messages", label: "Chat messages", hint: "For TRL, Axolotl, Unsloth." },
  { value: "sharegpt", label: "ShareGPT", hint: "For LLaMA-Factory, Axolotl." },
  { value: "alpaca", label: "Alpaca", hint: "Instruction, input and output fields." },
  { value: "prompt_completion", label: "Prompt-completion", hint: "For TRL." },
  { value: "raw", label: "Raw", hint: "Every field, with tokens and timings." },
]

const PLACEMENTS: { value: ReasoningPlacement; label: ReactNode; hint?: ReactNode }[] = [
  { value: "think_tags", label: PLACEMENT_LABEL.think_tags, hint: "Before the answer, in the assistant text." },
  { value: "reasoning_content", label: PLACEMENT_LABEL.reasoning_content, hint: "As vLLM and DeepSeek return it." },
  { value: "thinking", label: PLACEMENT_LABEL.thinking, hint: <>For {mono("gpt-oss")} templates.</> },
  { value: "omit", label: PLACEMENT_LABEL.omit },
]

const DEFAULT_OPTIONS: ExportOptions = {
  format: "messages",
  file_type: "jsonl",
  reasoning: "think_tags",
  include_system: true,
  exclude_truncated: true,
  require_reasoning: false,
  first_sample_only: false,
}

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value)
  useEffect(() => {
    const id = window.setTimeout(() => setV(value), ms)
    return () => window.clearTimeout(id)
  }, [value, ms])
  return v
}

/** Export a run as a training dataset. Every option change refreshes the preview. */
export function ExportDialog({ run, open, onOpenChange }: { run: Run; open: boolean; onOpenChange: (open: boolean) => void }) {
  const [options, setOptions] = useState<ExportOptions>(DEFAULT_OPTIONS)
  const debounced = useDebounced(options, 250)
  const preview = useQuery({ // taste-ignore: missing-states -- ExportDialogShell draws the loading, empty and error states
    queryKey: ["runs", run.id, "export-preview", debounced],
    queryFn: () => api.post<ExportPreview>(`/api/runs/${run.id}/export/preview`, debounced),
    enabled: open,
    placeholderData: keepPreviousData,
  })
  const set = <K extends keyof ExportOptions>(k: K, v: ExportOptions[K]) => setOptions((o) => ({ ...o, [k]: v }))
  const singleSample = run.params.samples_per_prompt === 1

  return (
    <ExportDialogShell
      title="Export dataset"
      open={open}
      onOpenChange={onOpenChange}
      preview={preview}
      stale={preview.isFetching || debounced !== options}
      emptyText="No samples match these options."
      downloadHref={`/api/runs/${run.id}/export${qs({ ...options })}`}
      fileType={options.file_type}
      push={(target: HubPushIn) => api.post<HubPushOut>(`/api/runs/${run.id}/export/hub`, { options, target })}
      repoExample="your-name/qwen3-math-distill"
    >
      <ChoiceGroup legend="Format" value={options.format} onChange={(v) => set("format", v)} choices={FORMATS} />
      <ChoiceGroup legend="Thinking trace" value={options.reasoning} onChange={(v) => set("reasoning", v)} choices={PLACEMENTS} />
      <OptionGroup legend="Samples">
        <CheckOption checked={options.exclude_truncated} onChange={(v) => set("exclude_truncated", v)} label="Skip truncated responses" />
        <CheckOption checked={options.require_reasoning} onChange={(v) => set("require_reasoning", v)} label="Only samples with a thinking trace" />
        {/* A run that takes one sample per prompt has no others to leave out. */}
        {!singleSample ? (
          <CheckOption checked={options.first_sample_only} onChange={(v) => set("first_sample_only", v)} label="First sample per prompt only" />
        ) : null}
        <CheckOption checked={options.include_system} onChange={(v) => set("include_system", v)} label="Include system prompt" />
      </OptionGroup>
      <FileTypeGroup value={options.file_type} onChange={(v) => set("file_type", v)} />
    </ExportDialogShell>
  )
}
