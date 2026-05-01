"use client";

// Shared tool call card — kept here so existing callers (and any future ones)
// have a stable import path. Internally delegates to the chat tool registry,
// which provides per-tool rendering for known stellarator_* tools and falls
// back to a JSON disclosure for unknowns.

import { useMemo } from "react";
import { nanoid } from "nanoid";
import { ToolStepRenderer } from "@/components/chat/tool-ui-registry";
import type { ToolStep } from "@/lib/chat/types";

export interface ToolCallCardProps {
  name: string;
  args: unknown;
  result?: unknown;
  error?: string;
  startedAt?: number;
  endedAt?: number;
}

export function ToolCallCard({
  name,
  args,
  result,
  error,
  startedAt,
  endedAt,
}: ToolCallCardProps) {
  const step = useMemo<ToolStep>(() => {
    const start = startedAt ?? Date.now();
    return {
      id: nanoid(),
      name,
      args,
      result,
      error,
      startedAt: start,
      endedAt: endedAt ?? (result !== undefined || error ? start : undefined),
      status: error ? "error" : result !== undefined ? "done" : "running",
    };
  }, [name, args, result, error, startedAt, endedAt]);

  return <ToolStepRenderer step={step} />;
}
