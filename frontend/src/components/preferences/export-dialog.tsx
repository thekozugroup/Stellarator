import { useState, type ReactNode } from "react"
import { keepPreviousData, useQuery } from "@tanstack/react-query"
import { CheckOption, ChoiceGroup, ExportDialogShell, FileTypeGroup, OptionGroup, PLACEMENT_LABEL } from "@/components/export/export-dialog-shell"
import { api, qs } from "@/lib/api"
import type { ExportPreview, HubPushIn, HubPushOut, PrefExportOptions, PrefFormat, PreferenceSet, ReasoningPlacement } from "@/lib/types"

const mono = (s: string) => <code className="font-mono text-[0.92em]">{s}</code>

const FORMATS: { value: PrefFormat; label: string; hint: ReactNode }[] = [
  { value: "dpo", label: "DPO", hint: <>{mono("prompt")}, {mono("chosen")}, {mono("rejected")}. 1 record per pair.</> },
  { value: "kto", label: "KTO", hint: <>{mono("prompt")}, {mono("completion")}, {mono("label")}. 2 records per pair.</> },
]

const SHAPES: { value: "conversational" | "plain"; label: string; hint: string }[] = [
  { value: "conversational", label: "Conversational", hint: "Message lists." },
  { value: "plain", label: "Plain text", hint: "Strings." },
]

const REASONING: { value: ReasoningPlacement; label: ReactNode }[] = (
  ["think_tags", "reasoning_content", "thinking", "omit"] as const
).map((value) => ({ value, label: PLACEMENT_LABEL[value] }))

const DEFAULT_OPTIONS: PrefExportOptions = {
  format: "dpo",
  file_type: "jsonl",
  reasoning: "think_tags",
  include_system: true,
  conversational: true,
}

export function ExportPairsDialog({
  set,
  open,
  onOpenChange,
}: {
  set: PreferenceSet
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [options, setOptions] = useState<PrefExportOptions>(DEFAULT_OPTIONS)
  const update = (patch: Partial<PrefExportOptions>) => setOptions((o) => ({ ...o, ...patch }))
  const preview = useQuery({ // taste-ignore: missing-states -- ExportDialogShell draws the loading, empty and error states
    queryKey: ["preference-export-preview", set.id, options],
    queryFn: () => api.post<ExportPreview>(`/api/preference-sets/${set.id}/export/preview`, options),
    enabled: open,
    placeholderData: keepPreviousData,
  })

  return (
    <ExportDialogShell
      title="Export pairs"
      open={open}
      onOpenChange={onOpenChange}
      preview={preview}
      stale={preview.isFetching}
      emptyText="No pairs yet."
      downloadHref={`/api/preference-sets/${set.id}/export${qs({ ...options })}`}
      fileType={options.file_type}
      push={(target: HubPushIn) => api.post<HubPushOut>(`/api/preference-sets/${set.id}/export/hub`, { options, target })}
      repoExample="your-name/math-dpo-pairs"
    >
      <ChoiceGroup legend="Format" value={options.format} onChange={(v) => update({ format: v })} choices={FORMATS} />
      <ChoiceGroup
        legend="Shape"
        value={options.conversational ? "conversational" : "plain"}
        onChange={(v) => update({ conversational: v === "conversational" })}
        choices={SHAPES}
      />
      <ChoiceGroup legend="Thinking trace" value={options.reasoning} onChange={(v) => update({ reasoning: v })} choices={REASONING} />
      <OptionGroup legend="Prompts">
        <CheckOption checked={options.include_system} onChange={(v) => update({ include_system: v })} label="Include system prompt" />
      </OptionGroup>
      <FileTypeGroup value={options.file_type} onChange={(v) => update({ file_type: v })} />
    </ExportDialogShell>
  )
}
