// Frames the raw captures into the store images listed in images.json, and renders the icons.
//
// Each image is an HTML page (background, headline, the window with its platform frame) that
// headless Chromium screenshots at the exact pixel size. Text is set in Inter from the app's own
// font files, so no fonts need installing.
//
//   node store/scripts/compose.mjs [--only <substring>] [--raw <dir>] [--out <dir>]
//
// Then run finish.py to flatten, resize the copies and optimize the PNGs.

import { createRequire } from "node:module"
import { execSync } from "node:child_process"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath, pathToFileURL } from "node:url"

const here = dirname(fileURLToPath(import.meta.url))
const repo = resolve(here, "..", "..")
const args = process.argv.slice(2)
const opt = (name) => {
  const i = args.indexOf(name)
  return i >= 0 ? args[i + 1] : undefined
}
const RAW = resolve(opt("--raw") ?? join(here, "..", ".work", "raw"))
const OUT = resolve(opt("--out") ?? join(here, "..", "images"))
const ONLY = opt("--only")
const WORK = resolve(join(here, "..", ".work", "pages"))

function loadPlaywright() {
  const require = createRequire(import.meta.url)
  try {
    return require("playwright")
  } catch {
    const root = execSync("npm root -g").toString().trim()
    return require(join(root, "playwright"))
  }
}

const url = (p) => pathToFileURL(p).href
const fonts = join(repo, "frontend", "node_modules", "@fontsource-variable", "inter", "files")
const FONT_CSS = ["latin", "latin-ext"]
  .map(
    (subset) => `@font-face {
  font-family: "Inter Store";
  src: url("${url(join(fonts, `inter-${subset}-opsz-normal.woff2`))}") format("woff2");
  font-weight: 100 900;
  font-style: normal;
}`,
  )
  .join("\n")

/** Backgrounds: the icon's gradient, a light pair, and a neutral gray for Linux. */
const BACKGROUNDS = {
  indigo: { css: "linear-gradient(180deg, #5566dc 0%, #2c3684 100%)", ink: "#ffffff", sub: "rgba(255,255,255,0.84)", shadow: 0.5 },
  light: { css: "linear-gradient(180deg, #eef1fe 0%, #c3cffb 100%)", ink: "#0b1515", sub: "#2c3684", shadow: 0.28 },
  neutral: { css: "linear-gradient(180deg, #f4f4f6 0%, #e4e4e8 100%)", ink: "#0b1515", sub: "#5e5e5e", shadow: 0.26 },
}

/**
 * The platform frames, in window units (CSS px of the 1280 px wide capture). `bar` is the height
 * of a title bar drawn above the capture; macOS has none (the app draws under its toolbar).
 */
const FRAMES = {
  mac: { radius: 26, bar: 0 },
  windows: { radius: 8, bar: 32 },
  linux: { radius: [8, 8, 0, 0], bar: 38 },
}

const ICON_32 = url(join(repo, "desktop", "src-tauri", "icons", "32x32.png"))
const ICON_MAC = url(join(repo, "desktop", "icon-src", "icon-macos.svg"))

function titleBar(frame, theme) {
  const dark = theme === "dark"
  if (frame === "windows") {
    const fg = dark ? "#ffffff" : "#1b1b1b"
    const glyph = (d) =>
      `<span class="cap"><svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="${fg}" stroke-width="1">${d}</svg></span>`
    return `<div class="bar win-bar" style="background:${dark ? "#202020" : "#f3f3f3"};color:${fg}">
      <img src="${ICON_32}" width="16" height="16" alt="">
      <span class="title">Stellarator</span>
      <span class="caps">${glyph('<path d="M0 5.5h10"/>')}${glyph('<rect x="0.5" y="0.5" width="9" height="9"/>')}${glyph('<path d="M0.5 0.5l9 9M9.5 0.5l-9 9"/>')}</span>
    </div>`
  }
  if (frame === "linux") {
    const fg = dark ? "#eeeeec" : "#2e3436"
    const btn = (d) =>
      `<span class="gbtn" style="background:${dark ? "rgba(255,255,255,0.1)" : "rgba(0,0,0,0.08)"}"><svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="${fg}" stroke-width="1.5" stroke-linecap="round">${d}</svg></span>`
    return `<div class="bar gtk-bar" style="background:${dark ? "#303030" : "#ebebeb"};color:${fg};border-bottom:1px solid ${dark ? "#1f1f1f" : "#d6d6d6"}">
      <span class="title">Stellarator</span>
      <span class="gbtns">${btn('<path d="M4.5 11.5h7"/>')}${btn('<rect x="4.5" y="4.5" width="7" height="7" rx="1"/>')}${btn('<path d="M4.75 4.75l6.5 6.5M11.25 4.75l-6.5 6.5"/>')}</span>
    </div>`
  }
  return ""
}

