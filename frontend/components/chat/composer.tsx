"use client";

import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { ArrowUp, AtSign, ChevronDown, Circle, Settings, Slash, Square } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import {
  type Driver,
  type ModelId,
  DRIVER_LABELS,
  driverFor,
} from "@/lib/chat/types";
import {
  type ModelOption,
  type UnavailableModelOption,
  STATIC_MODELS,
  resolveAvailableModels,
  parseModelId,
  defaultModelFor,
  driverFromModelId,
} from "@/lib/chat/models";
import { useIntegrationKeys, useOpenRouterModels } from "@/lib/integrations";
import { usePrefs } from "@/lib/local-prefs";
import { estimateTokens } from "@/lib/chat/tokens";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";

const SLASH_COMMANDS = [
  { id: "new-run", label: "/new-run", hint: "Scaffold a new training run", insert: "/new-run " },
  { id: "list-runs", label: "/list-runs", hint: "Show recent runs", insert: "/list-runs " },
  { id: "cite-paper", label: "/cite-paper", hint: "Attach a paper to a run", insert: "/cite-paper " },
  { id: "summarize-thread", label: "/summarize-thread", hint: "TL;DR of this thread", insert: "/summarize-thread" },
];

// Provider dot colors (inline style — no Tailwind arbitrary values needed)
const PROVIDER_COLORS: Record<string, string> = {
  claude:     "oklch(0.78 0.16 75)",  // warm amber (matches sidebar-primary)
  codex:      "oklch(0.74 0.14 155)", // green
  openai:     "oklch(0.74 0.12 240)", // blue
  openrouter: "oklch(0.74 0.14 310)", // purple
};

export interface ComposerHandle {
  insert: (text: string) => void;
  focus: () => void;
}

export interface ComposerProps {
  /** opencode-style model id, e.g. "claude/sonnet-4-6" */
  modelId: string;
  onModelIdChange: (id: string) => void;
  onSubmit: (text: string) => void;
  onCancel: () => void;
  busy: boolean;
  /** When true, disable the textarea and Send button (e.g. backend disconnected). */
  disabled?: boolean;
}

