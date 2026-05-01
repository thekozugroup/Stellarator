import { Badge } from "@/components/ui/badge";
import type { RunNote } from "@/lib/types";

const KIND_VARIANT: Record<RunNote["kind"], "info" | "muted" | "warning" | "success" | "default" | "secondary"> = {
  agent: "info",
  system: "muted",
  user: "default",
  tool: "secondary",
  metric: "success",
  decision: "warning",
};

export function NotesTimeline({ notes }: { notes: RunNote[] }) {
  if (!notes.length) {
    return (
      <div className="rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
        No notes yet. The owning agent will append decisions and observations here as the run progresses.
      </div>
    );
  }
  return (
    <ol className="relative space-y-4 border-l border-border/60 pl-6">
      {notes.map((n) => (
        <li key={n.id} className="relative">
          <span className="absolute -left-[26px] top-1.5 size-2.5 rounded-full bg-primary ring-4 ring-background" />
          <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <Badge variant={KIND_VARIANT[n.kind]} className="uppercase tracking-wider">
              {n.kind}
            </Badge>
            <span className="font-mono">{n.author}</span>
            <span>·</span>
            <time>{new Date(n.created_at).toLocaleTimeString()}</time>
          </div>
          <p className="mt-1.5 whitespace-pre-wrap text-sm leading-relaxed text-foreground">
            {n.body}
          </p>
        </li>
      ))}
    </ol>
  );
}
