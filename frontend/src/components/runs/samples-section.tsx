import { useEffect, useMemo, useRef, useState } from "react"
import { Link, useLocation, useNavigate, useSearchParams } from "react-router"
import { Search, X } from "lucide-react"
import { Bar, EmptyBlock, ErrorBlock } from "@/components/shared/block-states"
import { SampleStatusPill } from "@/components/shared/status-pill"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { DASH, duration, int } from "@/lib/format"
import type { Run, SampleSummary } from "@/lib/types"
import { cn } from "@/lib/utils"
import { PAGE_SIZE, SAMPLE_FILTERS, isActive, sampleLabel, useSamplePage, type SampleFilter } from "./run-data"
import { SampleViewer } from "./sample-viewer"
import { StepPair } from "./step-pair"

const FILTER_LABEL: Record<SampleFilter, string> = {
  all: "All",
  done: "Done",
  failed: "Failed",
  truncated: "Truncated",
  pending: "Pending",
}

function filterCount(run: Run, f: SampleFilter): number {
  switch (f) {
    case "all":
      return run.total
    case "done":
      return run.done
    case "failed":
      return run.failed
    case "truncated":
      return run.truncated
    case "pending":
      return run.pending
  }
}

function emptyText(f: SampleFilter, q: string): string {
  if (q) return `No ${f === "all" ? "" : `${FILTER_LABEL[f].toLowerCase()} `}samples match “${q}”.`
  switch (f) {
    case "failed":
      return "No failed samples."
    case "truncated":
      return "No truncated samples."
    case "pending":
      return "Nothing waiting."
    case "done":
      return "No finished samples yet."
    default:
      return "No samples yet."
  }
}

/** Keep the other search params; set or clear one. */
function withParams(params: URLSearchParams, changes: Record<string, string | null>): string {
  const next = new URLSearchParams(params)
  for (const [k, v] of Object.entries(changes)) {
    if (v === null || v === "") next.delete(k)
    else next.set(k, v)
  }
  const s = next.toString()
  return s ? `?${s}` : "?"
}

/** The status word a row shows: Truncated for a done sample cut off at max tokens. */
function statusWord(s: SampleSummary): string {
  if (s.status === "done" && s.finish_reason === "length") return FILTER_LABEL.truncated
  return s.status === "running" ? "In flight" : FILTER_LABEL[s.status]
}