/** macOS 26 window controls with a unified toolbar: 14 pt across, centers at (26, 26), (49, 26), (72, 26). */
const TRAFFIC = [
  ["#ff5f57", "#e0443e"],
  ["#febc2e", "#dea123"],
  ["#28c840", "#1aab29"],
]
  .map(([fill, edge], i) => `<span class="light" style="left:${26 + 23 * i - 7}px;background:${fill};box-shadow:inset 0 0 0 0.5px ${edge}"></span>`)
  .join("")

/** The window: a frame drawn at window units and zoomed to `scale`, placed at (x, y) in canvas px. */
function windowHtml(img, raw, x, y, scale, bg) {
  const f = FRAMES[img.frame]
  const w = 1280
  const h = raw.height + f.bar
  const radii = (Array.isArray(f.radius) ? f.radius : [f.radius, f.radius, f.radius, f.radius]).map((r) => `${r * scale}px`).join(" ")
  const dark = img.theme === "dark"
  const s = BACKGROUNDS[bg].shadow
  const shadow =
    img.frame === "mac"
      ? `0 ${30 * scale}px ${80 * scale}px rgba(8,12,40,${s}), 0 ${10 * scale}px ${24 * scale}px rgba(8,12,40,${s * 0.5})`
      : `0 ${12 * scale}px ${40 * scale}px rgba(8,12,40,${s * 0.85}), 0 ${3 * scale}px ${10 * scale}px rgba(8,12,40,${s * 0.4})`
  const edge = dark ? "rgba(255,255,255,0.14)" : "rgba(0,0,0,0.16)"
  return `<div class="win" style="left:${x}px;top:${y}px;width:${w * scale}px;height:${h * scale}px;border-radius:${radii};box-shadow:${shadow}">
    <div class="inner" style="zoom:${scale};width:${w}px;height:${h}px">
      ${titleBar(img.frame, img.theme)}
      <img class="shot" src="${url(raw.path)}" width="${w}" height="${raw.height}" alt="">
      ${img.frame === "mac" ? TRAFFIC : ""}
    </div>
    <div class="edge" style="border-radius:${radii};box-shadow:inset 0 0 0 ${Math.max(1, scale)}px ${edge}"></div>
  </div>`
}

/** Positions, in canvas px, for the three layouts: headline above the window, the window alone, social card. */
function page(img, raw) {
  const [W, H] = img.size
  const bg = BACKGROUNDS[img.background]
  const f = FRAMES[img.frame]
  const winH = raw.height + f.bar
  let body = ""
  if (img.layout === "social") {
    // Text on the left; the window to the right, running off the right edge.
    const scale = 0.62
    body = `<div class="social">
        <img class="appicon" src="${ICON_MAC}" alt="">
        <div class="name" style="color:${bg.ink}">${img.headline}</div>
        <div class="desc" style="color:${bg.sub}">${img.subline}</div>
        <div class="note" style="color:${bg.sub}">${img.note}</div>
      </div>
      ${windowHtml(img, raw, 640, Math.round((H - winH * scale) / 2) + 6, scale, img.background)}`
  } else if (img.headline) {
    // Type scaled to the canvas: 3.33% of the width for the headline, 1.53% for the subline.
    const hl = Math.round(W * 0.0333)
    const sl = Math.round(W * 0.0153)
    const top = Math.round(H * 0.056)
    const textH = Math.round(hl * 1.12) + Math.round(hl * 0.22) + Math.round(sl * 1.3)
    const gap = Math.round(H * 0.042)
    const bottom = Math.round(H * 0.05)
    const avail = H - top - textH - gap - bottom
    const scale = Math.min(avail / winH, (W * 0.86) / 1280)
    const x = Math.round((W - 1280 * scale) / 2)
    const y = top + textH + gap
    body = `<div class="head" style="top:${top}px">
        <div class="hl" style="font-size:${hl}px;color:${bg.ink}">${img.headline}</div>
        <div class="sl" style="font-size:${sl}px;margin-top:${Math.round(hl * 0.22)}px;color:${bg.sub}">${img.subline}</div>
      </div>
      ${windowHtml(img, raw, x, y, scale, img.background)}`
  } else {
    const margin = Math.round(H * (img.frame === "mac" ? 0.075 : 0.06))
    const scale = Math.min((H - 2 * margin) / winH, (W - 2 * margin) / 1280)
    const x = Math.round((W - 1280 * scale) / 2)
    const y = Math.round((H - winH * scale) / 2) - Math.round(margin * 0.15)
    body = windowHtml(img, raw, x, y, scale, img.background)
  }
  return `<!doctype html><html><head><meta charset="utf-8"><style>
${FONT_CSS}
html, body { margin: 0; width: ${W}px; height: ${H}px; overflow: hidden; }
body { position: relative; background: ${bg.css}; font-family: "Inter Store", sans-serif; font-optical-sizing: auto;
  -webkit-font-smoothing: antialiased; text-rendering: geometricPrecision; }
.head { position: absolute; left: 0; right: 0; text-align: center; }
.hl { font-weight: 600; letter-spacing: -0.025em; line-height: 1.12; }
.sl { font-weight: 450; letter-spacing: -0.006em; line-height: 1.3; }
.win { position: absolute; overflow: hidden; isolation: isolate; }
.inner { position: relative; }
.shot { display: block; }
.edge { position: absolute; inset: 0; pointer-events: none; }
.light { position: absolute; top: 19px; width: 14px; height: 14px; border-radius: 50%; }
.bar { position: relative; display: flex; align-items: center; box-sizing: border-box; font-family: "Inter Store", sans-serif; }
.win-bar { height: 32px; padding-left: 12px; font-size: 12px; }
.win-bar .title { margin-left: 12px; }
.win-bar .caps { margin-left: auto; display: flex; height: 32px; }
.cap { width: 46px; height: 32px; display: flex; align-items: center; justify-content: center; }
.gtk-bar { height: 38px; justify-content: center; font-size: 13px; font-weight: 700; }
.gtk-bar .gbtns { position: absolute; right: 7px; top: 7px; display: flex; gap: 10px; }
.gbtn { width: 24px; height: 24px; border-radius: 50%; display: flex; align-items: center; justify-content: center; }
.social { position: absolute; left: 72px; top: 0; bottom: 0; width: 540px; display: flex; flex-direction: column; justify-content: center; }
.appicon { width: 148px; height: 148px; margin: -14px 0 6px -14px; }
.name { font-size: 76px; font-weight: 600; letter-spacing: -0.03em; line-height: 1.05; }
.desc { font-size: 30px; font-weight: 450; letter-spacing: -0.012em; line-height: 1.25; margin-top: 18px; max-width: 520px; }
.note { font-size: 21px; font-weight: 450; line-height: 1.3; margin-top: 22px; opacity: 0.85; }
</style></head><body>${body}</body></html>`
}

