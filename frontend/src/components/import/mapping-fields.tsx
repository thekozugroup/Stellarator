import { FormGroup, FormItem, FormRow } from "@/components/shared/form-group"
import { InfoTip } from "@/components/shared/info-tip"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { ColumnInfo, ImportPreview, SplitMode, Turns } from "@/lib/types"
import { INFO, SPLIT_OPTIONS, splitHint } from "./helpers"

const NONE = "__none__"

function kindLabel(c: ColumnInfo): string {
  return c.kind === "messages" ? "conversation" : c.kind
}

function ColumnSelect({
  id,
  label,
  columns,
  value,
  onChange,
  optional,
  info,
}: {
  id: string
  label: string
  columns: ColumnInfo[]
  value: string | null
  onChange: (v: string | null) => void
  optional?: boolean
  info: string
}) {
  return (
    <FormRow id={id} label={label} info={info}>
      <Select value={value ?? NONE} onValueChange={(v) => onChange(v === NONE ? null : v)}>
        <SelectTrigger id={id} className="w-full">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {optional ? <SelectItem value={NONE}>None</SelectItem> : null}
          {[...columns.filter((c) => !c.looks_like_output), ...columns.filter((c) => c.looks_like_output)].map((c) => (
            <SelectItem key={c.name} value={c.name}>
              <span className="font-mono text-xs">{c.name}</span>
              <span className="text-muted-foreground"> · {c.looks_like_output ? "output" : kindLabel(c)}</span>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </FormRow>
  )
}

export function MappingFields({
  preview,
  promptField,
  systemField,
  onPromptField,
  onSystemField,
  splitMode,
  onSplitMode,
  offset,
  limit,
  onOffset,
  onLimit,
  offsetError,
  limitError,
  dedupe,
  onDedupe,
  turns,
  onTurns,
}: {
  preview: ImportPreview
  promptField: string | null | undefined
  systemField: string | null | undefined
  onPromptField: (v: string | null) => void
  onSystemField: (v: string | null) => void
  splitMode: SplitMode
  onSplitMode: (m: SplitMode) => void
  offset: string
  limit: string
  onOffset: (v: string) => void
  onLimit: (v: string) => void
  offsetError?: string | null
  limitError?: string | null
  dedupe: boolean
  onDedupe: (v: boolean) => void
  turns: Turns
  onTurns: (t: Turns) => void
}) {
  const tabular = preview.columns.length > 0
  const autoHint = splitMode === "auto" && splitHint(preview.split_mode) ? `Automatic: ${splitHint(preview.split_mode)}.` : undefined
  return (
    <>
      <FormGroup title={tabular ? "Mapping" : undefined}>
        {tabular ? (
          <>
            <ColumnSelect
              id="map-prompt"
              label="Prompt column"
              columns={preview.columns}
              value={promptField === undefined ? preview.prompt_field : promptField}
              onChange={onPromptField}
              info={INFO.promptColumn}
            />
            <ColumnSelect
              id="map-system"
              label="System prompt column"
              columns={preview.columns}
              value={systemField === undefined ? preview.system_field : systemField}
              onChange={onSystemField}
              optional
              info={INFO.systemColumn}
            />
            {preview.has_conversations ? (
              <FormRow group label="Prompt" info={INFO.turns} align="end">
                <Tabs value={turns} onValueChange={(v) => onTurns(v as Turns)}>
                  <TabsList aria-label="Prompt" className="w-full">
                    <TabsTrigger value="first_user">First user turn</TabsTrigger>
                    <TabsTrigger value="up_to_last_user">Whole conversation</TabsTrigger>
                  </TabsList>
                </Tabs>
              </FormRow>
            ) : null}
          </>
        ) : (
          <FormRow id="map-split" label="Split into prompts" hint={autoHint} info={INFO.splitText}>
            <Select value={splitMode} onValueChange={(v) => onSplitMode(v as SplitMode)}>
              <SelectTrigger id="map-split" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {SPLIT_OPTIONS.map((o) => (
                  <SelectItem key={o.value} value={o.value}>
                    {o.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </FormRow>
        )}
      </FormGroup>
      <FormGroup>
        <FormRow
          id="map-offset"
          label={tabular ? "Skip rows" : "Skip prompts"}
          error={offsetError}
          info={tabular ? INFO.offsetRows : INFO.offsetPrompts}
        >
          <Input
            id="map-offset"
            inputMode="numeric"
            value={offset}
            onChange={(e) => onOffset(e.target.value)}
            className="tabular"
            aria-invalid={offsetError ? true : undefined}
            aria-describedby={offsetError ? "map-offset-error" : undefined}
          />
        </FormRow>
        <FormRow id="map-limit" label="Import at most" error={limitError} info={tabular ? INFO.limitRows : INFO.limitPrompts}>
          <Input
            id="map-limit"
            inputMode="numeric"
            value={limit}
            onChange={(e) => onLimit(e.target.value)}
            placeholder="All"
            className="tabular"
            aria-invalid={limitError ? true : undefined}
            aria-describedby={limitError ? "map-limit-error" : undefined}
          />
        </FormRow>
        <FormItem className="flex items-center gap-1">
          <Label htmlFor="map-dedupe" className="flex min-h-7 items-center gap-2.5 text-sm font-normal pointer-coarse:min-h-11">
            <Checkbox id="map-dedupe" checked={dedupe} onCheckedChange={(v) => onDedupe(v === true)} />
            Remove duplicate prompts
          </Label>
          <InfoTip label="Remove duplicate prompts">{INFO.dedupe}</InfoTip>
        </FormItem>
      </FormGroup>
    </>
  )
}
