import { TextModeToggle } from "@/components/shared/model-text"
import { Fragment, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react"
import { Link, useParams, useSearchParams } from "react-router"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { ChevronLeft, ChevronRight, Ellipsis, Trash2 } from "lucide-react"
import { AutoPairDialog } from "@/components/preferences/auto-pair-dialog"
import { CandidateCard, type Mark } from "@/components/preferences/candidate-card"
import { DeleteSetDialog } from "@/components/preferences/delete-set-dialog"
import { ExportPairsDialog } from "@/components/preferences/export-dialog"
import { PromptView } from "@/components/preferences/prompt-view"
import {
  fetchItemAt,
  letter,
  patchCachedItem,
  prefKeys,
  shortModel,
  usePreferenceSet,
  type ItemFilter,
} from "@/components/preferences/queries"
import { Bar, EmptyBlock, ErrorBlock } from "@/components/shared/block-states"
import { Page } from "@/components/shared/page"
import { ProgressBar } from "@/components/shared/progress-bar"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Kbd } from "@/components/ui/kbd"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { api, ApiError } from "@/lib/api"
import { int } from "@/lib/format"
import type { Candidate, Page as PageOf, PreferenceItem, PreferenceSet } from "@/lib/types"

/** How long a finished pair stays on screen before the next prompt replaces it. */
const BEAT_MS = 450

type Sel = { chosen: number | null; rejected: number | null }
type SaveState =
  | { promptId: number; status: "saving" | "clearing" | "saved" | "cleared" }
  | { promptId: number; status: "error"; op: "save" | "clear"; message: string }

const FILTER_LABEL: Record<ItemFilter, string> = { undecided: "Undecided", decided: "Decided", all: "All" }
const FILTER_NOUN: Record<ItemFilter, string> = { undecided: "undecided", decided: "decided", all: "prompts" }

function parseFilter(v: string | null): ItemFilter {
  return v === "decided" || v === "all" ? v : "undecided"
}

function matchesFilter(item: PreferenceItem, filter: ItemFilter): boolean {
  if (filter === "all") return true
  return (filter === "decided") === (item.chosen_sample_id != null)
}

/**
 * Radio semantics across the cards: one chosen and one rejected per prompt. Marking the response
 * that holds the other mark swaps the two; with exactly two responses the other one takes the
 * opposite mark at once.
 */
function nextSelection(sel: Sel, mark: Mark, id: number, candidates: Candidate[]): Sel {
  const other: Mark = mark === "chosen" ? "rejected" : "chosen"
  if (sel[mark] === id && (candidates.length !== 2 || sel[other] != null)) return sel
  const next: Sel = { ...sel }
  if (next[other] === id) next[other] = next[mark]
  next[mark] = id
  if (candidates.length === 2) next[other] = candidates.find((c) => c.sample_id !== id)?.sample_id ?? null
  return next
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : "The request failed."
}

// --------------------------------------------------------------------------- page

export function PreferenceReviewPage() {
  const { prefId } = useParams()
  const setId = Number(prefId)
  const setQ = usePreferenceSet(setId)

  if (!Number.isFinite(setId) || (setQ.isError && setQ.error instanceof ApiError && setQ.error.status === 404)) {
    return (
      <Page title="Preference set not found" back={{ to: "/preferences", label: "Preferences" }}>
        <EmptyBlock
          action={
            <Button asChild variant="outline">
              <Link to="/preferences">Open preference sets</Link>
            </Button>
          }
        >
          This preference set doesn't exist. It may have been deleted.
        </EmptyBlock>
      </Page>
    )
  }
  if (setQ.isError) {
    return (
      <Page title="Preference set" back={{ to: "/preferences", label: "Preferences" }}>
        <ErrorBlock message={`Couldn't load this preference set. ${setQ.error.message}`} onRetry={() => setQ.refetch()} />
      </Page>
    )
  }
  if (setQ.isPending) {
    return (
      <Page
        title={<Bar className="h-7 w-72 max-w-full align-middle" />}
        meta={<Bar className="h-4 w-96 max-w-full" />}
        back={{ to: "/preferences", label: "Preferences" }}
      >
        <div aria-busy="true" aria-label="Loading preference set…">
          <div className="mb-6 flex flex-wrap items-center justify-between gap-3">
            <Bar className="h-8 w-80 max-w-full" />
            <Bar className="h-8 w-64" />
          </div>
          <ReviewSkeleton />
        </div>
      </Page>
    )
  }
  return <Review set={setQ.data} />
}

