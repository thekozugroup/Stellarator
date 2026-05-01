"use client";

import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import "highlight.js/styles/github-dark.css";
import { cn } from "@/lib/utils";

// Document-style markdown renderer (no bubble container).
// Tightly tuned typography to match manifest.build / assistant-ui defaults.

const components: Components = {
  h1: ({ className, ...p }) => (
    <h1 className={cn("mt-6 mb-3 text-xl font-semibold tracking-tight", className)} {...p} />
  ),
  h2: ({ className, ...p }) => (
    <h2 className={cn("mt-5 mb-2 text-lg font-semibold tracking-tight", className)} {...p} />
  ),
  h3: ({ className, ...p }) => (
    <h3 className={cn("mt-4 mb-2 text-base font-semibold", className)} {...p} />
  ),
  p: ({ className, ...p }) => (
    <p className={cn("my-2 leading-7 text-foreground/90", className)} {...p} />
  ),
  a: ({ className, ...p }) => (
    <a
      className={cn("text-primary underline decoration-primary/40 underline-offset-2 hover:decoration-primary", className)}
      target="_blank"
      rel="noreferrer"
      {...p}
    />
  ),
  ul: ({ className, ...p }) => (
    <ul className={cn("my-2 ml-5 list-disc space-y-1 marker:text-muted-foreground", className)} {...p} />
  ),
  ol: ({ className, ...p }) => (
    <ol className={cn("my-2 ml-5 list-decimal space-y-1 marker:text-muted-foreground", className)} {...p} />
  ),
  li: ({ className, ...p }) => <li className={cn("leading-7", className)} {...p} />,
  blockquote: ({ className, ...p }) => (
    <blockquote
      className={cn("my-3 border-l-2 border-primary/40 pl-4 italic text-muted-foreground", className)}
      {...p}
    />
  ),
  hr: ({ className, ...p }) => (
    <hr className={cn("my-6 border-border/60", className)} {...p} />
  ),
  table: ({ className, ...p }) => (
    <div className="my-3 overflow-x-auto rounded-md border border-border/60">
      <table className={cn("w-full text-sm", className)} {...p} />
    </div>
  ),
  th: ({ className, ...p }) => (
    <th className={cn("border-b border-border/60 bg-muted/40 px-3 py-2 text-left font-medium", className)} {...p} />
  ),
  td: ({ className, ...p }) => (
    <td className={cn("border-b border-border/40 px-3 py-2 align-top", className)} {...p} />
  ),
  code: ({ className, children, ...p }) => {
    const isBlock = (className ?? "").includes("language-");
    if (isBlock) {
      return (
        <code className={cn("text-[0.85em]", className)} {...p}>
          {children}
        </code>
      );
    }
    return (
      <code
        className={cn(
          "rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[0.85em] text-foreground",
          className,
        )}
        {...p}
      >
        {children}
      </code>
    );
  },
  pre: ({ className, ...p }) => (
    <pre
      className={cn(
        "my-3 overflow-x-auto rounded-lg border border-border/60 bg-card/60 p-3 text-[0.85rem] leading-6",
        className,
      )}
      {...p}
    />
  ),
};

export function Markdown({ children }: { children: string }) {
  return (
    <div className="prose prose-invert max-w-none text-sm">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[[rehypeHighlight, { detect: true, ignoreMissing: true }]]}
        components={components}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}
