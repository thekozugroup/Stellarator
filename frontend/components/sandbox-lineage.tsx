"use client";

import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";

interface SandboxLineageProps {
  isSandbox?: boolean;
  parentRunId?: string | null;
  className?: string;
}

export function SandboxLineage({ isSandbox, parentRunId, className }: SandboxLineageProps) {
  if (!isSandbox && !parentRunId) return null;

  if (isSandbox) {
    return (
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <Badge
              variant="outline"
              className={`border-warning/50 bg-warning/10 text-warning text-[10px] uppercase tracking-wider font-semibold ${className ?? ""}`}
            >
              Sandbox
            </Badge>
          </TooltipTrigger>
          <TooltipContent side="bottom" className="max-w-xs text-xs">
            Sandbox run — used for pre-flight validation, not promoted to production.
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  // Promoted from sandbox
  const short = parentRunId ? parentRunId.slice(0, 7) : "";
  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Link
            href={`/runs/${parentRunId}`}
            className="inline-flex items-center gap-1.5 rounded-full border border-success/40 bg-success/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-success hover:bg-success/20 transition-colors focus-visible:ring-2 focus-visible:ring-ring outline-none"
          >
            Production
            <span className="text-success/70">·</span>
            <span className="font-mono normal-case tracking-normal">from sandbox {short}</span>
          </Link>
        </TooltipTrigger>
        <TooltipContent side="bottom" className="text-xs">
          Promoted from sandbox run {parentRunId}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
