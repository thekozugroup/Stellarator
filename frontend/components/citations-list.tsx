import { ExternalLink, FileText } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import type { Citation } from "@/lib/types";

export function CitationsList({ citations }: { citations: Citation[] }) {
  if (!citations.length) {
    return (
      <div className="rounded-lg border border-dashed p-6 text-center text-xs text-muted-foreground">
        No citations linked. Ask the agent to /research to attach prior work.
      </div>
    );
  }
  return (
    <ul className="space-y-2">
      {citations.map((c) => (
        <li key={c.id}>
          <a
            href={c.url ?? "#"}
            target="_blank"
            rel="noreferrer"
            className="group flex items-start gap-3 rounded-lg border bg-card/50 p-3 transition-colors hover:bg-accent/40"
          >
            <FileText className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <p className="truncate text-sm font-medium leading-tight">{c.title}</p>
                {c.url && (
                  <ExternalLink className="size-3 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
                )}
              </div>
              <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
                {c.source && (
                  <Badge variant="outline" className="uppercase tracking-wider text-[10px]">
                    {c.source}
                  </Badge>
                )}
                {c.authors && c.authors.length > 0 && (
                  <span className="truncate">{c.authors.slice(0, 3).join(", ")}</span>
                )}
              </div>
            </div>
          </a>
        </li>
      ))}
    </ul>
  );
}