function pngSize(path) {
  const b = readFileSync(path)
  return { width: b.readUInt32BE(16), height: b.readUInt32BE(20) }
}

async function main() {
  const spec = JSON.parse(readFileSync(join(here, "images.json"), "utf8"))
  mkdirSync(WORK, { recursive: true })
  const { chromium } = loadPlaywright()
  const browser = await chromium.launch()
  const missing = []
  for (const img of spec.images) {
    if (ONLY && !img.file.includes(ONLY)) continue
    const shell = img.frame === "windows" ? "windows" : img.frame === "linux" ? "linux" : "mac"
    const rawPath = join(RAW, `${img.shot}.${img.theme}.${shell}.png`)
    if (!existsSync(rawPath)) {
      missing.push(rawPath)
      continue
    }
    const px = pngSize(rawPath)
    // Captures are 2x: the window is 1280 CSS px wide.
    const raw = { path: rawPath, height: Math.round(px.height / (px.width / 1280)) }
    const html = page(img, raw)
    const htmlPath = join(WORK, img.file.replace(/[/.]/g, "_") + ".html")
    writeFileSync(htmlPath, html)
    const [W, H] = img.size
    const ctx = await browser.newContext({ viewport: { width: W, height: H }, deviceScaleFactor: 1 })
    const p = await ctx.newPage()
    await p.goto(url(htmlPath), { waitUntil: "load" })
    await p.evaluate(() => document.fonts.ready)
    await p.waitForTimeout(150)
    const out = join(OUT, img.file)
    mkdirSync(dirname(out), { recursive: true })
    // Headless Chromium now and then fails a large capture once; a second try succeeds.
    await p.screenshot({ path: out }).catch(() => p.waitForTimeout(500).then(() => p.screenshot({ path: out })))
    await ctx.close()
    console.log(`wrote ${img.file} (${W}x${H})`)
  }
  for (const icon of spec.icons ?? []) {
    if (ONLY && !icon.file.includes(ONLY)) continue
    const ctx = await browser.newContext({ viewport: { width: icon.size, height: icon.size }, deviceScaleFactor: 1 })
    const p = await ctx.newPage()
    const htmlPath = join(WORK, icon.file.replace(/[/.]/g, "_") + ".html")
    writeFileSync(
      htmlPath,
      `<!doctype html><html><body style="margin:0;background:transparent"><img src="${url(join(repo, icon.source))}" style="display:block;width:${icon.size}px;height:${icon.size}px"></body></html>`,
    )
    await p.goto(url(htmlPath), { waitUntil: "load" })
    const out = join(OUT, icon.file)
    mkdirSync(dirname(out), { recursive: true })
    await p.screenshot({ path: out, omitBackground: !icon.opaque })
    await ctx.close()
    console.log(`wrote ${icon.file} (${icon.size}x${icon.size})`)
  }
  await browser.close()
  if (missing.length) {
    console.error(`\nMissing captures (run capture.mjs first):\n  ${missing.join("\n  ")}`)
    process.exit(1)
  }
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
