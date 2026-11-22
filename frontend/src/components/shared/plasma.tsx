import { useMemo } from "react"
import { cn } from "@/lib/utils"

/**
 * The pixel field (DESIGN.md, Identity): a square of cells stepping through a few palette colors
 * with ordered dithering, like a low-resolution image of plasma. It is the product's one gradient.
 * The same seed always draws the same field, so a tile identifies the endpoint or prompt set it
 * stands for. Decorative: always next to the name it stands for, never behind text.
 */

/** Color ramps from the palette (dark to light), one per field. */
const RAMPS = [
  ["#141a40", "#3a46b4", "#5566dc", "#a1b5f8", "#fc885e"], // plasma: navy, periwinkle, coral core
  ["#3a46b4", "#5566dc", "#a1b5f8", "#f5c2ab", "#fc885e"], // dawn
  ["#0b1515", "#1f4a3a", "#4f9a6a", "#9ed49c", "#e5f2c4"], // field: ink to green
  ["#141a40", "#2c3684", "#5566dc", "#a1b5f8", "#dde5fd"], // ice
  ["#2a1712", "#9a4424", "#e8703f", "#fc885e", "#ffd3bd"], // ember
  ["#0b1515", "#3a46b4", "#a1b5f8", "#9ed49c", "#e5f2c4"], // aurora: periwinkle into green
] as const

/** Navy to periwinkle only: white on every cell is at least 4.8:1, for tiles that carry the mark. */
const DEEP = ["#141a40", "#232c78", "#3a46b4", "#4a58cc", "#5566dc"] as const

/** 4 x 4 Bayer matrix, as thresholds in [-0.5, 0.5). */
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5].map((v) => v / 16 - 0.5)

/** FNV-1a: a stable 32-bit hash of the seed. */
function hash(seed: string): number {
  let h = 0x811c9dc5
  for (let i = 0; i < seed.length; i++) {
    h ^= seed.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return h >>> 0
}

/** The cells of a field, row by row: `cells * cells` colors. Exported for tests. */
export function plasmaCells(seed: string, cells: number, deep = false): string[] {
  const h = hash(seed)
  const ramp: readonly string[] = deep ? DEEP : RAMPS[h % RAMPS.length]
  const angle = ((h >>> 8) % 8) * (Math.PI / 4)
  const bend = (((h >>> 16) % 7) - 3) / 6 // curves the bands a little, -0.5 to 0.5
  const dx = Math.cos(angle)
  const dy = Math.sin(angle)
  const levels = ramp.length - 1
  const out: string[] = []
  for (let y = 0; y < cells; y++) {
    for (let x = 0; x < cells; x++) {
      const u = cells > 1 ? x / (cells - 1) - 0.5 : 0
      const v = cells > 1 ? y / (cells - 1) - 0.5 : 0
      // Position along the gradient, 0 to 1, with a gentle curve across it.
      const along = u * dx + v * dy
      const across = -u * dy + v * dx
      const t = Math.min(1, Math.max(0, 0.5 + along / 1.42 + bend * across * across))
      const level = Math.round(t * levels + BAYER[(y % 4) * 4 + (x % 4)] * 0.9)
      out.push(ramp[Math.min(levels, Math.max(0, level))])
    }
  }
  return out
}

/**
 * A square pixel-field tile. `cells` is how many cells run along each side (6 for small tiles, 8
 * or more for larger ones). Corners follow the size: 5 px at 20 px, 8 px at 28 px (pass a
 * rounded-* class). `deep` keeps to navy and periwinkle, for a tile with the white mark on it.
 */
export function Plasma({ seed, cells = 6, deep = false, className }: { seed: string; cells?: number; deep?: boolean; className?: string }) {
  const colors = useMemo(() => plasmaCells(seed, cells, deep), [seed, cells, deep])
  return (
    <span aria-hidden="true" className={cn("inline-block size-7 shrink-0 overflow-hidden rounded-sm", className)}>
      <svg viewBox={`0 0 ${cells} ${cells}`} shapeRendering="crispEdges" className="block size-full">
        {colors.map((fill, i) => (
          <rect key={i} x={i % cells} y={Math.floor(i / cells)} width={1.02} height={1.02} fill={fill} />
        ))}
      </svg>
    </span>
  )
}
