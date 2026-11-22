import { Pill } from "@/components/shared/status-pill"
import { count, duration, int } from "@/lib/format"
import type { LiveSequence } from "@/lib/types"
import { cn } from "@/lib/utils"
import { sampleLabel } from "./run-data"

/** Answering gets the ink and a gray dot; thinking and waiting stay gray. Copper is for the data. */
function Phase({ phase }: { phase: LiveSequence["phase"] }) {
  if (phase === "answering") return <Pill tone="positive">Answering</Pill>
  if (phase === "thinking") return <Pill tone="accent">Thinking</Pill>
  return <Pill>Waiting</Pill>
}

/**
 * The requests in flight, oldest first: which prompt, whether the model is thinking or answering,
 * and the last words it streamed. Values update in place; nothing moves.
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
  if (!sequences.length) return <p className="py-6 text-sm text-muted-foreground">Nothing in flight.</p>
  return (
    <>
      <ul className="divide-y">
        {sequences.map((s) => (
          <li key={s.sample_id} className="py-3 first:pt-0">
            <div className="flex items-center gap-2.5 text-sm">
              <span className="flex-none font-mono text-xs text-muted-foreground">
                {sampleLabel(s.prompt_idx, s.sample_idx, samplesPerPrompt)}
              </span>
              <span className="min-w-0 flex-1 truncate" title={s.prompt_preview}>
                {s.prompt_preview}
              </span>
              <Phase phase={s.phase} />
            </div>
            <div className="mt-1 flex flex-wrap gap-x-3 text-xs text-muted-foreground tabular">
              <span>{count(s.tokens, "token", "tokens")}</span>
              <span>{duration(s.elapsed_ms)}</span>
              {s.attempt > 1 ? <span className="text-coral-ink">attempt {int(s.attempt)}</span> : null}
            </div>
            {s.tail ? (
              <div
                className={cn(
                  "mt-2 rounded-md bg-muted px-2.5 py-1.5 text-[13px] leading-[1.6]",
                  s.phase === "thinking" ? "text-muted-foreground" : "text-foreground",
                )}
              >
                {/* At most the newest two lines: older text leaves through the top edge. */}
                <div className="flex max-h-[3.2em] flex-col justify-end overflow-hidden">
                  <p className="whitespace-pre-wrap [overflow-wrap:anywhere]">{s.tail.trimStart()}</p>
                </div>
              </div>
            ) : null}
          </li>
        ))}
      </ul>
      {inFlight > sequences.length ? (
        <p className="border-t pt-3 text-xs text-muted-foreground tabular">
          Showing the {int(sequences.length)} oldest of {int(inFlight)} in flight.
        </p>
      ) : null}
    </>
  )
}
