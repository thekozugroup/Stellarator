"use client";

import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { KeyRound, LogIn, Menu, Trash2 } from "lucide-react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { useChatRuntime } from "@/lib/chat/runtime";
import { driverFor, type Driver, DRIVER_LABELS } from "@/lib/chat/types";
import { clearKey, readKey } from "@/lib/chat/key-store";
import { usePrefs } from "@/lib/local-prefs";
import { db } from "@/lib/chat/db";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useConnectionStatus } from "@/components/connection-indicator";
import { driverFromModelId, parseModelId, legacyModelIdToNew } from "@/lib/chat/models";
import type { ModelId } from "@/lib/chat/types";
import { Composer, type ComposerHandle } from "./composer";
import { MessageView } from "./message";
import { Welcome } from "./welcome";
import { KeyModal } from "./key-modal";

export function ChatPanel({ threadId }: { threadId: string }) {
  const rt = useChatRuntime(threadId);
  const composerRef = useRef<ComposerHandle>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [keyModal, setKeyModal] = useState(false);
  const [keyPresent, setKeyPresent] = useState(false);

  // Editable title state
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const titleInputRef = useRef<HTMLInputElement>(null);

  const { prefs, setPrefs } = usePrefs();
  const connectionStatus = useConnectionStatus();
  const composerDisabled = connectionStatus !== "live";

  useEffect(() => {
    setKeyPresent(!!readKey());
  }, [keyModal]);

  // Auto-scroll to bottom on new content.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [rt.messages, rt.busy]);

  // Surface streaming errors as toasts (non-blocking).
  useEffect(() => {
    if (rt.error) toast.error("Chat error", { description: rt.error });
  }, [rt.error]);

  // Cite-intent dispatched from paper cards inside tool steps.
  useEffect(() => {
    function onCite(e: Event) {
      const detail = (e as CustomEvent<{ paperId: string }>).detail;
      composerRef.current?.insert(`/cite-paper ${detail.paperId}`);
    }
    window.addEventListener("stellarator:cite-intent", onCite);
    return () => window.removeEventListener("stellarator:cite-intent", onCite);
  }, []);

  // Focus title input when editing starts
  useEffect(() => {
    if (editingTitle) {
      titleInputRef.current?.focus();
      titleInputRef.current?.select();
    }
  }, [editingTitle]);

  // Resolve current modelId in opencode-style format.
  // Prefer lastModelId from prefs, fall back to thread model (migrate if needed).
  const rawModelId = rt.thread?.model ?? prefs.defaultChatModel ?? "claude/sonnet-4-6";
  const currentModelId = prefs.lastModelId ?? legacyModelIdToNew(rawModelId);
  const { provider: currentProvider, name: currentModelName } = parseModelId(currentModelId);

  // Derive legacy driver string for backend send call
  const driver = driverFromModelId(currentModelId) as Driver;
  const needsKey = driver === "openai" && !keyPresent;

  async function signInWithCodex() {
    try {
      const { url } = await api.codexOAuthStart();
      window.location.href = url;
    } catch (e) {
      toast.error("Codex sign-in failed", { description: (e as Error).message });
    }
  }

  function handleSubmit(text: string) {
    if (driver === "openai" && !readKey()) {
      setKeyModal(true);
      return;
    }
    void rt.send(text);
  }

  function handleModelIdChange(id: string) {
    setPrefs({ lastModelId: id, defaultChatModel: id });
    // Update thread's model+driver in the DB so rt.send uses the correct backend
    void rt.setModel(id as ModelId);
  }

  function startEditTitle() {
    if (!rt.thread) return;
    setTitleDraft(rt.thread.title);
    setEditingTitle(true);
  }

  async function commitTitle() {
    const trimmed = titleDraft.trim();
    if (trimmed && trimmed !== rt.thread?.title) {
      await db().threads.update(threadId, { title: trimmed, updatedAt: Date.now() });
    }
    setEditingTitle(false);
  }

  function handleTitleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") void commitTitle();
    if (e.key === "Escape") setEditingTitle(false);
  }

  if (!rt.thread) {
    return (
      <div className="grid h-full place-items-center text-sm text-muted-foreground">
        Loading thread…
      </div>
    );
  }

  // --------------------------------------------------------------------------
  // Key control — shows status or CTA based on the active provider
  // --------------------------------------------------------------------------
  function KeyControl() {
    if (driver === "claude") {
      return (
        <span className="text-[11px] text-muted-foreground/60">
          Configured via MCP
        </span>
      );
    }
    if (driver === "openai") {
      return keyPresent ? (
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1.5 text-xs text-muted-foreground hover:text-destructive"
          onClick={() => {
            clearKey();
            setKeyPresent(false);
            toast.success("Key removed from this tab");
          }}
          title="Key is held only in this browser tab"
        >
          <Trash2 className="size-3.5" /> Remove key
        </Button>
      ) : (
        <Button
          variant="outline"
          size="sm"
          className="h-7 gap-1.5 text-xs"
          onClick={() => setKeyModal(true)}
        >
          <KeyRound className="size-3.5" /> Add OpenAI key
        </Button>
      );
    }
    if (driver === "openrouter") {
      return (
        <Link href="/settings#openrouter">
          <Button variant="ghost" size="sm" className="h-7 gap-1.5 text-xs text-muted-foreground hover:text-foreground">
            <KeyRound className="size-3.5" /> Configure OpenRouter →
          </Button>
        </Link>
      );
    }
    if (driver === "codex") {
      const codexUser =
        typeof window !== "undefined"
          ? sessionStorage.getItem("stellarator.codex.user")
          : null;
      return codexUser ? (
        <span className="text-[11px] text-muted-foreground">
          Codex: <span className="font-mono">{codexUser}</span>
        </span>
      ) : (
        <Button
          variant="outline"
          size="sm"
          onClick={() => void signInWithCodex()}
          className="h-7 gap-1.5 text-xs"
        >
          <LogIn className="size-3.5" /> Sign in with Codex
        </Button>
      );
    }
    return null;
  }

  return (
    <div className="flex h-screen flex-col">
      {/* Sticky header */}
      <header className="sticky top-0 z-30 flex items-center justify-between gap-3 border-b border-border/50 bg-background/85 px-3 py-2.5 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        {/* Left: mobile menu + title */}
        <div className="flex min-w-0 items-center gap-2">
          <button
            type="button"
            data-open-thread-sheet
            aria-label="Open thread list"
            className="grid size-7 shrink-0 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground lg:hidden"
          >
            <Menu className="size-4" />
          </button>

          <div className="min-w-0">
            {editingTitle ? (
              <input
                ref={titleInputRef}
                value={titleDraft}
                onChange={(e) => setTitleDraft(e.target.value)}
                onBlur={() => void commitTitle()}
                onKeyDown={handleTitleKeyDown}
                className="h-6 w-full rounded border border-border bg-transparent px-1 text-sm font-semibold tracking-tight focus:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
            ) : (
              <button
                type="button"
                onClick={startEditTitle}
                className={cn(
                  "max-w-[200px] truncate rounded px-1 text-left text-sm font-semibold tracking-tight",
                  "hover:bg-accent/60 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  "sm:max-w-[320px]",
                )}
                title="Click to rename"
              >
                {rt.thread.title}
              </button>
            )}
            <p className="truncate px-1 text-[11px] text-muted-foreground">
              via <span className="font-mono">{currentModelName}</span>
              <span className="mx-1">·</span>
              <span className="uppercase tracking-wider">{DRIVER_LABELS[driver] ?? driver}</span>
            </p>
          </div>
        </div>

        {/* Right: key control */}
        <div className="flex shrink-0 items-center gap-1.5">
          <KeyControl />
        </div>
      </header>

      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-3xl px-4">
          {rt.messages.length === 0 ? (
            <Welcome
              onPick={(t) => {
                composerRef.current?.insert(t);
                composerRef.current?.focus();
              }}
            />
          ) : (
            <div className="divide-y divide-border/30 pb-6 pt-2">
              {rt.messages.map((m) => (
                <MessageView key={m.id} message={m} />
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="border-t border-border/50 bg-background/60 px-4 py-3">
        <div className="mx-auto w-full max-w-3xl">
          {needsKey && (
            <div className="mb-2 flex items-center justify-between gap-2 rounded-md border border-warning/30 bg-warning/5 px-3 py-1.5 text-xs text-warning">
              <span>OpenAI models need a key. It will be stored only in this browser tab.</span>
              <Button size="sm" variant="ghost" className="h-6 px-2 text-xs" onClick={() => setKeyModal(true)}>
                Add key
              </Button>
            </div>
          )}
          <Composer
            ref={composerRef}
            modelId={currentModelId}
            busy={rt.busy}
            disabled={composerDisabled}
            onCancel={rt.cancel}
            onModelIdChange={handleModelIdChange}
            onSubmit={handleSubmit}
          />
          <p className="mt-1.5 text-center text-[10px] text-muted-foreground">
            Cmd-Enter to send · Esc to clear · ⌘P to change model
          </p>
        </div>
      </div>

      <KeyModal
        open={keyModal}
        onOpenChange={setKeyModal}
        onSaved={() => setKeyPresent(true)}
        driver={driver}
      />
    </div>
  );
}
