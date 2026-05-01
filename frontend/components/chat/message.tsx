"use client";

import { ChevronRight, Sparkles } from "lucide-react";
import { cn } from "@/lib/utils";
import { Markdown } from "./markdown";
import { ToolStepRenderer } from "./tool-ui-registry";
import type { ChatMessage, Driver } from "@/lib/chat/types";

const AGENT_TONE: Record<Driver, string> = {
  claude:     "text-agent-claude bg-agent-claude/10 ring-agent-claude/30",
  openai:     "text-agent-openai bg-agent-openai/10 ring-agent-openai/30",
  openrouter: "text-agent-openrouter bg-agent-openrouter/10 ring-agent-openrouter/30",
  codex:      "text-agent-codex bg-agent-codex/10 ring-agent-codex/30",
};

// Document-style message: 32px avatar gutter on the left, body fills the rest.
// No bubbles. Streaming caret animates after the last char while streaming=true.
export function MessageView({ message }: { message: ChatMessage }) {
  const isUser = message.role === "user";
  const driver = message.driver ?? "openai";
  return (
    <article
      className={cn(
        "group relative flex gap-4 px-2 py-5",
        isUser ? "border-b border-border/30" : "",
      )}
      aria-label={`${message.role} message`}
    >
      <div className="shrink-0">
        <Avatar role={message.role} driver={driver} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="mb-1.5 flex items-center gap-2 text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
          <span>{isUser ? "You" : labelFor(driver)}</span>
          {!isUser && message.model && (
            <span className="font-mono text-[10px] text-muted-foreground/70">
              {message.model}
            </span>
          )}
        </div>

        {/* Tool steps render inline, in the order they arrived. */}
        {message.toolSteps && message.toolSteps.length > 0 && (
          <div className="mb-3 space-y-2">
            {message.toolSteps.map((s) => (
              <ToolStepRenderer key={s.id} step={s} />
            ))}
          </div>
        )}

        {message.content || message.streaming ? (
          <div className="relative">
            {isUser ? (
              <p className="whitespace-pre-wrap text-sm leading-7 text-foreground/95">
                {message.content}
              </p>
            ) : (
              <Markdown>{message.content || ""}</Markdown>
            )}
            {message.streaming && <StreamingCaret />}
          </div>
        ) : null}
      </div>
    </article>
  );
}

function labelFor(driver: Driver): string {
  if (driver === "claude") return "Claude";
  if (driver === "codex") return "Codex";
  return "Assistant";
}

function Avatar({ role, driver }: { role: ChatMessage["role"]; driver: Driver }) {
  if (role === "user") {
    return (
      <div className="grid size-8 place-items-center rounded-full bg-muted/60 text-foreground/80 ring-1 ring-border/60">
        <ChevronRight className="size-4" aria-hidden />
      </div>
    );
  }
  return (
    <div
      className={cn(
        "grid size-8 place-items-center rounded-full ring-1",
        AGENT_TONE[driver],
      )}
      aria-hidden
    >
      <Sparkles className="size-4" />
    </div>
  );
}

function StreamingCaret() {
  return (
    <span
      aria-hidden
      className="inline-block h-[1.05em] w-[2px] translate-y-[3px] animate-pulse bg-primary align-middle"
    />
  );
}
