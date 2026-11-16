import { useEffect, useMemo, useRef, useState } from "react"
import { Link, useLocation, useNavigate, useSearchParams } from "react-router"
import { ChevronLeft, ChevronRight, Search, X } from "lucide-react"
import { Bar, EmptyBlock, ErrorBlock } from "@/components/shared/block-states"
import { SampleStatusPill } from "@/components/shared/status-pill"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { DASH, duration, int } from "@/lib/format"
import type { Run, SampleSummary } from "@/lib/types"
import { cn } from "@/lib/utils"
import { PAGE_SIZE, SAMPLE_FILTERS, isActive, sampleLabel, useSamplePage, type SampleFilter } from "./run-data"
import { SampleViewer } from "./sample-viewer"

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
      <table className="hidden w-full table-fixed text-sm md:table">
        <caption className="sr-only">Samples. Select a row to read the sample in full.</caption>
        <thead>
          <tr className="text-left text-xs text-muted-foreground">
            <th scope="col" className="h-9 w-[5.5rem] px-4 font-normal">#</th>
            <th scope="col" className="h-9 px-3 font-normal">Prompt</th>
            <th scope="col" className="hidden h-9 px-3 font-normal lg:table-cell">Response</th>
            {showThinking ? <th scope="col" className="hidden h-9 w-[5.5rem] px-3 font-normal xl:table-cell">Thinking</th> : null}
            <th scope="col" className="h-9 w-[6rem] px-3 text-right font-normal">Tokens</th>
            <th scope="col" className={cn("h-9 w-[6rem] text-right font-normal", showStatus ? "px-3" : "pr-4 pl-3")}>Latency</th>
            {showStatus ? <th scope="col" className="h-9 w-[7.5rem] px-4 font-normal">Status</th> : null}
          </tr>
        </thead>
        <tbody>
          {items.map((s) => (
            <tr
              key={s.id}
              aria-selected={selected === s.id || undefined}
              className={cn(
                "relative border-t transition-colors duration-150 hover:bg-muted/60 has-[a:focus-visible]:bg-muted/60",
                selected === s.id && "bg-muted",
              )}
            >
              <td className="px-4 py-2.5 font-mono text-xs whitespace-nowrap text-muted-foreground">
                {sampleLabel(s.prompt_idx, s.sample_idx, spp)}
              </td>
              <td className="px-3 py-2.5">
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
              </td>
              {/* A failed row's error is gray: the red mark in Status already says it failed. */}
              <td className="hidden truncate px-3 py-2.5 text-muted-foreground lg:table-cell" title={s.error ?? undefined}>
                {s.status === "failed" && s.error ? s.error : s.response_preview || DASH}
              </td>
              {showThinking ? (
                <td className="hidden px-3 py-2.5 text-muted-foreground xl:table-cell">{noTrace(s) ? "None" : null}</td>
              ) : null}
              <td className={cn("px-3 py-2.5 text-right tabular", s.completion_tokens === null && "text-muted-foreground")}>
                {int(s.completion_tokens)}
              </td>
              <td className={cn("py-2.5 text-right text-muted-foreground tabular", showStatus ? "px-3" : "pr-4 pl-3")}>{duration(s.latency_ms)}</td>
              {showStatus ? (
                <td className="px-4 py-2.5">
                  <SampleStatusPill status={s.status} finishReason={s.finish_reason} />
                </td>
              ) : null}
            </tr>
          ))}
        </tbody>
      </table>

      <ul className="divide-y border-t md:hidden">
        {items.map((s) => (
          <li key={s.id} className={cn("relative px-4 py-3 has-[a:focus-visible]:bg-muted/60", selected === s.id && "bg-muted")}>
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
        <div key={i} className="flex h-[45px] items-center gap-4 border-t px-4">
          <Bar className="h-3 w-10" />
          <Bar className="w-1/3" />
          <Bar className="hidden w-1/4 lg:inline-block" />
          <span className="ml-auto flex gap-4">
            <Bar className="w-12" />
            <Bar className="h-5 w-14 rounded-full" />
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

  return (
    <section ref={sectionRef} aria-labelledby="samples-heading" className="scroll-mt-4 rounded-xl border">
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-3 px-4 pt-3 pb-3">
        <h2 id="samples-heading" tabIndex={-1} className="text-base font-semibold">
          Samples
        </h2>
        <div className="relative w-full sm:w-72">
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

      <nav aria-label="Filter samples by status" className="px-4 pb-3">
        <ul className="inline-flex flex-wrap gap-0.5 rounded-lg bg-muted p-[3px]">
          {SAMPLE_FILTERS.map((f) => {
            const active = f === status
            const n = filterCount(run, f)
            return (
              <li key={f}>
                <Link
                  to={{ search: withParams(params, { status: f === "all" ? null : f, page: null, sample: null }) }}
                  aria-current={active ? "page" : undefined}
                  className={cn(
                    "flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm whitespace-nowrap transition-colors duration-150",
                    active ? "bg-background font-medium text-foreground shadow-sm dark:bg-accent" : "text-muted-foreground hover:text-foreground",
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

      {samples.isError && !data ? (
        <div className="border-t p-4">
          <ErrorBlock message={`Couldn't load samples. ${samples.error.message}`} onRetry={() => void samples.refetch()} />
        </div>
      ) : !data ? (
        <RowsLoading />
      ) : items.length === 0 ? (
        <div className="border-t px-4">
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
        <div className="flex flex-wrap items-center justify-between gap-3 border-t px-4 py-3 text-sm">
          <p className="text-muted-foreground tabular" aria-live="polite">
            Showing {int(from)}–{int(to)} of {int(total)}
          </p>
          {pages > 1 ? (
            <div className="flex items-center gap-2">
              {page > 1 ? (
                <Button asChild variant="outline" size="sm">
                  <Link to={{ search: withParams(params, { page: page - 1 > 1 ? String(page - 1) : null, sample: null }) }}>
                    <ChevronLeft aria-hidden="true" />
                    Previous
                  </Link>
                </Button>
              ) : (
                <Button variant="outline" size="sm" disabled>
                  <ChevronLeft aria-hidden="true" />
                  Previous
                </Button>
              )}
              {page < pages ? (
                <Button asChild variant="outline" size="sm">
                  <Link to={{ search: withParams(params, { page: String(page + 1), sample: null }) }}>
                    Next
                    <ChevronRight aria-hidden="true" />
                  </Link>
                </Button>
              ) : (
                <Button variant="outline" size="sm" disabled>
                  Next
                  <ChevronRight aria-hidden="true" />
                </Button>
              )}
            </div>
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
