/**
 * Models write math as \( … \), \[ … \], $$ … $$ and sometimes $ … $. remark-math only knows
 * dollars, and single dollars are switched off because prompts are full of prices ("$12 an hour").
 * So: \( x \) becomes inline $$x$$, \[ x \] becomes a display block, and a line that holds only
 * $$ x $$ becomes a display block too (remark-math reads $$ … $$ within one line as inline math,
 * but a model that puts an equation on its own line means it as display math). Code spans and
 * fenced code are left alone.
 */
export function normalizeMath(text: string): string {
  if (!text.includes("\\(") && !text.includes("\\[") && !text.includes("$$")) return text
  // Split into code and non-code segments: fenced blocks first, then inline code spans.
  const parts = text.split(/(```[\s\S]*?(?:```|$)|~~~[\s\S]*?(?:~~~|$)|`[^`\n]*`)/g)
  return parts
    .map((part, i) => {
      if (i % 2 === 1) return part // code
      return part
        .replace(/\\\[([\s\S]+?)\\\]/g, (_m, body: string) => `\n$$\n${body.trim()}\n$$\n`)
        .replace(DISPLAY_LINE, (_m, indent: string, body: string) => `${indent}$$\n${indent}${body.trim()}\n${indent}$$`)
        .replace(/\\\(([\s\S]+?)\\\)/g, (_m, body: string) => `$$${body.trim()}$$`)
    })
    .join("")
}

/** A whole line of `$$ … $$` with no other `$$` inside, optionally indented (inside a list item). */
const DISPLAY_LINE = /^([ \t]*)\$\$((?:(?!\$\$)[^\n])*[^\s$](?:(?!\$\$)[^\n])*)\$\$[ \t]*$/gm