export const Composer = forwardRef<ComposerHandle, ComposerProps>(function Composer(
  { modelId, onModelIdChange, onSubmit, onCancel, busy, disabled = false },
  ref,
) {
  const [value, setValue] = useState("");
  const [picker, setPicker] = useState<null | "slash" | "mention" | "model">(null);
  const [pickerQuery, setPickerQuery] = useState("");
  const taRef = useRef<HTMLTextAreaElement>(null);
  const { setPrefs } = usePrefs();

  // Integration data for availability
  const { data: keys } = useIntegrationKeys();
  const { models: openRouterModelItems } = useOpenRouterModels();

  const hasOpenRouterKey = !!keys?.find((k) => k.kind === "openrouter");
  const hasOpenAIKey =
    typeof window !== "undefined"
      ? !!sessionStorage.getItem("stellarator.chat.openaiKey")
      : false;
  const codexUser =
    typeof window !== "undefined"
      ? sessionStorage.getItem("stellarator.codex.user")
      : null;

  // Build OpenRouter model options from dynamic list
  const openRouterModels: ModelOption[] = useMemo(
    () =>
      openRouterModelItems.map((m) => ({
        id: `openrouter/${m.id}`,
        provider: "openrouter" as const,
        label: m.label,
        subtitle: m.subtitle,
        requires: "openrouter-key" as const,
      })),
    [openRouterModelItems],
  );

  const { available, unavailable } = useMemo(
    () =>
      resolveAvailableModels(
        { hasOpenRouterKey, hasOpenAIKey, codexUser },
        openRouterModels,
      ),
    [hasOpenRouterKey, hasOpenAIKey, codexUser, openRouterModels],
  );

  // Derive display info from the current modelId
  const { provider: currentProvider, name: currentName } = parseModelId(modelId);
  const providerColor = PROVIDER_COLORS[currentProvider] ?? "oklch(0.6 0 0)";

  useImperativeHandle(ref, () => ({
    insert: (text: string) => {
      setValue((v) => (v ? `${v.replace(/\s*$/, "")} ${text}` : text));
      requestAnimationFrame(() => taRef.current?.focus());
    },
    focus: () => taRef.current?.focus(),
  }));

  // Auto-grow up to 12 lines
  useEffect(() => {
    const el = taRef.current;
    if (!el) return;
    el.style.height = "auto";
    const rawLh = parseFloat(getComputedStyle(el).lineHeight);
    const lineHeight = Number.isNaN(rawLh) ? 24 : rawLh;
    const max = 12 * lineHeight;
    el.style.height = `${Math.min(el.scrollHeight, max)}px`;
  }, [value]);

  // Detect slash / @run picker triggers
  useEffect(() => {
    const el = taRef.current;
    if (!el) return;
    const caret = el.selectionStart ?? value.length;
    const upto = value.slice(0, caret);
    const tokenMatch = upto.match(/(?:^|\s)([/@][\w-]*)$/);
    if (!tokenMatch) {
      if (picker === "slash" || picker === "mention") {
        setPicker(null);
        setPickerQuery("");
      }
      return;
    }
    const token = tokenMatch[1];
    if (token.startsWith("/")) {
      setPicker("slash");
      setPickerQuery(token.slice(1));
    } else {
      setPicker("mention");
      setPickerQuery(token.slice(1));
    }
  }, [value, picker]);

  // ⌘P shortcut to open model picker
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "p") {
        e.preventDefault();
        setPicker((p) => (p === "model" ? null : "model"));
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  function send() {
    const text = value.trim();
    if (!text || busy) return;
    onSubmit(text);
    setValue("");
  }

  function replaceTrigger(insert: string) {
    const el = taRef.current;
    if (!el) return;
    const caret = el.selectionStart ?? value.length;
    const upto = value.slice(0, caret);
    const after = value.slice(caret);
    const next = upto.replace(/(?:^|\s)([/@][\w-]*)$/, (m) => {
      const lead = m.startsWith(" ") ? " " : "";
      return `${lead}${insert}`;
    });
    setValue(`${next}${after}`);
    setPicker(null);
    requestAnimationFrame(() => taRef.current?.focus());
  }

  function handleModelSelect(id: string) {
    onModelIdChange(id);
    setPrefs({ lastModelId: id });
    setPicker(null);
    requestAnimationFrame(() => taRef.current?.focus());
  }

  const tokens = useMemo(() => estimateTokens(value), [value]);

  // Group available models by provider
  const byProvider = useMemo(() => {
    const groups: Record<string, ModelOption[]> = {};
    for (const m of available) {
      if (!groups[m.provider]) groups[m.provider] = [];
      groups[m.provider].push(m);
    }
    return groups;
  }, [available]);

  const providerOrder: string[] = ["claude", "codex", "openai", "openrouter"];

  // Determine if current model is unavailable (to show Configure CTA)
  const currentUnavailable = unavailable.find((m) => m.id === modelId) as UnavailableModelOption | undefined;

  return (
    <div className="rounded-xl border border-border/70 bg-card/70 shadow-sm focus-within:border-ring/40 focus-within:ring-2 focus-within:ring-ring/40">
      <label htmlFor="chat-composer" className="sr-only">
        Message
      </label>
      <Popover open={picker === "slash" || picker === "mention"}>
        <PopoverAnchor asChild>
          <textarea
            id="chat-composer"
            ref={taRef}
            value={value}
            disabled={disabled}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (disabled) return;
              if (e.key === "Escape") {
                if (picker) {
                  setPicker(null);
                  e.preventDefault();
                  return;
                }
                if (value) {
                  setValue("");
                  e.preventDefault();
                }
                return;
              }
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                send();
                return;
              }
              if (e.key === "Enter" && !e.shiftKey && !picker) {
                e.preventDefault();
                send();
              }
            }}
            rows={1}
            placeholder={
              disabled
                ? "Backend disconnected — reconnecting…"
                : "Ask the planner — describe a goal, paste a paper, or type / for commands"
            }
            className="block w-full resize-none bg-transparent px-4 py-3 text-sm leading-6 outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-60"
            aria-label="Compose a message"
          />
        </PopoverAnchor>

        {picker === "slash" && (
          <PopoverContent
            align="start"
            sideOffset={4}
            onOpenAutoFocus={(e: Event) => e.preventDefault()}
            className="w-72 p-0"
          >
            <Command>
              <CommandList>
                <CommandEmpty>No matching command.</CommandEmpty>
                <CommandGroup heading="Slash commands">
                  {SLASH_COMMANDS.filter((c) =>
                    c.label.slice(1).toLowerCase().startsWith(pickerQuery.toLowerCase()),
                  ).map((c) => (
                    <CommandItem
                      key={c.id}
                      onSelect={() => replaceTrigger(c.insert)}
                      className="flex flex-col items-start gap-0"
                    >
                      <span className="font-mono text-xs">{c.label}</span>
                      <span className="text-[11px] text-muted-foreground">{c.hint}</span>
                    </CommandItem>
                  ))}
                </CommandGroup>
              </CommandList>
            </Command>
          </PopoverContent>
        )}

        {picker === "mention" && (
          <PopoverContent
            align="start"
            sideOffset={4}
            onOpenAutoFocus={(e: Event) => e.preventDefault()}
            className="w-80 p-0"
          >
            <RunMentionList
              query={pickerQuery}
              onSelect={(id) => replaceTrigger(`@${id} `)}
            />
          </PopoverContent>
        )}
      </Popover>

      <div className="flex flex-wrap items-center gap-1.5 border-t border-border/50 px-2 py-1.5">
        {/* Unified provider/model Combobox */}
        <Popover open={picker === "model"} onOpenChange={(v) => setPicker(v ? "model" : null)}>
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => setPicker((p) => (p === "model" ? null : "model"))}
                  className="flex h-7 items-center gap-1.5 rounded-md border-0 bg-transparent px-2 text-xs text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  aria-label="Select provider and model (⌘P)"
                  aria-haspopup="listbox"
                  aria-expanded={picker === "model"}
                >
                  {/* Provider color dot */}
                  <span
                    className="size-2 shrink-0 rounded-full"
                    style={{ backgroundColor: providerColor }}
                    aria-hidden
                  />
                  <span className="font-medium text-foreground">{currentProvider}</span>
                  <span className="text-muted-foreground/60">·</span>
                  <span>{currentName}</span>
                  <ChevronDown className="size-3 opacity-50" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="top">Change model (⌘P)</TooltipContent>
            </Tooltip>
          </TooltipProvider>

          <PopoverContent
            align="start"
            sideOffset={4}
            className="w-80 p-0"
            onOpenAutoFocus={(e: Event) => e.preventDefault()}
          >
            <Command>
              <CommandInput placeholder="Search models…" className="h-8 text-xs" />
              <CommandList className="max-h-80">
                <CommandEmpty>No models found.</CommandEmpty>

                {providerOrder
                  .filter((p) => byProvider[p]?.length)
                  .map((p, idx) => (
                    <span key={p}>
                      {idx > 0 && <CommandSeparator />}
                      <CommandGroup
                        heading={
                          <span className="flex items-center gap-1.5">
                            <span
                              className="size-1.5 rounded-full"
                              style={{ backgroundColor: PROVIDER_COLORS[p] }}
                              aria-hidden
                            />
                            {PROVIDER_LABELS[p] ?? p}
                          </span>
                        }
                      >
                        {byProvider[p].map((m) => (
                          <CommandItem
                            key={m.id}
                            value={`${p} ${m.label} ${m.id}`}
                            onSelect={() => handleModelSelect(m.id)}
                            className="flex items-center justify-between gap-2 text-xs"
                          >
                            <span className="font-medium">{m.label}</span>
                            {m.subtitle && (
                              <span className="text-[10px] text-muted-foreground">{m.subtitle}</span>
                            )}
                            {m.id === modelId && (
                              <Circle className="ml-auto size-1.5 fill-primary text-primary" />
                            )}
                          </CommandItem>
                        ))}
                      </CommandGroup>
                    </span>
                  ))}

                {unavailable.length > 0 && (
                  <>
                    <CommandSeparator />
                    <CommandGroup heading="Not configured">
                      {unavailable.slice(0, 6).map((m) => (
                        <CommandItem
                          key={m.id}
                          value={`unavailable ${m.provider} ${m.label}`}
                          disabled
                          className="flex items-center justify-between gap-2 text-xs opacity-50"
                        >
                          <span>{m.label}</span>
                          <Link
                            href={`/settings#${m.provider}`}
                            className="ml-auto text-[10px] text-primary underline-offset-2 hover:underline"
                            onClick={() => setPicker(null)}
                          >
                            {m.hint}
                          </Link>
                        </CommandItem>
                      ))}
                    </CommandGroup>
                  </>
                )}
              </CommandList>
            </Command>
          </PopoverContent>
        </Popover>

        {/* Configure CTA if current selection is unsatisfied */}
        {currentUnavailable && (
          <Link
            href={`/settings#${currentUnavailable.provider}`}
            className="inline-flex items-center gap-1 rounded-md bg-warning/10 px-2 py-1 text-[10px] text-warning ring-1 ring-warning/30 transition-colors hover:bg-warning/20"
          >
            <Settings className="size-3" />
            Configure {currentUnavailable.provider} →
          </Link>
        )}

        <TooltipProvider delayDuration={300}>
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type="button"
                onClick={() => {
                  setValue((v) => v + (v.endsWith(" ") || v === "" ? "/" : " /"));
                  requestAnimationFrame(() => taRef.current?.focus());
                }}
                className="grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground"
                aria-label="Insert slash command"
              >
                <Slash className="size-3.5" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="top">Slash commands</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type="button"
                onClick={() => {
                  setValue((v) => v + (v.endsWith(" ") || v === "" ? "@" : " @"));
                  requestAnimationFrame(() => taRef.current?.focus());
                }}
                className="grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground"
                aria-label="Mention a run"
              >
                <AtSign className="size-3.5" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="top">Mention a run</TooltipContent>
          </Tooltip>
        </TooltipProvider>

        <span className="ml-auto font-mono text-[11px] tabular-nums text-muted-foreground">
          ~{tokens} tok
        </span>

        {busy ? (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={onCancel}
            className="h-7 gap-1.5 px-2 text-xs"
            aria-label="Stop generating"
          >
            <Square className="size-3" /> Stop
          </Button>
        ) : (
          <Button
            type="button"
            size="sm"
            onClick={send}
            disabled={!value.trim() || disabled}
            className="h-7 gap-1.5 px-2.5 text-xs"
            aria-label="Send message (Cmd+Enter)"
          >
            <ArrowUp className="size-3.5" />
            Send
          </Button>
        )}
      </div>
    </div>
  );
});

