"use client";

import { useCallback, useEffect, useState } from "react";

const KEY = "stellarator.prefs.v1";

export type Density = "compact" | "comfy";
// Re-exported for settings page convenience
export type ModelId = string;
/** @deprecated use lastModelId instead */
export type Driver = string;

export interface LocalPrefs {
  pinnedRuns: string[];
  density: Density;
  sidebarCollapsed: boolean;
  pinnedOpen: boolean;
  /** @deprecated Kept for migration. Use lastModelId. */
  defaultChatModel: ModelId;
  /** @deprecated Kept for migration. Use lastModelId. */
  lastDriver: Driver;
  /** opencode-style provider/model-name last selected in the composer. */
  lastModelId: string;
}

const DEFAULTS: LocalPrefs = {
  pinnedRuns: [],
  density: "compact",
  sidebarCollapsed: false,
  pinnedOpen: true,
  defaultChatModel: "claude/sonnet-4-6",
  lastDriver: "claude",
  lastModelId: "claude/sonnet-4-6",
};

function read(): LocalPrefs {
  if (typeof window === "undefined") return DEFAULTS;
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<LocalPrefs>;
    // Migrate legacy prefs: if no lastModelId but old lastDriver+defaultChatModel present
    if (!parsed.lastModelId && parsed.defaultChatModel) {
      parsed.lastModelId = migrateModelId(parsed.defaultChatModel);
    }
    return { ...DEFAULTS, ...parsed };
  } catch {
    return DEFAULTS;
  }
}

function migrateModelId(old: string): string {
  const MAP: Record<string, string> = {
    "claude-sonnet-4-6": "claude/sonnet-4-6",
    "claude-opus-4-7":   "claude/opus-4-7",
    "claude-haiku-4-5":  "claude/haiku-4-5",
    "claude-code":       "claude/sonnet-4-6",
    "gpt-4o":            "openai/gpt-4o",
    "gpt-4o-mini":       "openai/gpt-4o-mini",
    "o1":                "openai/o1",
    "o1-mini":           "openai/o1-mini",
    "codex":             "codex/gpt-5",
  };
  return MAP[old] ?? old;
}

function write(p: LocalPrefs): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(KEY, JSON.stringify(p));
    window.dispatchEvent(new CustomEvent("stellarator:prefs"));
  } catch {
    /* quota or private mode — silently ignore */
  }
}

export function usePrefs(): {
  prefs: LocalPrefs;
  setPrefs: (patch: Partial<LocalPrefs>) => void;
  togglePin: (id: string) => void;
  isPinned: (id: string) => boolean;
} {
  const [prefs, setLocal] = useState<LocalPrefs>(DEFAULTS);

  useEffect(() => {
    setLocal(read());
    const onChange = (): void => setLocal(read());
    window.addEventListener("stellarator:prefs", onChange);
    window.addEventListener("storage", onChange);
    return () => {
      window.removeEventListener("stellarator:prefs", onChange);
      window.removeEventListener("storage", onChange);
    };
  }, []);

  const setPrefs = useCallback((patch: Partial<LocalPrefs>): void => {
    const next = { ...read(), ...patch };
    write(next);
    setLocal(next);
  }, []);

  const togglePin = useCallback((id: string): void => {
    const cur = read();
    const set = new Set(cur.pinnedRuns);
    if (set.has(id)) set.delete(id);
    else set.add(id);
    const next = { ...cur, pinnedRuns: [...set] };
    write(next);
    setLocal(next);
  }, []);

  const isPinned = useCallback(
    (id: string): boolean => prefs.pinnedRuns.includes(id),
    [prefs.pinnedRuns],
  );

  return { prefs, setPrefs, togglePin, isPinned };
}
