/**
 * Models write math as \( … \), \[ … \], $$ … $$ and sometimes $ … $. remark-math only knows
 * dollars, and single dollars are switched off because prompts are full of prices ("$12 an hour").
 * So: \( x \) becomes inline $$x$$, \[ x \] becomes a display block. Code spans and fenced code
 * are left alone.
 */
export function normalizeMath(text: string): string {
  if (!text.includes("\\(") && !text.includes("\\[")) return text
  // Split into code and non-code segments: fenced blocks first, then inline code spans.
  const parts = text.split(/(```[\s\S]*?(?:```|$)|~~~[\s\S]*?(?:~~~|$)|`[^`\n]*`)/g)
  return parts
    .map((part, i) => {
      if (i % 2 === 1) return part // code
      return part
        .replace(/\\\[([\s\S]+?)\\\]/g, (_m, body: string) => `\n$$\n${body.trim()}\n$$\n`)
        .replace(/\\\(([\s\S]+?)\\\)/g, (_m, body: string) => `$$${body.trim()}$$`)
    })
    .join("")
}
