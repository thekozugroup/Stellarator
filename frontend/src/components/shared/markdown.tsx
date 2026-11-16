import { memo } from "react"
import ReactMarkdown, { type Components } from "react-markdown"
import rehypeHighlight from "rehype-highlight"
import rehypeKatex from "rehype-katex"
import remarkGfm from "remark-gfm"
import remarkMath from "remark-math"
import "katex/dist/katex.min.css"
import { normalizeMath } from "@/lib/math"
import { cn } from "@/lib/utils"

const components: Components = {
  // Model output is untrusted: never fetch remote images from it. Show the alt text as a link.
  img: ({ src, alt }) =>
    typeof src === "string" && src ? (
      <a href={src} target="_blank" rel="noopener noreferrer nofollow">
        {alt || "Image"} (image link)
      </a>
    ) : (
      <span>{alt || "Image"}</span>
    ),
  a: ({ href, children }) => (
    <a href={href} target="_blank" rel="noopener noreferrer nofollow">
      {children}
    </a>
  ),
  // Wide tables scroll inside their own box, never the page.
  table: ({ children }) => (
    <div className="md-table">
      <table>{children}</table>
    </div>
  ),
}

/** Rendered Markdown for prompts, thinking traces and responses. Raw HTML is never rendered. */
function MarkdownImpl({ text, className }: { text: string; className?: string }) {
  return (
    <div className={cn("md", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, [remarkMath, { singleDollarTextMath: false }]]}
        rehypePlugins={[[rehypeKatex, { throwOnError: false, strict: "ignore" }], [rehypeHighlight, { detect: false, ignoreMissing: true }]]}
        components={components}
      >
        {normalizeMath(text)}
      </ReactMarkdown>
    </div>
  )
}

const Markdown = memo(MarkdownImpl)
export default Markdown
