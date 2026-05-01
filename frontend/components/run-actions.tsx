"use client";

import { useEffect, useState } from "react";
import { Pause, Play, Rocket, Square } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { PromoteDialog } from "@/components/promote-dialog";
import { api } from "@/lib/api";
import { useIsOwner } from "@/lib/use-whoami";
import type { Run } from "@/lib/types";

export function RunActions({
  run,
  onChange,
}: {
  run: Run;
  onChange?: () => void;
}) {
  const isOwner = useIsOwner(run);
  const [promoteOpen, setPromoteOpen] = useState(false);

  // External "promote-intent" custom event (e.g. from sandbox_ready toast)
  useEffect(() => {
    if (typeof window === "undefined") return;
    function onIntent(e: Event) {
      const detail = (e as CustomEvent<{ run_id?: string }>).detail;
      if (detail?.run_id === run.id) setPromoteOpen(true);
    }
    window.addEventListener("stellarator:promote-intent", onIntent);
    return () =>
      window.removeEventListener("stellarator:promote-intent", onIntent);
  }, [run.id]);

  async function call(fn: () => Promise<unknown>, label: string) {
    try {
      await fn();
      toast.success(`${label} requested`);
      onChange?.();
    } catch (e) {
      toast.error(`${label} failed`, { description: (e as Error).message });
    }
  }

  const ownerAgent = run.owner_agent;
  const canPromote = run.is_sandbox && run.status === "succeeded";

  return (
    <div className="flex items-center gap-2">
      {canPromote && (
        <PromoteButton
          isOwner={isOwner}
          ownerAgent={ownerAgent}
          onClick={() => setPromoteOpen(true)}
        />
      )}
      {run.status === "running" && (
        <ActionButton
          isOwner={isOwner}
          ownerAgent={ownerAgent}
          action="pause"
          onClick={() => call(() => api.pauseRun(run.id), "Pause")}
          icon={<Pause className="size-3.5" />}
          label="Pause"
        />
      )}
      {run.status === "paused" && (
        <ActionButton
          isOwner={isOwner}
          ownerAgent={ownerAgent}
          action="resume"
          onClick={() => call(() => api.resumeRun(run.id), "Resume")}
          icon={<Play className="size-3.5" />}
          label="Resume"
        />
      )}
      {(run.status === "running" ||
        run.status === "paused" ||
        run.status === "queued") && (
        <ActionButton
          isOwner={isOwner}
          ownerAgent={ownerAgent}
          action="cancel"
          variant="destructive"
          onClick={() => call(() => api.cancelRun(run.id), "Cancel")}
          icon={<Square className="size-3.5" />}
          label="Cancel"
        />
      )}
      {canPromote && (
        <PromoteDialog open={promoteOpen} onOpenChange={setPromoteOpen} sandbox={run} />
      )}
    </div>
  );
}

function PromoteButton({
  isOwner,
  ownerAgent,
  onClick,
}: {
  isOwner: boolean;
  ownerAgent: string;
  onClick: () => void;
}) {
  if (isOwner) {
    return (
      <Button size="sm" onClick={onClick}>
        <Rocket className="size-3.5" />
        Promote
      </Button>
    );
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          size="sm"
          aria-disabled="true"
          onClick={(e) => e.preventDefault()}
          className="pointer-events-auto cursor-not-allowed opacity-50"
        >
          <Rocket className="size-3.5" />
          Promote
        </Button>
      </TooltipTrigger>
      <TooltipContent>Owned by {ownerAgent}</TooltipContent>
    </Tooltip>
  );
}

type Action = "pause" | "resume" | "cancel";

function ActionButton({
  isOwner,
  ownerAgent,
  action,
  onClick,
  icon,
  label,
  variant = "outline",
}: {
  isOwner: boolean;
  ownerAgent: string;
  action: Action;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  variant?: "outline" | "destructive";
}) {
  const tooltipText = `Owned by ${ownerAgent}. Sign in as that agent to ${action}.`;

  if (isOwner) {
    return (
      <Button size="sm" variant={variant} onClick={onClick}>
        {icon}
        {label}
      </Button>
    );
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          size="sm"
          variant={variant}
          aria-disabled="true"
          onClick={(e) => e.preventDefault()}
          className="pointer-events-auto opacity-50 cursor-not-allowed"
        >
          {icon}
          {label}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{tooltipText}</TooltipContent>
    </Tooltip>
  );
}
