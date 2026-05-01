"use client";

import { useEffect, useState } from "react";
import { ExternalLink, KeyRound, ShieldCheck } from "lucide-react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { readKey, writeKey } from "@/lib/chat/key-store";
import type { Driver } from "@/lib/chat/types";
import { DRIVER_LABELS } from "@/lib/chat/types";

// First-visit key / auth prompt. Branches based on active driver.
export function KeyModal({
  open,
  onOpenChange,
  onSaved,
  driver = "openai",
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  onSaved?: () => void;
  driver?: Driver;
}) {
  const [val, setVal] = useState("");
  useEffect(() => {
    if (open) setVal(readKey());
  }, [open]);

  function save() {
    writeKey(val.trim());
    onSaved?.();
    onOpenChange(false);
  }

  if (driver === "openrouter") {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <KeyRound className="size-4 text-primary" />
              Configure OpenRouter
            </DialogTitle>
          </DialogHeader>
          <div className="mt-2 space-y-3 text-sm">
            <p className="text-muted-foreground">
              OpenRouter keys are encrypted and stored server-side. Configure yours in Settings.
            </p>
            <div className="flex justify-end gap-2 pt-1">
              <Button variant="ghost" onClick={() => onOpenChange(false)}>
                Cancel
              </Button>
              <Link href="/settings" onClick={() => onOpenChange(false)}>
                <Button>
                  Open Settings <ExternalLink className="ml-1.5 size-3" />
                </Button>
              </Link>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    );
  }

  if (driver === "codex") {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <KeyRound className="size-4 text-primary" />
              Sign in with Codex
            </DialogTitle>
          </DialogHeader>
          <div className="mt-2 space-y-3 text-sm">
            <p className="text-muted-foreground">
              Codex uses browser OAuth. Sign in from Settings to connect your account.
            </p>
            <div className="flex justify-end gap-2 pt-1">
              <Button variant="ghost" onClick={() => onOpenChange(false)}>
                Cancel
              </Button>
              <Link href="/settings" onClick={() => onOpenChange(false)}>
                <Button>
                  Sign in from Settings <ExternalLink className="ml-1.5 size-3" />
                </Button>
              </Link>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    );
  }

  if (driver === "claude") {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <KeyRound className="size-4 text-primary" />
              Claude Code (MCP)
            </DialogTitle>
          </DialogHeader>
          <div className="mt-2 space-y-3 text-sm">
            <p className="text-muted-foreground">
              Claude Code is configured via MCP. No key is needed here — use the Bootstrap prompt
              in Settings to connect your agent.
            </p>
            <div className="flex justify-end gap-2 pt-1">
              <Button variant="ghost" onClick={() => onOpenChange(false)}>
                Dismiss
              </Button>
              <Link href="/settings" onClick={() => onOpenChange(false)}>
                <Button variant="outline">
                  View bootstrap prompt
                </Button>
              </Link>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    );
  }

  // Default: OpenAI API key (driver === "openai")
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <KeyRound className="size-4 text-primary" />
            Add an {DRIVER_LABELS[driver] ?? "OpenAI"} key
          </DialogTitle>
        </DialogHeader>
        <div className="mt-2 space-y-3 text-sm">
          <p className="text-muted-foreground">
            Required for GPT-4o and o1 via API key. Claude Code and Codex use server-side credentials.
          </p>
          <Input
            type="password"
            value={val}
            autoFocus
            onChange={(e) => setVal(e.target.value)}
            placeholder="sk-..."
            className="font-mono text-xs"
            aria-label="OpenAI API key"
          />
          <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-xs text-warning ring-1 ring-warning/30">
            <ShieldCheck className="mt-0.5 size-3.5 shrink-0" />
            <p>
              This key is held only in this browser tab (sessionStorage) and is cleared when you
              close the tab. It is never written to localStorage or sent to anything other than
              the Stellarator backend.
            </p>
          </div>
          <div className="flex justify-end gap-2 pt-1">
            <Button variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button onClick={save} disabled={!val.trim()}>
              Save key
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