// --------------------------------------------------------------------------- review

function Review({ set }: { set: PreferenceSet }) {
  const qc = useQueryClient()
  const [params, setParams] = useSearchParams()
  const filter = parseFilter(params.get("filter"))
  const i = Math.max(0, Number.parseInt(params.get("i") ?? "0", 10) || 0)

  const [autoOpen, setAutoOpen] = useState(false)
  const [exportOpen, setExportOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)

  const itemQ = useQuery({ // taste-ignore: missing-states -- an empty queue renders ReviewEmpty (no prompts, none decided, all decided)
    queryKey: prefKeys.item(set.id, filter, i),
    queryFn: () => fetchItemAt(set.id, filter, i),
    // Keep the last prompt on screen while the next loads, but never across filters.
    placeholderData: (prev, prevQuery) => (prevQuery?.queryKey[2] === filter ? prev : undefined),
  })
  const page = itemQ.data
  const isPlaceholder = itemQ.isPlaceholderData
  const item = page?.items[0] ?? null
  const total = page?.total ?? 0

  const [drafts, setDrafts] = useState<Record<number, Sel>>({})
  const [saveState, setSaveState] = useState<SaveState | null>(null)
  const seq = useRef(0)
  const advanceTimer = useRef<number | undefined>(undefined)

  const sel: Sel = item
    ? (drafts[item.prompt_id] ?? { chosen: item.chosen_sample_id, rejected: item.rejected_sample_id })
    : { chosen: null, rejected: null }
  const status = saveState && item && saveState.promptId === item.prompt_id ? saveState : null
  const left = item ? !matchesFilter(item, filter) : false

  // Latest values for timers and the global key handler.
  const live = useRef({ item, filter, i, total, sel, isPlaceholder })
  useLayoutEffect(() => {
    live.current = { item, filter, i, total, sel, isPlaceholder }
  })

  function setPosition(n: number) {
    setParams(
      (prev) => {
        const p = new URLSearchParams(prev)
        if (n > 0) p.set("i", String(n))
        else p.delete("i")
        return p
      },
      { replace: true },
    )
  }

  function setFilter(f: ItemFilter) {
    cancelAdvance()
    setParams((prev) => {
      const p = new URLSearchParams(prev)
      if (f === "undecided") p.delete("filter")
      else p.set("filter", f)
      p.delete("i")
      return p
    })
  }

  function cancelAdvance() {
    window.clearTimeout(advanceTimer.current)
    advanceTimer.current = undefined
  }
  useEffect(() => () => window.clearTimeout(advanceTimer.current), [])

  // A position past the end (after the last prompt left the filter, or a stale link): wrap the
  // undecided queue to its start, otherwise go to the last prompt.
  useEffect(() => {
    if (!page || isPlaceholder || page.total === 0 || i < page.total) return
    setPosition(filter === "undecided" ? 0 : page.total - 1)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [page, isPlaceholder, i, filter])

  // Prefetch the neighbours so J and K feel instant.
  useEffect(() => {
    if (!page || isPlaceholder) return
    for (const n of [i + 1, i - 1]) {
      if (n >= 0 && n < page.total) {
        qc.prefetchQuery({ queryKey: prefKeys.item(set.id, filter, n), queryFn: () => fetchItemAt(set.id, filter, n) })
      }
    }
  }, [page, isPlaceholder, i, filter, set.id, qc])

  /** The prompt at `i` left the current filter, so everything after it moved up by one. */
  function shiftQueue(f: ItemFilter, at: number, count: number) {
    const next = qc.getQueryData<PageOf<PreferenceItem>>(prefKeys.item(set.id, f, at + 1))
    const fresh = next && next.total === count && next.items.length > 0
    qc.setQueryData<PageOf<PreferenceItem>>(prefKeys.item(set.id, f, at), {
      items: fresh ? next.items : [],
      total: Math.max(0, count - 1),
      offset: at,
      limit: 1,
    })
    qc.invalidateQueries({ queryKey: prefKeys.itemsOf(set.id) })
  }

  function go(delta: 1 | -1) {
    cancelAdvance()
    const cur = live.current
    const gone = cur.item ? !matchesFilter(cur.item, cur.filter) : false
    const target = delta === 1 ? (gone ? cur.i : cur.i + 1) : cur.i - 1
    if (target < 0) return
    if (!gone && target >= cur.total) return
    if (gone) shiftQueue(cur.filter, cur.i, cur.total)
    setSaveState(null)
    setPosition(target)
  }

  async function save(promptId: number, s: Sel) {
    if (s.chosen == null || s.rejected == null) return
    const mine = ++seq.current
    cancelAdvance()
    setSaveState({ promptId, status: "saving" })
    try {
      const updated = await api.put<PreferenceItem>(`/api/preference-sets/${set.id}/pairs/${promptId}`, {
        chosen_sample_id: s.chosen,
        rejected_sample_id: s.rejected,
      })
      if (mine !== seq.current) return
      patchCachedItem(qc, set.id, updated)
      qc.invalidateQueries({ queryKey: prefKeys.list })
      const shown = live.current.item?.prompt_id === promptId ? live.current.sel : null
      const settled = !shown || (shown.chosen === s.chosen && shown.rejected === s.rejected)
      setDrafts((d) => {
        const cur = d[promptId]
        if (!cur || (cur.chosen === s.chosen && cur.rejected === s.rejected)) {
          const rest = { ...d }
          delete rest[promptId]
          return rest
        }
        return d
      })
      setSaveState({ promptId, status: "saved" })
      if (settled && shown) {
        advanceTimer.current = window.setTimeout(() => {
          advanceTimer.current = undefined
          if (live.current.item?.prompt_id === promptId) go(1)
        }, BEAT_MS)
      }
    } catch (err) {
      if (mine !== seq.current) return
      setSaveState({ promptId, status: "error", op: "save", message: errorText(err) })
    }
  }

  async function clear() {
    const cur = live.current.item
    if (!cur || live.current.isPlaceholder) return
    cancelAdvance()
    const promptId = cur.prompt_id
    setDrafts((d) => {
      if (!(promptId in d)) return d
      const rest = { ...d }
      delete rest[promptId]
      return rest
    })
    if (cur.chosen_sample_id == null) {
      seq.current++
      setSaveState(null)
      return
    }
    const mine = ++seq.current
    setSaveState({ promptId, status: "clearing" })
    try {
      await api.del(`/api/preference-sets/${set.id}/pairs/${promptId}`)
      if (mine !== seq.current) return
      patchCachedItem(qc, set.id, { ...cur, chosen_sample_id: null, rejected_sample_id: null, source: null })
      qc.invalidateQueries({ queryKey: prefKeys.list })
      setSaveState({ promptId, status: "cleared" })
    } catch (err) {
      if (mine !== seq.current) return
      setSaveState({ promptId, status: "error", op: "clear", message: errorText(err) })
    }
  }

  function pick(n: number, mark: Mark) {
    const { item: cur, sel: curSel, isPlaceholder: loading } = live.current
    if (!cur || loading) return
    const cand = cur.candidates[n]
    if (!cand) return
    cancelAdvance()
    const next = nextSelection(curSel, mark, cand.sample_id, cur.candidates)
    if (next === curSel) return
    const saved = next.chosen === cur.chosen_sample_id && next.rejected === cur.rejected_sample_id
    setDrafts((d) => {
      const rest = { ...d }
      if (saved) delete rest[cur.prompt_id]
      else rest[cur.prompt_id] = next
      return rest
    })
    if (next.chosen != null && next.rejected != null && !saved) {
      void save(cur.prompt_id, next)
    } else {
      setSaveState(null)
    }
  }

  function retry() {
    if (!status || status.status !== "error" || !item) return
    if (status.op === "clear") void clear()
    else void save(item.prompt_id, sel)
  }

  // Global keys: 1-9 chosen, Shift+1-9 rejected, J/K and arrows to move, U to clear.
  const handlers = useRef({ pick, go, clear })
  useLayoutEffect(() => {
    handlers.current = { pick, go, clear }
  })
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey || e.isComposing) return
      const target = e.target instanceof Element ? e.target : null
      if (target?.closest("input, textarea, select, [contenteditable]:not([contenteditable='false'])")) return
      if (document.querySelector("[role='dialog'], [role='alertdialog'], [role='menu'], [role='listbox']")) return
      const digit = /^(?:Digit|Numpad)([1-9])$/.exec(e.code)
      if (digit) {
        e.preventDefault()
        handlers.current.pick(Number(digit[1]) - 1, e.shiftKey ? "rejected" : "chosen")
        return
      }
      if (e.shiftKey) return
      const composite = target?.closest("[role='radiogroup'], [role='tablist'], [data-slot='toggle-group']")
      switch (e.key) {
        case "j":
        case "J":
          handlers.current.go(1)
          break
        case "k":
        case "K":
          handlers.current.go(-1)
          break
        case "ArrowRight":
          if (composite) return
          handlers.current.go(1)
          break
        case "ArrowLeft":
          if (composite) return
          handlers.current.go(-1)
          break
        case "u":
        case "U":
          void handlers.current.clear()
          break
        default:
          return
      }
      e.preventDefault()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  // A new prompt starts at its top, and keyboard focus moves to its question.
  const regionRef = useRef<HTMLElement>(null)
  const headingRef = useRef<HTMLHeadingElement>(null)
  const shownPrompt = item?.prompt_id
  const lastPrompt = useRef<number | undefined>(undefined)
  useEffect(() => {
    if (shownPrompt == null) return
    if (lastPrompt.current !== undefined && lastPrompt.current !== shownPrompt) {
      const region = regionRef.current
      if (region && region.getBoundingClientRect().top < 0) region.scrollIntoView({ block: "start" })
      const active = document.activeElement
      if (!active || active === document.body || region?.contains(active)) headingRef.current?.focus({ preventScroll: true })
    }
    lastPrompt.current = shownPrompt
  }, [shownPrompt])

  useEffect(() => {
    document.title = `${set.name} · Stellarator`
    return () => {
      document.title = "Stellarator"
    }
  }, [set.name])

  const undecided = Math.max(0, set.prompt_count - set.decided)
  const counts: Record<ItemFilter, number> = { undecided, decided: set.decided, all: set.prompt_count }
  const nextDisabled = !left && i + 1 >= total
  const showSampleFor = new Set(
    (item?.candidates ?? [])
      .map((c) => c.run_id)
      .filter((rid, n, all) => all.indexOf(rid) !== n),
  )

  let body: ReactNode
  if (itemQ.isError && !page) {
    body = <ErrorBlock message={`Couldn't load this prompt. ${itemQ.error.message}`} onRetry={() => itemQ.refetch()} />
  } else if (!page || (!item && i < total)) {
    body = <ReviewSkeleton withHeading />
  } else if (!item) {
    body = (
      <ReviewEmpty
        set={set}
        filter={filter}
        onExport={() => setExportOpen(true)}
        onFilter={setFilter}
      />
    )
  } else {
    body = (
      <Fragment key={item.prompt_id}>
        <div className="mb-3 flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
          <h2 id="review-question" ref={headingRef} tabIndex={-1} className="text-base font-semibold focus:outline-none">
            Which response is better for this prompt?
          </h2>
          <div className="flex flex-wrap items-center gap-3">
            <p className="tabular text-xs text-muted-foreground">
              Prompt {int(item.prompt_idx + 1)} in {set.prompt_set_name}
            </p>
            <TextModeToggle />
          </div>
        </div>
        <PromptView messages={item.messages} />
        <div className="mt-4 grid items-start gap-4 lg:grid-cols-2">
          {item.candidates.map((c, n) => (
            <CandidateCard
              key={c.sample_id}
              candidate={c}
              label={letter(n)}
              position={n + 1}
              mark={sel.chosen === c.sample_id ? "chosen" : sel.rejected === c.sample_id ? "rejected" : null}
              showSample={showSampleFor.has(c.run_id)}
              disabled={isPlaceholder}
              onMark={(m) => pick(n, m)}
            />
          ))}
        </div>
        <KeyboardHelp count={item.candidates.length} />
      </Fragment>
    )
  }

  return (
    <Page
      title={set.name}
      back={{ to: "/preferences", label: "Preferences" }}
      meta={<SetMeta set={set} />}
      actions={
        <>
          {set.runs.length >= 2 ? (
            <Button variant="outline" onClick={() => setAutoOpen(true)}>
              Auto-pair by run…
            </Button>
          ) : null}
          <Button onClick={() => setExportOpen(true)}>Export pairs…</Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon" aria-label="More actions" title="More actions">
                <Ellipsis aria-hidden="true" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem variant="destructive" onSelect={() => setDeleteOpen(true)}>
                <Trash2 aria-hidden="true" />
                Delete preference set…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </>
      }
    >
      <div className="mb-6 flex flex-wrap items-end justify-between gap-x-6 gap-y-4">
        <ToggleGroup
          type="single"
          variant="outline"
          spacing={0}
          value={filter}
          onValueChange={(v) => v && setFilter(v as ItemFilter)}
          aria-label="Show prompts"
          className="max-w-full"
        >
          {(["undecided", "decided", "all"] as const).map((f) => (
            <ToggleGroupItem
              key={f}
              value={f}
              aria-label={`${FILTER_LABEL[f]}, ${int(counts[f])}`}
              className="gap-1.5 px-3 data-[state=on]:font-semibold"
            >
              {FILTER_LABEL[f]}
              <span className="tabular font-normal text-muted-foreground">{int(counts[f])}</span>
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        {set.prompt_count > 0 ? (
          <div className="w-full sm:w-64">
            <p className="tabular text-sm">
              {int(set.decided)} of {int(set.prompt_count)} decided
            </p>
            <ProgressBar
              done={set.decided}
              total={set.prompt_count}
              label={`${int(set.decided)} of ${int(set.prompt_count)} prompts decided`}
              className="mt-1.5"
            />
          </div>
        ) : null}
      </div>

      <section ref={regionRef} aria-labelledby={item ? "review-question" : undefined} aria-label={item ? undefined : "Review"} className="scroll-mt-20 md:scroll-mt-6">
        {body}
      </section>

      {item ? (
        <div className="sticky bottom-0 z-20 -mx-4 mt-6 border-t bg-background px-4 py-3 sm:-mx-6 sm:px-6 lg:-mx-8 lg:px-8">
          <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
            <div id="review-status" className="min-w-0 flex-1 basis-64" aria-live="polite">
              <StatusLine item={item} sel={sel} status={status} onRetry={retry} />
            </div>
            <div className="flex flex-wrap items-center gap-2">
              {sel.chosen != null || sel.rejected != null ? (
                <Button variant="ghost" className="pointer-coarse:h-11" onClick={() => void clear()} aria-keyshortcuts="U">
                  Clear pair
                </Button>
              ) : null}
              {item.chosen_sample_id == null ? (
                <Button variant="outline" className="pointer-coarse:h-11" onClick={() => go(1)} disabled={nextDisabled} aria-keyshortcuts="J">
                  Skip
                </Button>
              ) : null}
              <nav aria-label="Prompts" className="flex items-center gap-1">
                <Button
                  variant="ghost"
                  size="icon-lg"
                  className="pointer-coarse:size-11"
                  aria-label="Previous prompt"
                  aria-keyshortcuts="K"
                  title="Previous prompt (K)"
                  onClick={() => go(-1)}
                  disabled={i === 0}
                >
                  <ChevronLeft aria-hidden="true" />
                </Button>
                <span className="tabular px-1 text-sm whitespace-nowrap text-muted-foreground">
                  {int(Math.min(i + 1, Math.max(total, 1)))} of {int(total)} {FILTER_NOUN[filter]}
                </span>
                <Button
                  variant="ghost"
                  size="icon-lg"
                  className="pointer-coarse:size-11"
                  aria-label="Next prompt"
                  aria-keyshortcuts="J"
                  title="Next prompt (J)"
                  onClick={() => go(1)}
                  disabled={nextDisabled}
                >
                  <ChevronRight aria-hidden="true" />
                </Button>
              </nav>
            </div>
          </div>
        </div>
      ) : null}

      <AutoPairDialog set={set} open={autoOpen} onOpenChange={setAutoOpen} />
      <ExportPairsDialog set={set} open={exportOpen} onOpenChange={setExportOpen} />
      <DeleteSetDialog set={set} open={deleteOpen} onOpenChange={setDeleteOpen} />
    </Page>
  )
}

// --------------------------------------------------------------------------- pieces

function SetMeta({ set }: { set: PreferenceSet }) {
  const runs = set.runs
  const fmt = new Intl.ListFormat(undefined, { style: "long", type: "conjunction" })
  const parts = fmt.formatToParts(runs.map((_, n) => String(n)))
  return (
    <p className="max-w-[90ch]">
      {runs.length ? (
        <>
          Compares{" "}
          {parts.map((part, n) =>
            part.type === "element" ? (
              <span key={n}>
                <Link to={`/runs/${runs[Number(part.value)].id}`} className="text-foreground underline-offset-2 hover:underline">
                  {runs[Number(part.value)].name}
                </Link>{" "}
                <span className="font-mono text-[13px]" title={runs[Number(part.value)].model}>
                  ({shortModel(runs[Number(part.value)].model)})
                </span>
              </span>
            ) : (
              <Fragment key={n}>{part.value}</Fragment>
            ),
          )}{" "}
          on{" "}
        </>
      ) : (
        <>The runs of this set were deleted. Prompt set: </>
      )}
      <Link to={`/prompts/${set.prompt_set_id}`} className="text-foreground underline-offset-2 hover:underline">
        {set.prompt_set_name}
      </Link>
      .
    </p>
  )
}

function StatusLine({
  item,
  sel,
  status,
  onRetry,
}: {
  item: PreferenceItem
  sel: Sel
  status: SaveState | null
  onRetry: () => void
}) {
  const name = (id: number | null) => {
    const n = item.candidates.findIndex((c) => c.sample_id === id)
    return n >= 0 ? letter(n) : "?"
  }
  if (status?.status === "error") {
    return (
      <div role="alert" className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-sm text-destructive">
        <span className="min-w-0">
          {status.op === "save" ? "Couldn't save this pair." : "Couldn't clear this pair."} {status.message} Your choice is
          kept here.
        </span>
        <Button variant="outline" size="sm" onClick={onRetry}>
          Retry
        </Button>
      </div>
    )
  }
  if (status?.status === "saving") return <p className="text-sm text-muted-foreground">Saving pair…</p>
  if (status?.status === "clearing") return <p className="text-sm text-muted-foreground">Clearing pair…</p>
  if (status?.status === "cleared") return <p className="text-sm text-muted-foreground">Pair cleared.</p>
  if (sel.chosen != null && sel.rejected != null) {
    const by = item.source === "rule" && sel.chosen === item.chosen_sample_id && sel.rejected === item.rejected_sample_id
    return (
      <p className="text-sm">
        {status?.status === "saved" ? "Saved: " : ""}
        <span className="font-medium">{name(sel.chosen)}</span> chosen over{" "}
        <span className="font-medium">{name(sel.rejected)}</span>
        {by ? <span className="text-muted-foreground">, paired by auto-pair</span> : null}.
      </p>
    )
  }
  if (sel.chosen != null)
    return (
      <p className="text-sm">
        <span className="font-medium">{name(sel.chosen)}</span> is chosen. Pick a rejected response.
      </p>
    )
  if (sel.rejected != null)
    return (
      <p className="text-sm">
        <span className="font-medium">{name(sel.rejected)}</span> is rejected. Pick a chosen response.
      </p>
    )
  return <p className="text-sm text-muted-foreground">Pick a chosen and a rejected response.</p>
}

function KeyboardHelp({ count }: { count: number }) {
  const last = String(Math.min(9, Math.max(count, 2)))
  return (
    <p className="mt-5 flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-muted-foreground pointer-coarse:hidden">
      <span className="font-medium text-foreground">Keyboard</span>
      <span className="inline-flex items-center gap-1">
        <Kbd>1</Kbd>–<Kbd>{last}</Kbd> chosen
      </span>
      <span className="inline-flex items-center gap-1">
        <Kbd>Shift</Kbd>
        <Kbd>1</Kbd>–<Kbd>{last}</Kbd> rejected
      </span>
      <span className="inline-flex items-center gap-1">
        <Kbd>J</Kbd>
        <Kbd>K</Kbd> or <Kbd>→</Kbd>
        <Kbd>←</Kbd> next and previous
      </span>
      <span className="inline-flex items-center gap-1">
        <Kbd>U</Kbd> clear pair
      </span>
    </p>
  )
}

function ReviewEmpty({
  set,
  filter,
  onExport,
  onFilter,
}: {
  set: PreferenceSet
  filter: ItemFilter
  onExport: () => void
  onFilter: (f: ItemFilter) => void
}) {
  if (set.prompt_count === 0 || filter === "all") {
    return (
      <EmptyBlock
        className="rounded-lg border px-4"
        action={
          <Button asChild variant="outline">
            <Link to="/runs">Open runs</Link>
          </Button>
        }
      >
        These runs have no prompt with two finished responses yet. Prompts appear here as the runs finish them.
      </EmptyBlock>
    )
  }
  if (filter === "decided") {
    return (
      <EmptyBlock
        className="rounded-lg border px-4"
        action={
          <Button variant="outline" onClick={() => onFilter("undecided")}>
            Show undecided prompts
          </Button>
        }
      >
        No prompt is decided yet. Pick a chosen and a rejected response on an undecided prompt, or auto-pair by run.
      </EmptyBlock>
    )
  }
  return (
    <EmptyBlock
      className="rounded-lg border px-4"
      action={
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={onExport}>
            Export pairs…
          </Button>
          <Button variant="ghost" onClick={() => onFilter("decided")}>
            Review decided prompts
          </Button>
        </div>
      }
    >
      <p className="text-base text-foreground">
        {set.prompt_count === 1 ? "The one prompt is decided." : `All ${int(set.prompt_count)} prompts are decided.`}
      </p>
    </EmptyBlock>
  )
}

function ReviewSkeleton({ withHeading = false }: { withHeading?: boolean }) {
  return (
    <div aria-busy="true" aria-label="Loading prompt…">
      {withHeading ? (
        <p className="mb-3 text-base font-semibold">Which response is better for this prompt?</p>
      ) : (
        <Bar className="mb-3 h-5 w-72 max-w-full" />
      )}
      <div className="grid gap-2 rounded-lg bg-muted px-4 py-3">
        <Bar className="h-3 w-10 bg-background" />
        <Bar className="h-4 w-full max-w-xl bg-background" />
        <Bar className="h-4 w-2/3 max-w-md bg-background" />
      </div>
      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        {[0, 1].map((n) => (
          <div key={n} className="rounded-lg border">
            <div className="flex items-center gap-3 border-b px-4 py-3">
              <Bar className="h-5 w-5" />
              <Bar className="h-4 w-40" />
              <Bar className="ml-auto h-9 w-48" />
            </div>
            <div className="grid gap-2 px-4 py-4">
              <Bar className="h-4 w-36" />
              <Bar className="h-4 w-full" />
              <Bar className="h-4 w-full" />
              <Bar className="h-4 w-3/4" />
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
