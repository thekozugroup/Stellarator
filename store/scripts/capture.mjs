// Captures the raw app screens that compose.mjs frames into store images.
//
// It drives the web UI in headless Chromium (Playwright) at 2x, with the desktop app's window
// emulated: on macOS the overlay title bar and 26 pt corners, on Windows 8 px corners, on Linux
// none. Which screens, themes and platforms it captures comes from images.json, so a new image
// there is captured on the next run.
//
//   node store/scripts/capture.mjs [--only <shot>[,<shot>]] [--out <dir>]
//
// Needs the app running with the sample data (store/README.md): the UI at STELLARATOR_UI
// (default http://127.0.0.1:5180), its API at the same address under /api, and the mock model
// server behind the "DGX Spark vLLM" endpoint for the live run. The live run takes about four
// minutes: it resumes the long run, lets it run at 12 seqs, raises seqs to 32 so the chart shows the
// change, captures, and pauses the run again.

import { createRequire } from "node:module"
import { execSync } from "node:child_process"
import { mkdirSync, readFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const here = dirname(fileURLToPath(import.meta.url))
const BASE = (process.env.STELLARATOR_UI ?? "http://127.0.0.1:5180").replace(/\/$/, "")
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

/** The window each platform gives the page (docs/desktop.md, "The title bar on macOS"). */
const SHELLS = {
  mac: { viewport: { width: 1280, height: 800 }, desktop: { titleBar: "overlay", windowRadius: 26 } },
  windows: { viewport: { width: 1280, height: 720 }, desktop: { windowRadius: 8 } },
  linux: { viewport: { width: 1280, height: 720 }, desktop: { windowRadius: 0 } },
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

/** Each screen: how to get there and into the state the image shows. */
const SHOTS = {
  "run-live": async (page, ctx) => {
    await page.goto(`${BASE}/runs/${ctx.liveRun.id}`, { waitUntil: "networkidle" })
    await settle(page, 7000)
  },
  "run-new": async (page) => {
    await page.goto(`${BASE}/runs/new`, { waitUntil: "networkidle" })
    await settle(page)
    await page.getByRole("combobox", { name: "Endpoint" }).click()
    await page.getByRole("option", { name: /DGX Spark/ }).click()
    await settle(page)
    await page.locator("#model").click()
    await page.getByRole("option", { name: "mock-reasoning-content" }).click()
    await page.getByRole("combobox", { name: "Prompt set" }).click()
    await page.getByRole("option", { name: /gsm8k/ }).click()
    await page.locator("#concurrency").fill("32")
    await page.locator("#temperature").fill("0.6")
    await page.locator("#temperature").blur()
    await settle(page)
  },
  import: async (page) => {
    await page.goto(`${BASE}/prompts/import`, { waitUntil: "networkidle" })
    await settle(page, 500)
    const field = page.locator("input").first()
    await field.fill(HF_DATASET)
    await field.press("Enter")
    try {
      await page.getByRole("button", { name: /^Import [\d,]+ prompts/ }).waitFor({ timeout: 45000 })
      await settle(page, 2500)
      return
    } catch {
      console.warn(`  ${HF_DATASET} did not load (offline?); showing the paste source instead`)
    }
    await page.getByRole("tab", { name: "Paste" }).click()
    await page.locator("textarea").first().fill(PASTE_TEXT)
    await settle(page, 2500)
  },
  endpoints: async (page) => {
    await page.goto(`${BASE}/endpoints`, { waitUntil: "networkidle" })
    await settle(page, 500)
    await page.getByRole("button", { name: "Add endpoint" }).first().click()
    await settle(page, 500)
    await page.getByRole("button", { name: "vLLM", exact: true }).click()
    await settle(page, 300)
    await page.locator("#ep-name").fill("GPU server vLLM")
    await page.locator("#ep-base_url").fill("http://192.168.1.50:8000/v1")
    await page.locator("#ep-name").focus()
    await settle(page, 600)
  },
  preferences: async (page) => {
    await page.goto(`${BASE}/preferences/1`, { waitUntil: "networkidle" })
    await settle(page, 1200)
  },
  export: async (page) => {
    await page.goto(`${BASE}/runs/1`, { waitUntil: "networkidle" })
    await settle(page)
    await page.getByRole("button", { name: /export/i }).first().click()
    await settle(page, 1500)
  },
  sample: async (page) => {
    await page.goto(`${BASE}/runs/1`, { waitUntil: "networkidle" })
    await settle(page)
    await page.locator("table tbody tr").nth(1).locator("a, button").first().click()
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
  const ctxFor = async ({ theme, shell }) => {
    const c = await browser.newContext({
      viewport: SHELLS[shell].viewport,
      deviceScaleFactor: 2,
      colorScheme: theme,
      reducedMotion: "reduce",
    })
    await c.addInitScript((d) => {
      window.__STELLARATOR_DESKTOP__ = Object.freeze(d)
    }, SHELLS[shell].desktop)
    return c
  }
  const capture = async (key, job, ctx) => {
    const c = await ctxFor(job)
    const page = await c.newPage()
    await SHOTS[job.shot](page, ctx)
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
