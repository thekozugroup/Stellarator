import { useEffect, useMemo, useRef, useState } from "react"
import { useNavigate, useSearchParams } from "react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Loader2 } from "lucide-react"
import { Page } from "@/components/shared/page"
import { FormGroup, FormRow } from "@/components/shared/form-group"
import { actionBarClass } from "@/components/shared/action-bar"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { FileFields, HuggingFaceFields, PasteFields } from "@/components/import/source-fields"
import { MappingFields } from "@/components/import/mapping-fields"
import { PreviewPanel } from "@/components/import/preview-panel"
import { DetectedSummary } from "@/components/import/detected-summary"
import { ImportModeToggle } from "@/components/import/import-mode"
import {
  INFO,
  defaultSplit,
  importSummary,
  isDatasetId,
  parseCount,
  parseDataset,
  readImportMode,
  saveImportMode,
  stripExtension,
  type ImportMode,
} from "@/components/import/helpers"
import { api, ApiError } from "@/lib/api"
import { count } from "@/lib/format"
import { cn } from "@/lib/utils"
import type { HFSplit, ImportFormat, ImportMapping, ImportPreview, ImportSource, PromptSet, SplitMode, Turns, Upload } from "@/lib/types"

type SourceTab = "huggingface" | "file" | "paste"
const TABS: { value: SourceTab; label: string }[] = [
  { value: "huggingface", label: "Hugging Face" },
  { value: "file", label: "File" },
  { value: "paste", label: "Paste" },
]

/** A value that follows `value` after `ms` of quiet. */
function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value)
  useEffect(() => {
    const t = window.setTimeout(() => setV(value), ms)
    return () => window.clearTimeout(t)
  }, [value, ms])
  return v
}

