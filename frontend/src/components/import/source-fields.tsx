import { useRef, useState } from "react"
import { FileText, Loader2 } from "lucide-react"
import { FormGroup, FormRow } from "@/components/shared/form-group"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { bytes } from "@/lib/format"
import type { HFSplit, ImportFormat, Upload } from "@/lib/types"
import { cn } from "@/lib/utils"
import { ACCEPT, FORMAT_OPTIONS, INFO, formatLabel } from "./helpers"

const ALL = "__all__"

export function HuggingFaceFields({
  input,
  onInput,
  onLoad,
  loading,
  error,
  splits,
  config,
  split,
  allSplits,
  onConfig,
  onSplit,
  showSplits,
}: {
  input: string
  onInput: (v: string) => void
  onLoad: () => void
  loading: boolean
  error: string | null
  splits: HFSplit[]
  config: string | null
  split: string | null
  allSplits: boolean
  onConfig: (c: string) => void
  /** null = all splits */
  onSplit: (s: string | null) => void
  /** Advanced only: Simple uses the default config and the train split. */
  showSplits: boolean
}) {
  const configs = Array.from(new Set(splits.map((s) => s.config)))
  const inConfig = splits.filter((s) => s.config === config)
  return (
    <FormGroup>
      <FormRow id="hf-dataset" label="Dataset" error={error} info={INFO.dataset} stacked>
        <div className="flex gap-2">
          <Input
            id="hf-dataset"
            value={input}
            onChange={(e) => onInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault()
                onLoad()
              }
            }}
            placeholder="openai/gsm8k"
            autoComplete="off"
            autoCapitalize="none"
            spellCheck={false}
            className="font-mono"
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? "hf-dataset-error" : undefined}
          />
          <Button type="button" variant="secondary" onClick={onLoad} disabled={!input.trim() || loading}>
            {loading ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
            {loading ? "Loading…" : "Load"}
          </Button>
        </div>
      </FormRow>
      {showSplits && splits.length > 0 ? (
        <>
          {configs.length > 1 ? (
            <FormRow id="hf-config" label="Config" info={INFO.config}>
              <Select value={config ?? undefined} onValueChange={onConfig}>
                <SelectTrigger id="hf-config" className="w-full font-mono">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {configs.map((c) => (
                    <SelectItem key={c} value={c}>
                      {c}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </FormRow>
          ) : null}
          <FormRow id="hf-split" label="Split" info={INFO.split}>
            <Select value={allSplits ? ALL : (split ?? undefined)} onValueChange={(v) => onSplit(v === ALL ? null : v)}>
              <SelectTrigger id="hf-split" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {inConfig.map((s) => (
                  <SelectItem key={s.split} value={s.split}>
                    {s.split}
                  </SelectItem>
                ))}
                {inConfig.length > 1 ? <SelectItem value={ALL}>All splits</SelectItem> : null}
              </SelectContent>
            </Select>
          </FormRow>
        </>
      ) : null}
    </FormGroup>
  )
}

export function FileFields({
  upload,
  uploading,
  uploadName,
  error,
  onFile,
  format,
  onFormat,
  showFormat,
}: {
  upload: Upload | null
  uploading: boolean
  uploadName: string | null
  error: string | null
  onFile: (f: File) => void
  format: ImportFormat
  onFormat: (f: ImportFormat) => void
  /** Advanced only: Simple reads the detected format. */
  showFormat: boolean
}) {
  const input = useRef<HTMLInputElement>(null)
  const [over, setOver] = useState(false)
  const has = !!upload || uploading

  return (
    <div className="grid grid-cols-[minmax(0,1fr)] gap-4">
      <div
        onDragOver={(e) => {
          e.preventDefault()
          setOver(true)
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          e.preventDefault()
          setOver(false)
          const f = e.dataTransfer.files?.[0]
          if (f && !uploading) onFile(f)
        }}
        className={cn(
          "relative rounded-xl bg-muted p-4 text-sm transition-colors duration-150 has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring",
          over && "bg-accent",
        )}
      >
        <input
          ref={input}
          id="import-file"
          type="file"
          accept={ACCEPT}
          className="sr-only"
          disabled={uploading}
          onChange={(e) => {
            const f = e.target.files?.[0]
            if (f) onFile(f)
            e.target.value = ""
          }}
        />
        {has ? (
          <div className="flex items-center gap-3">
            <FileText className="size-5 flex-none text-muted-foreground" aria-hidden="true" />
            <div className="min-w-0 flex-1">
              <p className="truncate font-mono" title={upload?.filename ?? uploadName ?? undefined}>
                {upload?.filename ?? uploadName}
              </p>
              <p className="text-xs text-muted-foreground" role="status">
                {uploading ? "Uploading…" : `${bytes(upload!.size)} · detected ${formatLabel(upload!.detected_format)}`}
              </p>
            </div>
            <Button type="button" variant="secondary" size="sm" disabled={uploading} asChild>
              <label htmlFor="import-file" className="cursor-pointer pointer-coarse:min-h-11">
                Replace file
              </label>
            </Button>
          </div>
        ) : (
          <div className="flex flex-col items-start gap-2">
            <label htmlFor="import-file" className="cursor-pointer font-medium after:absolute after:inset-0 after:content-['']">
              Choose a file
            </label>
            <p className="text-xs text-muted-foreground">
              or drop it here. Text, Markdown, CSV, TSV, JSON, JSONL or Parquet.
            </p>
          </div>
        )}
      </div>
      {error ? (
        <p role="alert" className="-mt-2 text-sm text-destructive">
          {error}
        </p>
      ) : null}
      {showFormat && upload ? (
        <FormGroup>
        <FormRow id="import-format" label="Format" info={INFO.format}>
          <Select value={format} onValueChange={(v) => onFormat(v as ImportFormat)}>
            <SelectTrigger id="import-format" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {FORMAT_OPTIONS.map((o) => (
                <SelectItem key={o.value} value={o.value}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </FormRow>
        </FormGroup>
      ) : null}
    </div>
  )
}

export function PasteFields({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <FormGroup>
    <FormRow id="paste-text" label="Prompts" hint="Separate prompts with ---, a blank line or headings." info={INFO.paste} stacked>
      <Textarea
        id="paste-text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={"Explain how a stellarator confines plasma.\n---\nWrite a haiku about copper coils."}
        rows={10}
        spellCheck={false}
        className="min-h-48 font-mono"
        aria-describedby="paste-text-hint"
      />
    </FormRow>
    </FormGroup>
  )
}
