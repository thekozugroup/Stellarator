import { useRef, useState } from "react"
import { FileText, Loader2 } from "lucide-react"
import { Field } from "@/components/shared/field"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { bytes, int } from "@/lib/format"
import type { HFSplit, ImportFormat, Upload } from "@/lib/types"
import { cn } from "@/lib/utils"
import { ACCEPT, FORMAT_OPTIONS, formatLabel } from "./helpers"

export function HuggingFaceFields({
  input,
  onInput,
  onLoad,
  loading,
  error,
  splits,
  config,
  split,
  onConfig,
  onSplit,
}: {
  input: string
  onInput: (v: string) => void
  onLoad: () => void
  loading: boolean
  error: string | null
  splits: HFSplit[]
  config: string | null
  split: string | null
  onConfig: (c: string) => void
  onSplit: (s: string) => void
}) {
  const configs = Array.from(new Set(splits.map((s) => s.config)))
  const inConfig = splits.filter((s) => s.config === config)
  return (
    <div className="grid gap-4">
      <Field
        id="hf-dataset"
        label="Dataset"
        hint="A public dataset, or a gated one if your token in Settings has access."
        error={error}
      >
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
            aria-describedby={error ? "hf-dataset-error" : "hf-dataset-hint"}
          />
          <Button type="button" variant="outline" onClick={onLoad} disabled={!input.trim() || loading}>
            {loading ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
            {loading ? "Loading…" : "Load"}
          </Button>
        </div>
      </Field>
      {splits.length > 0 ? (
        <div className={cn("grid gap-4", configs.length > 1 && "sm:grid-cols-2")}>
          {configs.length > 1 ? (
            <Field id="hf-config" label="Config">
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
            </Field>
          ) : null}
          <Field id="hf-split" label="Split">
            <Select value={split ?? undefined} onValueChange={onSplit}>
              <SelectTrigger id="hf-split" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {inConfig.map((s) => (
                  <SelectItem key={s.split} value={s.split}>
                    {s.split}
                    {s.num_rows !== null ? <span className="text-muted-foreground"> · {int(s.num_rows)} rows</span> : null}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
        </div>
      ) : null}
    </div>
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
}: {
  upload: Upload | null
  uploading: boolean
  uploadName: string | null
  error: string | null
  onFile: (f: File) => void
  format: ImportFormat
  onFormat: (f: ImportFormat) => void
}) {
  const input = useRef<HTMLInputElement>(null)
  const [over, setOver] = useState(false)
  const has = !!upload || uploading

  return (
    <div className="grid gap-4">
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
          "relative rounded-lg border border-dashed border-input px-4 py-5 text-sm transition-colors has-[:focus-visible]:border-ring has-[:focus-visible]:ring-3 has-[:focus-visible]:ring-ring",
          over && "border-foreground bg-muted",
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
              <p className="truncate font-mono text-[13px]" title={upload?.filename ?? uploadName ?? undefined}>
                {upload?.filename ?? uploadName}
              </p>
              <p className="text-xs text-muted-foreground" role="status">
                {uploading ? "Uploading…" : `${bytes(upload!.size)} · detected ${formatLabel(upload!.detected_format)}`}
              </p>
            </div>
            <Button type="button" variant="outline" size="sm" disabled={uploading} asChild>
              <label htmlFor="import-file" className="cursor-pointer">
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
      {upload ? (
        <Field id="import-format" label="Format" hint="Auto picks from the file's extension and content.">
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
        </Field>
      ) : null}
    </div>
  )
}

export function PasteFields({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <Field id="paste-text" label="Prompts" hint="One prompt per block. Separate prompts with a --- line, a blank line or headings.">
      <Textarea
        id="paste-text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={"Explain how a stellarator confines plasma.\n---\nWrite a haiku about copper coils."}
        rows={10}
        spellCheck={false}
        className="min-h-48 font-mono text-[13px] md:text-[13px]"
        aria-describedby="paste-text-hint"
      />
    </Field>
  )
}
