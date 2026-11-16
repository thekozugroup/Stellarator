import { Link } from "react-router"
import { Check } from "lucide-react"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Button } from "@/components/ui/button"
import { int } from "@/lib/format"
import { cn } from "@/lib/utils"
import { useEndpoints, usePromptSets } from "./run-data"

/**
 * First run: the runs table is empty, so its frame holds the three steps to a first run, each
 * marked done from real data. The next step to take is the one button; the step marks show progress.
 */
export function SetupChecklist() {
  const endpoints = useEndpoints()
  const sets = usePromptSets()

  if (endpoints.isError || sets.isError) {
    return (
      <ErrorBlock
        message="Couldn't check your endpoints and prompt sets. Is the Stellarator server running?"
        onRetry={() => {
          void endpoints.refetch()
          void sets.refetch()
        }}
      />
    )
  }

  const loading = endpoints.isPending || sets.isPending
  const steps = [
    {
      title: "Add an endpoint",
      body: "Any OpenAI-compatible server: vLLM, SGLang, llama.cpp, Ollama or a hosted API.",
      to: "/endpoints",
      action: "Add an endpoint",
      done: (endpoints.data?.length ?? 0) > 0,
      doneText: endpoints.data?.length === 1 ? "1 endpoint added" : `${int(endpoints.data?.length ?? 0)} endpoints added`,
    },
    {
      title: "Import prompts",
      body: "From Hugging Face, a file or pasted text.",
      to: "/prompts/import",
      action: "Import prompts",
      done: (sets.data?.length ?? 0) > 0,
      doneText: sets.data?.length === 1 ? "1 prompt set imported" : `${int(sets.data?.length ?? 0)} prompt sets imported`,
    },
    {
      title: "Start a run",
      body: "",
      to: "/runs/new",
      action: "New run",
      done: false,
      doneText: "",
    },
  ]
  const next = steps.findIndex((s) => !s.done)

  return (
    <div className="max-w-[40rem] py-2">
      <h2 className="text-base font-semibold">Set up your first run</h2>
      <ol className="mt-3 grid gap-0 divide-y">
        {steps.map((s, i) => (
          <li key={s.title} className="flex gap-3 py-4">
            <span
              aria-hidden="true"
              className={cn(
                "mt-0.5 flex size-6 flex-none items-center justify-center rounded-full border text-xs font-medium tabular",
                s.done ? "border-transparent bg-positive-tint text-positive-ink" : "text-muted-foreground",
              )}
            >
              {s.done ? <Check className="size-3.5" /> : i + 1}
            </span>
            <div className="min-w-0 flex-1">
              <h3 className="text-sm font-medium">
                {s.title}
                {s.done ? <span className="sr-only"> (done)</span> : null}
              </h3>
              {loading ? (
                <Bar className="mt-1 w-64 max-w-full" />
              ) : s.done || s.body ? (
                <p className="mt-0.5 text-sm text-muted-foreground">{s.done ? s.doneText : s.body}</p>
              ) : null}
              {!loading && !s.done ? (
                i === next ? (
                  <Button asChild variant="outline" size="sm" className="mt-3">
                    <Link to={s.to}>{s.action}</Link>
                  </Button>
                ) : (
                  <Link to={s.to} className="mt-2 inline-block text-sm underline underline-offset-4 hover:text-foreground">
                    {s.action}
                  </Link>
                )
              ) : null}
            </div>
          </li>
        ))}
      </ol>
    </div>
  )
}