function SampleRows({
  items,
  run,
  params,
  selected,
  filter,
}: {
  items: SampleSummary[]
  run: Run
  params: URLSearchParams
  selected: number | null
  filter: SampleFilter
}) {
  const spp = run.params.samples_per_prompt
  // Under a one-status tab every row would repeat the tab's name, so the status goes.
  const showStatus = filter === "all" || items.some((s) => statusWord(s) !== FILTER_LABEL[filter])
  const open = (id: number) => ({ search: withParams(params, { sample: String(id) }) })
  // A thinking trace is the norm; the column appears only to mark finished samples without one.
  const noTrace = (s: SampleSummary) => s.status === "done" && !s.has_reasoning
  const showThinking = items.some(noTrace)
  return (
    <>
      <div className="hidden md:block">
        <Table className="table-fixed">
          <caption className="sr-only">Samples. Select a row to read the sample in full.</caption>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead scope="col" className="w-[5.5rem]">#</TableHead>
              <TableHead scope="col">Prompt</TableHead>
              <TableHead scope="col" className="hidden lg:table-cell">Response</TableHead>
              {showThinking ? <TableHead scope="col" className="hidden w-[5.5rem] xl:table-cell">Thinking</TableHead> : null}
              <TableHead scope="col" numeric className="w-[6rem]">Tokens</TableHead>
              <TableHead scope="col" numeric className="w-[6rem]">Latency</TableHead>
              {showStatus ? <TableHead scope="col" className="w-[7.5rem]">Status</TableHead> : null}
            </TableRow>
          </TableHeader>
          <TableBody>
            {items.map((s) => (
              <TableRow key={s.id} aria-selected={selected === s.id || undefined} className="relative">
                <TableCell className="font-mono text-xs text-muted-foreground">{sampleLabel(s.prompt_idx, s.sample_idx, spp)}</TableCell>
                <TableCell>
                  <Link
                    data-touch="inline"
                    to={open(s.id)}
                    state={{ viewer: true }}
                    data-sample-link={s.id}
                    title={s.prompt_preview}
                    className="block truncate outline-none after:absolute after:inset-0 after:content-[''] focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-ring"
                  >
                    {s.prompt_preview || DASH}
                  </Link>
                </TableCell>
                {/* A failed row's error is gray: the red mark in Status already says it failed. */}
                <TableCell className="hidden truncate text-muted-foreground lg:table-cell" title={s.error ?? undefined}>
                  {s.status === "failed" && s.error ? s.error : s.response_preview || DASH}
                </TableCell>
                {showThinking ? <TableCell className="hidden text-muted-foreground xl:table-cell">{noTrace(s) ? "None" : null}</TableCell> : null}
                <TableCell numeric className={cn(s.completion_tokens === null && "text-muted-foreground")}>
                  {int(s.completion_tokens)}
                </TableCell>
                <TableCell numeric className="text-muted-foreground">
                  {duration(s.latency_ms)}
                </TableCell>
                {showStatus ? (
                  <TableCell>
                    <SampleStatusPill status={s.status} finishReason={s.finish_reason} />
                  </TableCell>
                ) : null}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      <ul className="divide-y border-t md:hidden">
        {items.map((s) => (
          <li key={s.id} className={cn("relative py-3 has-[a:focus-visible]:bg-foreground/4", selected === s.id && "bg-accent")}>
            <div className="flex items-center gap-2 text-xs">
              <span className="font-mono text-muted-foreground">{sampleLabel(s.prompt_idx, s.sample_idx, spp)}</span>
              {showStatus ? <SampleStatusPill status={s.status} finishReason={s.finish_reason} /> : null}
              <span className="ml-auto text-muted-foreground tabular">
                {s.completion_tokens !== null ? `${int(s.completion_tokens)} tokens` : null}
              </span>
            </div>
            <Link
              data-touch="inline"
              to={open(s.id)}
              state={{ viewer: true }}
              data-sample-link={s.id}
              className="mt-1 block truncate text-sm outline-none after:absolute after:inset-0 after:content-[''] focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-ring"
            >
              {s.prompt_preview || DASH}
            </Link>
            <p className="truncate text-sm text-muted-foreground">{s.status === "failed" && s.error ? s.error : s.response_preview || DASH}</p>
          </li>
        ))}
      </ul>
    </>
  )
}

function RowsLoading() {
  return (
    <div aria-busy="true" aria-label="Loading samples">
      {Array.from({ length: 6 }, (_, i) => (
        <div key={i} className="flex h-11 items-center gap-4 border-b px-3 md:h-8">
          <Bar className="h-3 w-10" />
          <Bar className="w-1/3" />
          <Bar className="hidden w-1/4 lg:inline-block" />
          <span className="ml-auto flex gap-4">
            <Bar className="w-12" />
            <Bar className="h-4 w-14" />
          </span>
        </div>
      ))}
    </div>
  )
}

/**
 * The run's samples: a status filter with counts, a search box, 50 rows a page from the server,
 * and the sample viewer. Filter, search, page and the open sample all live in the URL.
 */
export function SamplesSection({ run }: { run: Run }) {
  const [params, setParams] = useSearchParams()
  const location = useLocation()
  const navigate = useNavigate()
  const rawStatus = params.get("status") as SampleFilter | null
  const status: SampleFilter = rawStatus && SAMPLE_FILTERS.includes(rawStatus) ? rawStatus : "all"
  const q = params.get("q") ?? ""
  const page = Math.max(1, Number.parseInt(params.get("page") ?? "1", 10) || 1)
  const sampleParam = params.get("sample")
  const sampleId = sampleParam && /^\d+$/.test(sampleParam) ? Number(sampleParam) : null

  const query = useMemo(() => ({ status, q, page }), [status, q, page])

  // A new page starts at the top of the table, not at the pagination row it was asked from.
  const sectionRef = useRef<HTMLElement>(null)
  const shownPage = useRef(page)
  useEffect(() => {
    if (shownPage.current === page) return
    shownPage.current = page
    const el = sectionRef.current
    if (el && el.getBoundingClientRect().top < 0) el.scrollIntoView({ block: "start" })
  }, [page])
  const samples = useSamplePage(run.id, query, isActive(run.status))

  // Search: typed text goes to the URL after 300 ms; the URL wins when it changes elsewhere (Back).
  const [text, setText] = useState(q)
  const lastSent = useRef(q)
  useEffect(() => {
    if (q !== lastSent.current) {
      lastSent.current = q
      setText(q)
    }
  }, [q])
  useEffect(() => {
    const t = text.trim()
    if (t === lastSent.current) return
    const id = window.setTimeout(() => {
      lastSent.current = t
      setParams((p) => new URLSearchParams(withParams(p, { q: t, page: null }).slice(1)), { replace: true })
    }, 300)
    return () => window.clearTimeout(id)
  }, [text, setParams])

  const data = samples.data
  const total = data?.total ?? 0
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE))
  const from = total ? (page - 1) * PAGE_SIZE + 1 : 0
  const to = Math.min(total, page * PAGE_SIZE)

  const items = data?.items ?? []
  const idx = sampleId !== null ? items.findIndex((s) => s.id === sampleId) : -1
  const prevId = idx > 0 ? items[idx - 1].id : null
  const nextId = idx >= 0 && idx < items.length - 1 ? items[idx + 1].id : null

  // The sheet opens from the URL, not from a trigger, so focus is returned by hand to the row of
  // the sample last shown (J and K may have moved it).
  const lastShown = useRef<number | null>(null)
  useEffect(() => {
    if (sampleId !== null) lastShown.current = sampleId
  }, [sampleId])
  const returnFocus = (e: Event) => {
    e.preventDefault()
    const links = document.querySelectorAll<HTMLElement>(`[data-sample-link="${lastShown.current}"]`)
    const visible = Array.from(links).find((el) => el.offsetParent !== null)
    ;(visible ?? document.getElementById("samples-heading"))?.focus()
  }

  const closeViewer = () => {
    if ((location.state as { viewer?: boolean } | null)?.viewer) navigate(-1)
    else setParams((p) => new URLSearchParams(withParams(p, { sample: null }).slice(1)), { replace: true })
  }
  const stepViewer = (id: number) =>
    navigate({ search: withParams(params, { sample: String(id) }) }, { replace: true, state: location.state })

  const filterHref = (f: SampleFilter) => ({ search: withParams(params, { status: f === "all" ? null : f, page: null, sample: null }) })

  return (
    <section
      ref={sectionRef}
      aria-labelledby="samples-heading"
      className="scroll-mt-(--top-chrome) [--sticky-below:2.75rem] pointer-coarse:[--sticky-below:3.75rem]"
    >
      <h2 id="samples-heading" tabIndex={-1} className="mb-1 text-base font-semibold">
        Samples
      </h2>

      {/* Search and the status filter stay under the toolbar while the rows scroll. */}
      <div className="sticky top-(--top-chrome) z-20 flex h-11 items-center gap-2 bg-background pointer-coarse:h-[3.75rem]">
        <nav aria-label="Filter samples by status" className="hidden xl:block">
          <ul className="inline-flex rounded-2xl bg-segment-track p-0.5">
            {SAMPLE_FILTERS.map((f) => {
              const active = f === status
              const n = filterCount(run, f)
              return (
                <li key={f}>
                  <Link
                    to={filterHref(f)}
                    aria-current={active ? "page" : undefined}
                    className={cn(
                      "flex h-7 items-center gap-1.5 rounded-full border border-transparent px-2.5 text-sm whitespace-nowrap transition-colors duration-150 pointer-fine:h-6",
                      active ? "border-control-edge bg-control font-medium text-foreground shadow-xs" : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {FILTER_LABEL[f]}
                    {!q ? (
                      <span className={cn("tabular", f === "failed" && n > 0 ? "text-destructive" : "text-muted-foreground")}>{int(n)}</span>
                    ) : null}
                  </Link>
                </li>
              )
            })}
          </ul>
        </nav>
        <div className="xl:hidden">
          <Select value={status} onValueChange={(v) => navigate(filterHref(v as SampleFilter), { replace: false })}>
            <SelectTrigger aria-label="Filter samples by status" className="min-w-28">
              <SelectValue>
                {FILTER_LABEL[status]}
                {!q ? <span className="text-muted-foreground tabular">{int(filterCount(run, status))}</span> : null}
              </SelectValue>
            </SelectTrigger>
            <SelectContent position="popper" align="start">
              {SAMPLE_FILTERS.map((f) => (
                <SelectItem key={f} value={f}>
                  {FILTER_LABEL[f]}
                  {!q ? (
                    <span className={cn("ml-2 tabular", f === "failed" && filterCount(run, f) > 0 ? "text-destructive" : "text-muted-foreground")}>
                      {int(filterCount(run, f))}
                    </span>
                  ) : null}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="relative min-w-0 flex-1 sm:ml-auto sm:max-w-72">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
          <Input
            type="search"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && text) {
                e.preventDefault()
                setText("")
              }
            }}
            placeholder="Search prompts and responses…"
            aria-label="Search samples"
            enterKeyHint="search"
            autoComplete="off"
            spellCheck={false}
            className="pl-8"
          />
        </div>
      </div>

      {samples.isError && !data ? (
        <div className="py-4">
          <ErrorBlock message={`Couldn't load samples. ${samples.error.message}`} onRetry={() => void samples.refetch()} />
        </div>
      ) : !data ? (
        <RowsLoading />
      ) : items.length === 0 ? (
        <div className="border-t">
          <EmptyBlock
            action={
              q ? (
                <Button type="button" variant="outline" size="sm" onClick={() => setText("")}>
                  <X aria-hidden="true" />
                  Clear search
                </Button>
              ) : page > 1 && total > 0 ? (
                <Button asChild variant="outline" size="sm">
                  <Link to={{ search: withParams(params, { page: null }) }}>Go to the first page</Link>
                </Button>
              ) : undefined
            }
          >
            {page > 1 && total > 0 ? `Page ${int(page)} is past the last page.` : emptyText(status, q)}
          </EmptyBlock>
        </div>
      ) : (
        <div className={cn("transition-opacity duration-150", samples.isPlaceholderData && "opacity-60")}>
          <SampleRows items={items} run={run} params={params} selected={sampleId} filter={status} />
        </div>
      )}

      {data && total > 0 ? (
        <div className="flex flex-wrap items-center justify-between gap-3 border-t py-3 text-sm">
          <p className="text-muted-foreground tabular" aria-live="polite">
            Showing {int(from)}–{int(to)} of {int(total)}
          </p>
          {pages > 1 ? (
            <StepPair
              prev={{
                label: "Previous page",
                to: { search: withParams(params, { page: page - 1 > 1 ? String(page - 1) : null, sample: null }) },
                disabled: page <= 1,
              }}
              next={{
                label: "Next page",
                to: { search: withParams(params, { page: String(page + 1), sample: null }) },
                disabled: page >= pages,
              }}
            />
          ) : null}
        </div>
      ) : null}

      <SampleViewer
        sampleId={sampleId}
        samplesPerPrompt={run.params.samples_per_prompt}
        prevId={prevId}
        nextId={nextId}
        onStep={stepViewer}
        onClose={closeViewer}
        onCloseAutoFocus={returnFocus}
      />
    </section>
  )
}
