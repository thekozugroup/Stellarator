import { count, duration, int } from "@/lib/format"
import type { LiveSequence } from "@/lib/types"
import { cn } from "@/lib/utils"
import { sampleLabel } from "./run-data"

/** The state word of a log row: thinking is the data color, answering green, waiting muted. */
const PHASE_CLASS: Record<LiveSequence["phase"], string> = {
  thinking: "text-periwinkle-ink",
  answering: "text-positive-ink",
  waiting: "text-muted-foreground",
}

/** Elapsed time as a clock, "0:42" or "1:02:05", for the time column of a log row. */
function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const ss = String(s % 60).padStart(2, "0")
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`
}

/**
 * The requests in flight, oldest first, as log rows (DESIGN.md, Components): the sample number and
 * the time it has been going in mono, the state word in its color, then the prompt. Under it, in
 * muted text, the newest words the model streamed (at most two lines) and a retry's attempt.
 * Values update in place; nothing moves. Rows share one grid so the columns line up; from a 24rem
 * panel the token count sits at the end of each row.
 */
export function InFlightList({
  sequences,
  inFlight,
  samplesPerPrompt,
}: {
  sequences: LiveSequence[]
  inFlight: number
  samplesPerPrompt: number
}) {
  if (!sequences.length) return <p className="py-2 text-sm text-muted-foreground">Nothing in flight.</p>
  return (
    <div className="@container">
      <ul className="grid grid-cols-[auto_auto_auto_minmax(0,1fr)] gap-x-3 @sm:grid-cols-[auto_auto_auto_minmax(0,1fr)_auto]">
        {sequences.map((s) => (
          <li key={s.sample_id} className="col-span-full grid grid-cols-subgrid items-baseline py-1.5">
            <span className="font-mono text-xs text-muted-foreground">{sampleLabel(s.prompt_idx, s.sample_idx, samplesPerPrompt)}</span>
            <span className="text-right font-mono text-xs text-muted-foreground" title={duration(s.elapsed_ms)}>
              {clock(s.elapsed_ms)}
            </span>
            <span className={cn("font-mono text-xs", PHASE_CLASS[s.phase])}>{s.phase}</span>
            <span className="min-w-0 truncate text-sm" title={s.prompt_preview}>
              {s.prompt_preview}
            </span>
            <span className="hidden text-right font-mono text-xs text-muted-foreground @sm:inline">
              <span className="sr-only">, </span>
              {count(s.tokens, "token", "tokens")}
            </span>
            <div
              className={cn(
                "col-span-full mt-0.5 text-[13px] leading-[1.6] text-muted-foreground @sm:col-start-4",
                !s.tail && s.attempt <= 1 && "@sm:hidden",
              )}
            >
              {/* In a narrow panel the token count moves here, from the end of the row. */}
              <p className="flex gap-x-3 font-mono text-xs empty:hidden">
                <span className="@sm:hidden">{count(s.tokens, "token", "tokens")}</span>
                {s.attempt > 1 ? <span className="text-coral-ink">attempt {int(s.attempt)}</span> : null}
              </p>
              {s.tail ? (
                // At most the newest two lines: older text leaves through the top edge.
                <div className={cn("flex max-h-[3.2em] flex-col justify-end overflow-hidden", s.phase === "answering" && "text-foreground")}>
                  <p className="whitespace-pre-wrap [overflow-wrap:anywhere]">{s.tail.trimStart()}</p>
                </div>
              ) : null}
            </div>
          </li>
        ))}
      </ul>
      {inFlight > sequences.length ? (
        <p className="mt-2 text-xs text-muted-foreground tabular">
          Showing the {int(sequences.length)} oldest of {int(inFlight)} in flight.
        </p>
      ) : null}
    </div>
  )
}
