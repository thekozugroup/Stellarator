// Math in model output, checked against the same Markdown pipeline the app renders with.
// Run: npm test (Node 22.6 or later; types are stripped, not checked).
import { test } from "node:test"
import assert from "node:assert/strict"
import { unified } from "unified"
import remarkParse from "remark-parse"
import remarkGfm from "remark-gfm"
import remarkMath from "remark-math"
import { normalizeMath } from "../src/lib/math.ts"

type Node = { type: string; value?: string; children?: Node[] }

/** The math nodes in reading order: display(…) or inline(…). */
function math(markdown: string): string[] {
  const tree = unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkMath, { singleDollarTextMath: false })
    .parse(normalizeMath(markdown)) as Node
  const found: string[] = []
  const walk = (n: Node) => {
    if (n.type === "math") found.push(`display(${n.value})`)
    if (n.type === "inlineMath") found.push(`inline(${n.value})`)
    n.children?.forEach(walk)
  }
  walk(tree)
  return found
}

test("an equation on its own line is display math", () => {
  assert.deepEqual(math("The rate is\n\n$$r = 6 - 2 = 4$$\n\nso it empties."), ["display(r = 6 - 2 = 4)"])
  assert.deepEqual(math("The rate is\n$$r = 4$$\nso it empties."), ["display(r = 4)"])
  assert.deepEqual(math("$$ t = 60 $$   "), ["display(t = 60)"])
  assert.deepEqual(math("- step one\n\n  $$z = 3$$\n"), ["display(z = 3)"])
})

test("$$ … $$ within a sentence stays inline", () => {
  assert.deepEqual(math("Then $$x^2$$ is positive."), ["inline(x^2)"])
  assert.deepEqual(math("$$a$$ and $$b$$"), ["inline(a)", "inline(b)"])
})

test("LaTeX delimiters", () => {
  assert.deepEqual(math("Let \\(x = 1\\) here."), ["inline(x = 1)"])
  assert.deepEqual(math("So\n\\[ y = 2 \\]\ndone"), ["display(y = 2)"])
  assert.deepEqual(math("$$\nw = 1\n$$"), ["display(w = 1)"])
})

test("prices and code are not math", () => {
  assert.deepEqual(math("It costs $12 an hour and $15 on Sundays."), [])
  assert.deepEqual(math("```\n$$not math$$\n```"), [])
  assert.deepEqual(math("Write `$$x$$` for math."), [])
})
