import { Badge } from "@/components/ui/badge";
import type { RunStatus } from "@/lib/types";

const MAP: Record<RunStatus, { label: string; variant: "success" | "warning" | "destructive" | "info" | "muted" | "secondary" }> = {
  queued: { label: "Queued", variant: "muted" },
  provisioning: { label: "Provisioning", variant: "info" },
  running: { label: "Running", variant: "success" },
  paused: { label: "Paused", variant: "warning" },
  succeeded: { label: "Succeeded", variant: "success" },
  failed: { label: "Failed", variant: "destructive" },
  cancelled: { label: "Cancelled", variant: "muted" },
};

export function RunStatusBadge({ status }: { status: RunStatus }) {
  const m = MAP[status];
  return (
    <Badge variant={m.variant} className="gap-1.5">
      <span
        className={`size-1.5 rounded-full ${
          status === "running"
            ? "bg-success animate-pulse"
            : status === "paused"
              ? "bg-warning"
              : status === "failed"
                ? "bg-destructive"
                : status === "succeeded"
                  ? "bg-success"
                  : "bg-muted-foreground"
        }`}
      />
      {m.label}
    </Badge>
  );
}