// Provider display labels for the picker heading
const PROVIDER_LABELS: Record<string, string> = {
  claude:     "Claude Code · MCP",
  codex:      "Codex · Sign-in",
  openai:     "OpenAI · API key",
  openrouter: "OpenRouter",
};

// ---------------------------------------------------------------------------
// RunMentionList
// ---------------------------------------------------------------------------
function RunMentionList({
  query,
  onSelect,
}: {
  query: string;
  onSelect: (id: string, name: string) => void;
}) {
  const { data, isLoading } = useQuery({
    queryKey: ["chat.mention.runs"],
    queryFn: () => api.listRuns({ limit: 12 }),
    staleTime: 30_000,
  });
  const rows = (data?.runs ?? []).filter((r) =>
    `${r.id} ${r.name}`.toLowerCase().includes(query.toLowerCase()),
  );
  return (
    <Command shouldFilter={false}>
      <CommandList>
        {isLoading && (
          <div className="px-3 py-2 text-xs text-muted-foreground">Loading runs…</div>
        )}
        {!isLoading && rows.length === 0 && <CommandEmpty>No matching runs.</CommandEmpty>}
        <CommandGroup heading="Active runs">
          {rows.map((r) => (
            <CommandItem
              key={r.id}
              onSelect={() => onSelect(r.id, r.name)}
              className="flex items-center justify-between gap-3"
            >
              <div className="min-w-0">
                <div className="truncate text-xs font-medium">{r.name}</div>
                <div className="font-mono text-[10px] text-muted-foreground">{r.id}</div>
              </div>
              <Badge variant="outline" className="font-normal text-[10px] uppercase tracking-wider">
                {r.status}
              </Badge>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </Command>
  );
}

// Re-export helpers callers may need
export { driverFor, driverFromModelId, parseModelId, defaultModelFor };
