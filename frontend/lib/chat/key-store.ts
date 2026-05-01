// Tab-scoped API key storage. Keys live in sessionStorage only — never localStorage.
"use client";

const KEY = "stellarator.chat.openaiKey";

export function readKey(): string {
  if (typeof window === "undefined") return "";
  try {
    return window.sessionStorage.getItem(KEY) ?? "";
  } catch {
    return "";
  }
}

export function writeKey(value: string): void {
  if (typeof window === "undefined") return;
  try {
    if (value) window.sessionStorage.setItem(KEY, value);
    else window.sessionStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
}

export function clearKey(): void {
  writeKey("");
}
