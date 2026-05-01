"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import {
  ArrowRight,
  Check,
  Circle,
  Copy,
  KeyRound,
  LogIn,
  Rocket,
  Terminal,
  X,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useIntegrationKeys } from "@/lib/integrations";
import { cn } from "@/lib/utils";

const DISMISS_KEY = "stellarator.onboarding.dismissed";
const API_URL =
  (process.env.NEXT_PUBLIC_API_URL || "").replace(/\/$/, "");

interface Props {
  hasCompletedRuns: boolean;
}

export function OnboardingChecklist({ hasCompletedRuns }: Props) {
  const [dismissed, setDismissed] = useState(false);
  const [hasToken, setHasToken] = useState(false);
  const { data: keys } = useIntegrationKeys();

  useEffect(() => {
    if (typeof window === "undefined") return;
    setDismissed(window.localStorage.getItem(DISMISS_KEY) === "1");
    setHasToken(!!window.localStorage.getItem("stellarator.viewerToken"));
    const onStorage = () =>
      setHasToken(!!window.localStorage.getItem("stellarator.viewerToken"));
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  const hasTinker = !!keys?.find((k) => k.kind === "tinker");
  const hasAgent = !!keys?.find(
    (k) => k.kind === "openai-oauth" || k.kind === "openrouter",
  );

  const allDone = hasToken && hasTinker && hasAgent;

  // If all prerequisites unmet AND user hasn't completed runs, show.
  // If allDone or has completed runs, optionally show "ready" success card unless dismissed.
  const allMissing = !hasToken && !hasTinker && !hasAgent && !hasCompletedRuns;

  if (dismissed && !allMissing) return null;
  if (hasCompletedRuns && allDone) return null;

  function dismiss() {
    if (typeof window !== "undefined")
      window.localStorage.setItem(DISMISS_KEY, "1");
    setDismissed(true);
  }

  if (allDone) {
    return (
      <Card className="relative border-success/30 bg-success/5">
        <CardContent className="flex items-center gap-4 p-4">
          <div className="grid size-10 shrink-0 place-items-center rounded-full bg-success/15 text-success">
            <Rocket className="size-5" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-semibold">Ready to launch</p>
            <p className="text-xs text-muted-foreground">
              All prerequisites are connected. Open the chat planner to scaffold your first run.
            </p>
          </div>
          <Button asChild size="sm">
            <Link href="/chat">
              Open chat <ArrowRight className="size-3.5" />
            </Link>
          </Button>
          <button
            type="button"
            onClick={dismiss}
            aria-label="Dismiss"
            className="grid size-7 place-items-center rounded text-muted-foreground hover:text-foreground"
          >
            <X className="size-3.5" />
          </button>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="relative">
      <CardContent className="space-y-3 p-4">
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="text-sm font-semibold">Welcome to Stellarator</p>
            <p className="text-xs text-muted-foreground">
              Three quick steps to wire up your first training run.
            </p>
          </div>
          <button
            type="button"
            onClick={dismiss}
            aria-label="Dismiss onboarding"
            className="grid size-7 place-items-center rounded text-muted-foreground hover:text-foreground"
          >
            <X className="size-3.5" />
          </button>
        </div>

        <ol className="space-y-1.5">
          <Step
            done={hasToken}
            n={1}
            title="Set Stellarator access token"
            hint="Required to talk to the API as your agent."
            cta={
              <Button asChild size="sm" variant="outline" className="h-7 text-xs">
                <Link href="/settings#access-token">
                  <KeyRound className="size-3.5" />
                  Open settings
                </Link>
              </Button>
            }
          />
          <Step
            done={hasTinker}
            n={2}
            title="Configure Tinker API key"
            hint="Powers the training control plane."
            cta={
              <Button asChild size="sm" variant="outline" className="h-7 text-xs">
                <Link href="/settings#tinker">
                  <KeyRound className="size-3.5" />
                  Add key
                </Link>
              </Button>
            }
          />
          <Step
            done={hasAgent}
            n={3}
            title="Connect an agent"
            hint="Pick at least one driver: Claude Code, OpenAI, OpenRouter, or Codex."
            cta={<ConnectAgentMenu />}
          />
        </ol>
      </CardContent>
    </Card>
  );
}

function Step({
  done,
  n,
  title,
  hint,
  cta,
}: {
  done: boolean;
  n: number;
  title: string;
  hint: string;
  cta: React.ReactNode;
}) {
  return (
    <li
      className={cn(
        "flex items-center gap-3 rounded-md border px-3 py-2 transition-colors",
        done ? "border-success/30 bg-success/5" : "border-border/60",
      )}
    >
      <div
        className={cn(
          "grid size-6 shrink-0 place-items-center rounded-full text-[10px] font-mono",
          done
            ? "bg-success text-background"
            : "border border-border/60 text-muted-foreground",
        )}
      >
        {done ? <Check className="size-3.5" /> : <Circle className="size-3 opacity-0" />}
        {!done && <span className="absolute">{n}</span>}
      </div>
      <div className="min-w-0 flex-1">
        <p className={cn("text-xs font-medium", done && "text-muted-foreground line-through")}>
          {title}
        </p>
        <p className="text-[11px] text-muted-foreground">{hint}</p>
      </div>
      {!done && cta}
    </li>
  );
}

function ConnectAgentMenu() {
  function copyMcp() {
    const cmd = `claude mcp add stellarator -- npx -y stellarator-mcp --api-url ${API_URL}`;
    // navigator.clipboard is only available in secure contexts (HTTPS or localhost).
    // Over plain HTTP from LAN/Tailscale, clipboard is undefined → guard + fallback.
    if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
      void navigator.clipboard.writeText(cmd);
      toast.success("MCP command copied");
    } else {
      // Fallback: legacy execCommand on a temporary textarea.
      try {
        const ta = document.createElement("textarea");
        ta.value = cmd;
        ta.setAttribute("readonly", "");
        ta.style.position = "absolute";
        ta.style.left = "-9999px";
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        document.body.removeChild(ta);
        toast.success("MCP command copied");
      } catch {
        toast.message("Copy failed — copy manually:", { description: cmd });
      }
    }
  }
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm" variant="outline" className="h-7 text-xs">
          Connect <ArrowRight className="size-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuLabel>Pick a driver</DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuItem className="gap-2" onSelect={copyMcp}>
          <Copy className="size-3.5" /> Claude Code via MCP
        </DropdownMenuItem>
        <DropdownMenuItem
          className="gap-2"
          onSelect={() => {
            window.location.href = `${API_URL}/v1/oauth/codex/start`;
          }}
        >
          <Terminal className="size-3.5" /> Sign in with Codex
        </DropdownMenuItem>
        <DropdownMenuItem className="gap-2" asChild>
          <Link href="/settings#openrouter">
            <KeyRound className="size-3.5" /> Add OpenRouter key
          </Link>
        </DropdownMenuItem>
        <DropdownMenuItem className="gap-2" asChild>
          <Link href="/settings#openai">
            <LogIn className="size-3.5" /> Add OpenAI API key
          </Link>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
