/**
 * chart-tokens.ts — single source of truth for Recharts CSS-var color lookups.
 *
 * Instead of hardcoding oklch(...) strings inside chart components, call these
 * helpers at render time so tokens survive theme swaps.
 *
 * All functions are safe to call in a useEffect or inside a recharts customizer
 * callback (they read getComputedStyle on documentElement).
 */

function cssVar(name: string, fallback = ""): string {
  if (typeof document === "undefined") return fallback;
  return (
    getComputedStyle(document.documentElement).getPropertyValue(name).trim() ||
    fallback
  );
}

/** Resolved CSS-var chart palette */
export const chartTokens = {
  /** Grid / axis stroke color */
  get gridStroke() {
    return cssVar("--color-border", "oklch(0.3 0.005 286)");
  },
  /** Axis tick color */
  get tickFill() {
    return cssVar("--color-muted-foreground", "oklch(0.6 0.01 286)");
  },
  /** Tooltip background */
  get tooltipBg() {
    return cssVar("--color-popover", "oklch(0.18 0.005 286)");
  },
  /** Tooltip border */
  get tooltipBorder() {
    return cssVar("--color-border", "oklch(0.27 0.008 286)");
  },
  /** Resolved --radius in px (converts rem to px using fontSize) */
  get borderRadiusPx(): number {
    const raw = cssVar("--radius", "0.625rem");
    if (raw.endsWith("rem")) {
      const rem = parseFloat(raw);
      const base =
        typeof document !== "undefined"
          ? parseFloat(getComputedStyle(document.documentElement).fontSize)
          : 16;
      return rem * base;
    }
    return parseFloat(raw) || 8;
  },
  chart1() {
    return cssVar("--color-chart-1", "oklch(0.78 0.16 75)");
  },
  chart2() {
    return cssVar("--color-chart-2", "oklch(0.7 0.18 195)");
  },
  chart3() {
    return cssVar("--color-chart-3", "oklch(0.65 0.22 320)");
  },
  chart4() {
    return cssVar("--color-chart-4", "oklch(0.72 0.18 145)");
  },
  chart5() {
    return cssVar("--color-chart-5", "oklch(0.7 0.2 25)");
  },
};
