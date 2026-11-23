// Captures the raw app screens that compose.mjs frames into store images.
//
// It drives the app's browser build (`nsh package --target web`, served by stellarator-server) in
// headless Chromium (Playwright) at 2x. The address carries `?frame=mac`, `windows` or `linux`
// (app/src/web.rs), so the app lays the screen out as in that platform's window: on macOS the
// band for the traffic lights and 26 pt corners, on Windows 8 px corners, on Linux none. Which
// screens, themes and platforms it captures comes from images.json, so a new image there is
// captured on the next run.
//
//   node store/scripts/capture.mjs [--only <shot>[,<shot>]] [--out <dir>]
//
// Needs the server running with the sample data and the web build (store/README.md) at
// STELLARATOR_UI (default http://127.0.0.1:8096), and the mock model server behind the "DGX Spark
// vLLM" endpoint for the live run. The live run takes about four minutes: it resumes the long run,
// lets it run at 12 seqs, raises seqs to 32 so the chart shows the change, captures, and pauses the
// run again.

import { createRequire } from "node:module"
import { execSync } from "node:child_process"
import { mkdirSync, readFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const here = dirname(fileURLToPath(import.meta.url))
const BASE = (process.env.STELLARATOR_UI ?? "http://127.0.0.1:8096").replace(/\/$/, "")
const args = process.argv.slice(2)
const opt = (name) => {
  const i = args.indexOf(name)
  return i >= 0 ? args[i + 1] : undefined
}
const OUT = resolve(opt("--out") ?? join(here, "..", ".work", "raw"))
const ONLY = opt("--only")?.split(",")

// Seconds of the live run before and after seqs change. Shorter values make a flatter chart.
const LIVE_BEFORE = Number(process.env.LIVE_BEFORE ?? 150)
const LIVE_AFTER = Number(process.env.LIVE_AFTER ?? 75)
const LIVE_SEQS = [12, 32]

// The Hugging Face dataset shown on the import screen. Its chosen/rejected columns show that only
// the prompts are kept. Without internet access the paste source is shown instead.
const HF_DATASET = process.env.HF_DATASET ?? "HuggingFaceH4/ultrafeedback_binarized"
const PASTE_TEXT = [
  "Explain the difference between a mutex and a semaphore in two sentences.",
  "---",
  "A train leaves at 9:40 and arrives at 13:05. How long is the trip?",
  "---",
  "Prove that the square root of 2 is irrational.",
  "---",
  "Write a Python function that returns the n-th Fibonacci number in O(log n) time.",
].join("\n")

function loadPlaywright() {
  const require = createRequire(import.meta.url)
  try {
    return require("playwright")
  } catch {
    const root = execSync("npm root -g").toString().trim()
    return require(join(root, "playwright"))
  }
}

/** The window of each platform: its content size, and the app's `?frame=` for it. */
const SHELLS = {
  mac: { viewport: { width: 1280, height: 800 }, frame: "mac" },
  windows: { viewport: { width: 1280, height: 720 }, frame: "windows" },
  linux: { viewport: { width: 1280, height: 720 }, frame: "linux" },
}

async function api(method, path, body) {
  const res = await fetch(BASE + path, {
    method,
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  })
  if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${await res.text()}`)
  return res.status === 204 ? null : res.json()
}

const settle = (page, ms = 800) => page.waitForTimeout(ms)

/** Opens `path` laid out as `shell`'s window and waits for it to draw. */
async function open(page, path, shell, ms = 2000) {
  await page.goto(`${BASE}${path}?frame=${SHELLS[shell].frame}`, { waitUntil: "networkidle" })
  await settle(page, ms)
}

/** Picks `option` in the pop-up `combobox`. */
async function pick(page, combobox, option) {
  await combobox.click()
  await settle(page, 600)
  await page.getByRole("option", { name: option }).first().click()
  await settle(page, 600)
}

/** Each screen: how to get there and into the state the image shows. */
const SHOTS = {
  "run-live": async (page, shell, ctx) => {
    await open(page, `/runs/${ctx.liveRun.id}`, shell, 7000)
  },
  "run-new": async (page, shell) => {
    await open(page, "/runs/new", shell)
    await pick(page, page.getByRole("combobox", { name: "Endpoint" }), /DGX Spark/)
    await settle(page, 1200)
    await pick(page, page.getByRole("combobox").nth(1), "mock-reasoning-content")
    await pick(page, page.getByRole("combobox", { name: "Prompt set" }), /gsm8k/)
    await page.getByRole("radiogroup", { name: "Common seqs values" }).getByRole("radio", { name: "32", exact: true }).click()
    // Temperature: the field that holds the default 0.7.
    const fields = page.getByRole("textbox")
    for (let i = 0; i < (await fields.count()); i++) {
      if ((await fields.nth(i).inputValue().catch(() => "")) === "0.7") {
        await fields.nth(i).fill("0.6")
        break
      }
    }
    await page.mouse.click(1000, 120)
    await settle(page)
  },
  import: async (page, shell) => {
    await open(page, "/prompts/import", shell, 1000)
    const field = page.getByRole("textbox", { name: "openai/gsm8k" })
    await field.fill(HF_DATASET)
    await field.press("Enter")
    try {
      await page.getByRole("button", { name: /^Import [\d,]+ prompts/ }).waitFor({ timeout: 45000 })
      await settle(page, 2500)
      return
    } catch {
      console.warn(`  ${HF_DATASET} did not load (offline?); showing the paste source instead`)
    }
    await page.getByRole("radio", { name: "Paste" }).click()
    await settle(page, 500)
    await page.getByRole("textbox").last().fill(PASTE_TEXT)
    await settle(page, 2500)
  },
  endpoints: async (page, shell) => {
    await open(page, "/endpoints", shell, 1000)
    await page.getByRole("button", { name: "Add endpoint" }).first().click()
    await settle(page, 600)
    await page.getByRole("button", { name: "vLLM", exact: true }).click()
    await settle(page, 300)
    const name = page.getByRole("textbox", { name: "DGX Spark" })
    await name.fill("GPU server vLLM")
    await page.getByRole("textbox", { name: "http://192.168.1.50:8000/v1" }).fill("http://192.168.1.50:8000/v1")
    await name.focus()
    await settle(page, 600)
  },
  preferences: async (page, shell) => {
    await open(page, "/preferences/1", shell, 2500)
  },
  export: async (page, shell) => {
    await open(page, "/runs/1", shell)
    await page.getByRole("button", { name: "Export dataset…" }).click()
    await settle(page, 1500)
  },
  sample: async (page, shell) => {
    await open(page, "/runs/1", shell)
    // The second sample of the first prompt (#1.2).
    await page.getByRole("link", { name: /capital of Australia/ }).nth(1).click()
    await settle(page, 1500)
  },
}

/** The long run: resume it, change seqs part way, and return a function that pauses it again. */
async function startLiveRun() {
  const runs = await api("GET", "/api/runs")
  const run = runs.sort((a, b) => b.total - a.total || b.done - a.done)[0]
  const wasRunning = run.status === "running"
  console.log(`live run: #${run.id} "${run.name}" (${run.status}, ${run.done} of ${run.total})`)
  await api("PATCH", `/api/runs/${run.id}`, { concurrency: LIVE_SEQS[0] })
  if (!wasRunning) await api("POST", `/api/runs/${run.id}/resume`, {})
  console.log(`  ${LIVE_BEFORE} s at ${LIVE_SEQS[0]} seqs`)
  await new Promise((r) => setTimeout(r, LIVE_BEFORE * 1000))
  await api("PATCH", `/api/runs/${run.id}`, { concurrency: LIVE_SEQS[1] })
  console.log(`  ${LIVE_AFTER} s at ${LIVE_SEQS[1]} seqs`)
  await new Promise((r) => setTimeout(r, LIVE_AFTER * 1000))
  return { run, stop: async () => (wasRunning ? null : api("POST", `/api/runs/${run.id}/pause`, {})) }
}

async function main() {
  const spec = JSON.parse(readFileSync(join(here, "images.json"), "utf8"))
  const wanted = new Map()
  for (const img of spec.images) {
    const shell = img.frame === "windows" ? "windows" : img.frame === "linux" ? "linux" : "mac"
    const key = `${img.shot}.${img.theme}.${shell}`
    if (!SHOTS[img.shot]) throw new Error(`images.json: unknown shot "${img.shot}" in ${img.file}`)
    if (!ONLY || ONLY.includes(img.shot)) wanted.set(key, { shot: img.shot, theme: img.theme, shell })
  }
  mkdirSync(OUT, { recursive: true })
  const { chromium } = loadPlaywright()
  const browser = await chromium.launch()
  const ctxFor = ({ theme, shell }) =>
    browser.newContext({
      viewport: SHELLS[shell].viewport,
      deviceScaleFactor: 2,
      colorScheme: theme,
      reducedMotion: "reduce",
    })
  const capture = async (key, job, ctx) => {
    const c = await ctxFor(job)
    const page = await c.newPage()
    await SHOTS[job.shot](page, job.shell, ctx)
    await page.screenshot({ path: join(OUT, `${key}.png`) })
    await c.close()
    console.log(`  wrote ${key}.png`)
  }

  const live = [...wanted].filter(([, j]) => j.shot === "run-live")
  const rest = [...wanted].filter(([, j]) => j.shot !== "run-live")
  for (const [key, job] of rest) await capture(key, job, {})
  if (live.length) {
    const { run, stop } = await startLiveRun()
    try {
      // All variants of the live run at the same moment, so they show the same chart.
      await Promise.all(live.map(([key, job]) => capture(key, job, { liveRun: run })))
    } finally {
      await stop()
    }
  }
  await browser.close()
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
