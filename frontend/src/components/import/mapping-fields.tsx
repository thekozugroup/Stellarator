import { Field } from "@/components/shared/field"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { int } from "@/lib/format"
import type { ColumnInfo, ImportPreview, SplitMode } from "@/lib/types"
import { SPLIT_OPTIONS, splitHint } from "./helpers"

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
}: {
  id: string
  label: string
  columns: ColumnInfo[]
  value: string | null
  onChange: (v: string | null) => void
  optional?: boolean
}) {
  return (
    <Field id={id} label={label}>
      <Select value={value ?? NONE} onValueChange={(v) => onChange(v === NONE ? null : v)}>
        <SelectTrigger id={id} className="w-full">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {optional ? <SelectItem value={NONE}>None</SelectItem> : null}
          {columns.map((c) => (
            <SelectItem key={c.name} value={c.name}>
              <span className="font-mono text-[13px]">{c.name}</span>
              <span className="text-muted-foreground"> · {kindLabel(c)}</span>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </Field>
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
}) {
  const tabular = preview.columns.length > 0
  const autoHint = splitMode === "auto" && splitHint(preview.split_mode) ? `Automatic: ${splitHint(preview.split_mode)}.` : undefined
  return (
    <div className="grid gap-4">
      {tabular ? (
        <>
          <ColumnSelect
            id="map-prompt"
            label="Prompt column"
            columns={preview.columns}
            value={promptField === undefined ? preview.prompt_field : promptField}
            onChange={onPromptField}
          />
          <ColumnSelect
            id="map-system"
            label="System prompt column"
            columns={preview.columns}
            value={systemField === undefined ? preview.system_field : systemField}
            onChange={onSystemField}
            optional
          />
        </>
      ) : (
        <Field id="map-split" label="Split into prompts" hint={autoHint}>
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
        </Field>
      )}
      <div className="grid grid-cols-2 items-start gap-4">
        <Field id="map-offset" label="Skip rows" error={offsetError}>
          <Input
            id="map-offset"
            inputMode="numeric"
            value={offset}
            onChange={(e) => onOffset(e.target.value)}
            className="tabular"
            aria-invalid={offsetError ? true : undefined}
            aria-describedby={offsetError ? "map-offset-error" : undefined}
          />
        </Field>
        <Field
          id="map-limit"
          label="Import at most"
          error={limitError}
          hint={preview.total_rows !== null ? `of ${int(preview.total_rows)} rows` : undefined}
        >
          <Input
            id="map-limit"
            inputMode="numeric"
            value={limit}
            onChange={(e) => onLimit(e.target.value)}
            placeholder="All"
            className="tabular"
            aria-invalid={limitError ? true : undefined}
            aria-describedby={limitError ? "map-limit-error" : preview.total_rows !== null ? "map-limit-hint" : undefined}
          />
        </Field>
      </div>
      <div className="flex items-center gap-2.5">
        <Checkbox id="map-dedupe" checked={dedupe} onCheckedChange={(v) => onDedupe(v === true)} />
        <Label htmlFor="map-dedupe" className="text-sm font-medium">
          Remove duplicate prompts
        </Label>
      </div>
    </div>
  )
}