export function ImportPromptsPage() {
  const navigate = useNavigate()
  const qc = useQueryClient()
  const [params, setParams] = useSearchParams()
  const tab: SourceTab = TABS.some((t) => t.value === params.get("source")) ? (params.get("source") as SourceTab) : "huggingface"

  // Simple and Advanced show different controls over one shared state.
  const [mode, setModeState] = useState<ImportMode>(readImportMode)
  const simple = mode === "simple"
  const setMode = (m: ImportMode) => {
    setModeState(m)
    saveImportMode(m)
  }

  // Source state. Each source keeps its own state while you switch tabs.
  const [hfInput, setHfInput] = useState("")
  const [hfDataset, setHfDataset] = useState("")
  const [hfConfig, setHfConfig] = useState<string | null>(null)
  const [hfSplit, setHfSplit] = useState<string | null>(null)
  const [allSplits, setAllSplits] = useState(false)
  const [hfInfo, setHfInfo] = useState<{ dataset: string; splits: HFSplit[]; config: string | null; split: string | null } | null>(null)
  const [upload, setUpload] = useState<Upload | null>(null)
  const [format, setFormat] = useState<ImportFormat>("auto")
  const [paste, setPaste] = useState("")
  const debouncedPaste = useDebounced(paste, 400)

  // Mapping state. undefined = untouched, so the server's auto-detection stays in charge.
  const [promptField, setPromptField] = useState<string | null | undefined>(undefined)
  const [systemField, setSystemField] = useState<string | null | undefined>(undefined)
  const [splitMode, setSplitMode] = useState<SplitMode>("auto")
  const [offset, setOffset] = useState("")
  const [limit, setLimit] = useState("")
  const [dedupe, setDedupe] = useState(true)
  const [turns, setTurns] = useState<Turns>("first_user")
  const [detectedPrompt, setDetectedPrompt] = useState<string | null>(null)
  const [name, setName] = useState("")
  const [nameTouched, setNameTouched] = useState(false)

  const resetMapping = () => {
    setPromptField(undefined)
    setSystemField(undefined)
    setSplitMode("auto")
    setOffset("")
    setLimit("")
  }

  const setTab = (next: string) => {
    const p = new URLSearchParams(params)
    p.set("source", next)
    setParams(p, { replace: true })
  }

  const offsetN = parseCount(offset)
  const limitN = parseCount(limit)
  const offsetError = offsetN === undefined ? "Use a whole number, or leave empty." : null
  const limitError = limitN === undefined ? "Use a whole number, or leave empty." : limitN === 0 ? "Use 1 or more, or leave empty to import all." : null

  const mapping: ImportMapping = useMemo(() => {
    const m: ImportMapping = { split_mode: splitMode, turns, offset: offsetN ?? 0, limit: limitN && limitN > 0 ? limitN : null, dedupe }
    if (promptField !== undefined) m.prompt_field = promptField
    // undefined = let the server detect a system column; "" = explicitly none.
    if (systemField === null) m.system_field = ""
    else if (systemField !== undefined) m.system_field = systemField
    return m
  }, [splitMode, turns, offsetN, limitN, dedupe, promptField, systemField, detectedPrompt])
  const debouncedMapping = useDebounced(mapping, 400)

  const upl = useMutation({
    mutationFn: (file: File) => api.upload<Upload>("/api/imports/upload", file),
    onSuccess: (u) => {
      setUpload(u)
      setFormat("auto")
      resetMapping()
    },
  })

  let source: ImportSource | null = null
  let identity = ""
  if (tab === "huggingface" && hfDataset) {
    source = { kind: "huggingface", dataset: hfDataset, config: hfConfig, split: allSplits ? null : hfSplit, all_splits: allSplits }
    identity = `hf:${hfDataset}`
  } else if (tab === "file" && upload) {
    source = { kind: "file", upload_id: upload.upload_id, format }
    identity = `file:${upload.upload_id}`
  } else if (tab === "paste" && debouncedPaste.trim()) {
    source = { kind: "paste", text: debouncedPaste }
    identity = "paste"
  }

  const preview = useQuery({
    queryKey: ["import-preview", identity, source, debouncedMapping],
    queryFn: () => api.post<ImportPreview>("/api/imports/preview", { source, mapping: debouncedMapping }),
    enabled: !!source && !offsetError && !limitError,
    retry: false,
    staleTime: 60_000,
    placeholderData: (prev, prevQuery) => (prevQuery?.queryKey[1] === identity ? prev : undefined),
  })
  const data = preview.data
  useEffect(() => {
    if (data?.prompt_field) setDetectedPrompt(data.prompt_field)
  }, [data?.prompt_field])
  const settling = mapping !== debouncedMapping || (tab === "paste" && paste !== debouncedPaste)
  const updating = !!source && !!data && (preview.isFetching || settling)
  const previewError = preview.error ? (preview.error as Error).message : null

  // Remember the dataset's configs and splits so the selects survive a failed re-preview.
  useEffect(() => {
    if (tab === "huggingface" && data && hfDataset && data.hf_splits.length) {
      setHfInfo({ dataset: hfDataset, splits: data.hf_splits, config: data.hf_config, split: data.hf_split })
    }
  }, [data, tab, hfDataset])
  const info = hfInfo && hfInfo.dataset === hfDataset ? hfInfo : null
  const effConfig = hfConfig ?? info?.config ?? null
  const effSplit = hfSplit ?? info?.split ?? null

  const loadDataset = (value: string = hfInput) => {
    const ds = parseDataset(value)
    if (!ds) return
    setHfInput(ds)
    if (ds !== hfDataset) {
      setHfConfig(null)
      setHfSplit(null)
      setAllSplits(false)
      resetMapping()
      setName("")
      setNameTouched(false)
      setHfDataset(ds)
    } else {
      void preview.refetch()
    }
  }
  // Simple mode loads a dataset by itself once the input names one (a link or owner/name).
  const settledInput = useDebounced(hfInput, 600)
  useEffect(() => {
    if (!simple || tab !== "huggingface") return
    const ds = parseDataset(settledInput)
    if (isDatasetId(ds) && ds !== hfDataset) loadDataset(settledInput)
    // loadDataset changes with every render; the input, mode and tab decide when this runs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settledInput, simple, tab])

  const pickConfig = (c: string) => {
    const splits = info?.splits.filter((s) => s.config === c) ?? []
    const keep = splits.find((s) => s.split === effSplit)?.split
    setHfConfig(c)
    setHfSplit(keep ?? defaultSplit(splits.map((s) => s.split)) ?? null)
    resetMapping()
  }
  const pickSplit = (s: string | null) => {
    setHfConfig(effConfig)
    if (s === null) setAllSplits(true)
    else {
      setAllSplits(false)
      setHfSplit(s)
    }
  }

  // Default name: dataset + split, or the file name; the user's edits win.
  const defaultName =
    tab === "huggingface"
      ? hfDataset
        ? `${hfDataset}${effSplit && !allSplits ? ` ${effSplit}` : ""}`
        : ""
      : tab === "file"
        ? upload
          ? stripExtension(upload.filename)
          : ""
        : paste.trim()
          ? "Pasted prompts"
          : ""
  const shownName = nameTouched ? name : defaultName
  // Simple mode has no name field: a name typed in Advanced is kept, an empty one falls back.
  const importName = simple && !shownName.trim() ? defaultName : shownName

  const commit = useMutation({
    mutationFn: () => {
      const s: ImportSource | null =
        tab === "huggingface" && hfDataset
          ? {
              kind: "huggingface",
              dataset: hfDataset,
              config: data?.hf_config ?? hfConfig,
              split: allSplits ? null : (data?.hf_split ?? hfSplit),
              all_splits: allSplits,
            }
          : tab === "file" && upload
            ? { kind: "file", upload_id: upload.upload_id, format }
            : tab === "paste"
              ? { kind: "paste", text: paste }
              : null
      return api.post<PromptSet>("/api/imports", { source: s, mapping, name: importName.trim() })
    },
    onSuccess: (set) => {
      // The server answers 202 with the set still importing; its page shows the progress.
      qc.setQueryData(["prompt-set", set.id], set)
      void qc.invalidateQueries({ queryKey: ["prompt-sets"] })
      navigate(`/prompts/${set.id}`)
    },
  })

  const commitErr = commit.error instanceof ApiError ? commit.error : null
  const importing = commit.isPending
  const alertRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (commit.error) {
      const bad = document.querySelector<HTMLElement>("[aria-invalid='true']")
      ;(bad ?? alertRef.current)?.focus()
    }
  }, [commit.error])

  const est = data?.estimated_prompts ?? null
  const nameMissing = !importName.trim()
  const noPromptColumn = !!data && data.columns.length > 0 && !data.prompt_field
  const blocked =
    !data || !!previewError || est === 0 || noPromptColumn || nameMissing || !!offsetError || !!limitError || upl.isPending
  const reason = !data
    ? "Choose a source to import."
    : previewError
      ? "Fix the problem in the preview first."
      : noPromptColumn
        ? simple
          ? "Choose a prompt column in Advanced."
          : "Choose a prompt column."
        : est === 0
          ? "Nothing to import with these settings."
          : nameMissing
            ? "Give the prompt set a name."
            : simple && (offsetError || limitError)
              ? "Fix the row limits in Advanced."
              : null
  const label = importing ? "Starting…" : est !== null ? `Import ${count(est, "prompt", "prompts")}` : "Import prompts"

  const hfFieldError = tab === "huggingface" && !data && previewError ? previewError : (commitErr?.fields["source.dataset"] ?? null)
  const fileError = upl.error ? (upl.error as Error).message : tab === "file" && !data && previewError ? previewError : null
  const panelError = tab === "huggingface" && !info ? null : previewError

  const summary =
    simple && data && !previewError
      ? importSummary(data, {
          source: tab,
          turns,
          allSplits,
          format,
          offset: offsetN ?? null,
          limit: limitN ?? null,
          dedupe,
          name: nameTouched && name.trim() && name.trim() !== defaultName ? name.trim() : null,
        })
      : []

  return (
    <Page
      title="Import prompts"
      back={{ to: "/prompts", label: "Prompt sets" }}
      actions={<ImportModeToggle value={mode} onChange={setMode} />}
      wide
    >
      <form
        onSubmit={(e) => {
          e.preventDefault()
          if (!blocked && !importing) commit.mutate()
        }}
        className="grid grid-cols-[minmax(0,1fr)] gap-x-10 gap-y-8 lg:grid-cols-[28rem_minmax(0,1fr)] lg:grid-rows-[auto_auto_1fr]"
      >
        <fieldset disabled={importing} className="m-0 grid min-w-0 grid-cols-[minmax(0,1fr)] content-start gap-4 border-0 p-0">
            <Tabs value={tab} onValueChange={setTab}>
              <TabsList aria-label="Source" className="w-full">
                {TABS.map((t) => (
                  <TabsTrigger key={t.value} value={t.value}>
                    {t.label}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
            {tab === "huggingface" ? (
              <HuggingFaceFields
                input={hfInput}
                onInput={setHfInput}
                onLoad={() => loadDataset()}
                loading={preview.isFetching && !data}
                error={hfFieldError}
                splits={info?.splits ?? []}
                config={effConfig}
                split={effSplit}
                allSplits={allSplits}
                onConfig={pickConfig}
                onSplit={pickSplit}
                showSplits={!simple}
              />
            ) : tab === "file" ? (
              <FileFields
                upload={upload}
                uploading={upl.isPending}
                uploadName={upl.variables?.name ?? null}
                error={fileError}
                onFile={(f) => {
                  setName("")
                  setNameTouched(false)
                  upl.mutate(f)
                }}
                format={format}
                onFormat={setFormat}
                showFormat={!simple}
              />
            ) : (
              <PasteFields value={paste} onChange={setPaste} />
            )}
            {summary.length > 0 ? <DetectedSummary rows={summary} /> : null}
        </fieldset>

        <div className="min-w-0 lg:col-start-2 lg:row-span-3 lg:row-start-1">
          <PreviewPanel
            preview={data}
            hasSource={!!source}
            loading={!!source && !data && preview.isFetching}
            updating={updating}
            error={panelError}
            onRetry={() => void preview.refetch()}
            emptyHint={hfFieldError ? "Fix the dataset above to see its prompts." : undefined}
            showDropped={!simple}
          />
        </div>

        {data && !simple ? (
          <fieldset disabled={importing} className="m-0 grid min-w-0 grid-cols-[minmax(0,1fr)] content-start gap-4 border-0 p-0 lg:col-start-1 lg:row-start-2">
            <MappingFields
              preview={data}
              promptField={promptField}
              systemField={systemField}
              onPromptField={setPromptField}
              onSystemField={setSystemField}
              splitMode={splitMode}
              onSplitMode={setSplitMode}
              offset={offset}
              limit={limit}
              onOffset={setOffset}
              onLimit={setLimit}
              offsetError={offsetError ?? commitErr?.fields["mapping.offset"]}
              limitError={limitError ?? commitErr?.fields["mapping.limit"]}
              dedupe={dedupe}
              onDedupe={setDedupe}
              turns={turns}
              onTurns={setTurns}
            />
            <FormGroup>
              <FormRow id="import-name" label="Name" error={commitErr?.fields["name"]} info={INFO.name}>
                <Input
                  id="import-name"
                  value={shownName}
                  onChange={(e) => {
                    setName(e.target.value)
                    setNameTouched(true)
                  }}
                  aria-invalid={commitErr?.fields["name"] ? true : undefined}
                  aria-describedby={commitErr?.fields["name"] ? "import-name-error" : undefined}
                  maxLength={300}
                />
              </FormRow>
            </FormGroup>
          </fieldset>
        ) : null}

        <div
          className={cn(
            // Under 1024 px a sticky bar in the bar material that runs down behind the tab bar.
            actionBarClass("lg", "grid content-start gap-2 lg:col-start-1"),
            // Simple mode has no mapping row: the action follows the source directly.
            simple ? "lg:row-start-2" : "lg:row-start-3",
          )}
        >
          {commit.error ? (
            <div
              ref={alertRef}
              tabIndex={-1}
              role="alert"
              className="rounded-xl bg-negative-tint px-3 py-2 text-sm text-negative-ink outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {commit.error.message}
            </div>
          ) : null}
          <Button type="submit" disabled={blocked || importing} className="w-full lg:w-fit">
            {importing ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
            {label}
          </Button>
          {reason && !importing ? <p className="text-xs text-muted-foreground">{reason}</p> : null}
        </div>
      </form>
    </Page>
  )
}
