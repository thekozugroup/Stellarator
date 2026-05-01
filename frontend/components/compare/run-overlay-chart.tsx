"use client";

import { useCallback, useMemo, useRef, useState } from "react";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  Legend,
} from "recharts";
import { chartTokens } from "@/lib/chart-tokens";
import type { RunMetric } from "@/lib/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface RunSeries {
  id: string;
  name: string;
  color: string;
  metrics: RunMetric[];
}

type XMode = "step" | "wall-clock" | "tokens";

// ---------------------------------------------------------------------------
// EMA smoothing
// ---------------------------------------------------------------------------

function ema(values: (number | null)[], alpha: number): (number | null)[] {
  if (alpha <= 0) return values;
  let last: number | null = null;
  return values.map((v) => {
    if (v == null) return null;
    last = last == null ? v : alpha * last + (1 - alpha) * v;
    return last;
  });
}

// ---------------------------------------------------------------------------
// RunOverlayChart
// ---------------------------------------------------------------------------

export interface RunOverlayChartProps {
  series: RunSeries[];
  height?: number;
}

export function RunOverlayChart({ series, height = 360 }: RunOverlayChartProps) {
  const [muted, setMuted] = useState<Set<string>>(new Set());
  const [solo, setSolo] = useState<string | null>(null);
  const [xMode, setXMode] = useState<XMode>("step");
  const [smoothing, setSmoothing] = useState(0);
  const longPressTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Mute / solo toggle
  const toggleMute = useCallback((id: string) => {
    setSolo(null);
    setMuted((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleLongPressStart = useCallback(
    (id: string) => {
      longPressTimer.current = setTimeout(() => {
        setMuted(new Set());
        setSolo((prev) => (prev === id ? null : id));
      }, 500);
    },
    [],
  );

  const handleLongPressEnd = useCallback(() => {
    if (longPressTimer.current) clearTimeout(longPressTimer.current);
  }, []);

  // Build chart data
  const chartData = useMemo(() => {
    const byKey = new Map<number | string, Record<string, number | null>>();
    series.forEach((s) => {
      const losses = s.metrics.map((m) => m.loss ?? null);
      const smoothed = ema(losses, smoothing);
      s.metrics.forEach((m, idx) => {
        const xVal =
          xMode === "step"
            ? m.step
            : xMode === "tokens"
            ? (m.tokens ?? m.step)
            : new Date(m.ts).getTime();
        const row = byKey.get(xVal) ?? { _x: xVal };
        row[s.id] = smoothed[idx];
        row[`${s.id}_eval`] = m.eval_loss ?? null;
        byKey.set(xVal, row);
      });
    });
    return Array.from(byKey.values()).sort(
      (a, b) => (a._x as number) - (b._x as number),
    );
  }, [series, xMode, smoothing]);

  const visibleIds = useMemo(
    () =>
      solo
        ? series.filter((s) => s.id === solo).map((s) => s.id)
        : series.filter((s) => !muted.has(s.id)).map((s) => s.id),
    [series, solo, muted],
  );

  const { gridStroke, tickFill, tooltipBg, tooltipBorder, borderRadiusPx } =
    chartTokens;

  const xTickFormatter = useMemo(() => {
    if (xMode === "wall-clock") {
      return (v: number) => {
        const d = new Date(v);
        return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
      };
    }
    return undefined;
  }, [xMode]);

  return (
    <div className="space-y-3">
      {/* Controls row */}
      <div className="flex flex-wrap items-center gap-3">
        {/* X-axis mode */}
        <div className="inline-flex rounded-md ring-1 ring-border overflow-hidden text-xs">
          {(["step", "wall-clock", "tokens"] as XMode[]).map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => setXMode(m)}
              className={`px-2.5 py-1 transition-colors ${
                xMode === m
                  ? "bg-primary text-primary-foreground"
                  : "bg-transparent text-muted-foreground hover:text-foreground"
              }`}
            >
              {m === "wall-clock" ? "Time" : m.charAt(0).toUpperCase() + m.slice(1)}
            </button>
          ))}
        </div>

        {/* Smoothing */}
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>Smooth</span>
          <input
            type="range"
            min={0}
            max={0.99}
            step={0.01}
            value={smoothing}
            onChange={(e) => setSmoothing(parseFloat(e.target.value))}
            className="w-24 accent-primary"
            aria-label="EMA smoothing alpha"
          />
          <span className="tabular-nums w-8">{smoothing.toFixed(2)}</span>
        </label>

        {/* Series toggles */}
        <div className="flex flex-wrap gap-1.5 ml-auto">
          {series.map((s) => {
            const isMuted = solo ? s.id !== solo : muted.has(s.id);
            return (
              <button
                key={s.id}
                type="button"
                onMouseDown={() => handleLongPressStart(s.id)}
                onMouseUp={() => {
                  handleLongPressEnd();
                  toggleMute(s.id);
                }}
                onTouchStart={() => handleLongPressStart(s.id)}
                onTouchEnd={() => {
                  handleLongPressEnd();
                  toggleMute(s.id);
                }}
                title={`${isMuted ? "Show" : "Hide"} ${s.name} (long-press to solo)`}
                aria-pressed={!isMuted}
                className={`inline-flex items-center gap-1.5 rounded px-2 py-0.5 text-[11px] ring-1 transition-opacity ${
                  isMuted ? "opacity-30" : "opacity-100"
                }`}
                style={{ color: s.color, outlineColor: s.color }}
              >
                <span
                  className="size-2 rounded-full"
                  style={{ background: s.color }}
                  aria-hidden
                />
                {s.name}
              </button>
            );
          })}
        </div>
      </div>

      {/* Chart */}
      <ResponsiveContainer width="100%" height={height}>
        <LineChart data={chartData} margin={{ top: 8, right: 16, left: 0, bottom: 8 }}>
          <CartesianGrid strokeDasharray="2 4" stroke={gridStroke} />
          <XAxis
            dataKey="_x"
            tick={{ fill: tickFill, fontSize: 11 }}
            stroke={gridStroke}
            tickFormatter={xTickFormatter}
          />
          <YAxis
            tick={{ fill: tickFill, fontSize: 11 }}
            stroke={gridStroke}
            width={48}
          />
          <Tooltip
            contentStyle={{
              background: tooltipBg,
              border: `1px solid ${tooltipBorder}`,
              borderRadius: borderRadiusPx,
              fontSize: 12,
            }}
          />
          <Legend wrapperStyle={{ fontSize: 11 }} />

          {series.map((s) => {
            const visible = visibleIds.includes(s.id);
            return [
              // Training loss line
              <Line
                key={s.id}
                type="monotone"
                dataKey={s.id}
                name={s.name}
                stroke={s.color}
                strokeWidth={visible ? 2 : 0}
                dot={false}
                isAnimationActive={false}
                connectNulls
                hide={!visible}
              />,
              // Eval loss — dashed, lower opacity
              <Line
                key={`${s.id}_eval`}
                type="monotone"
                dataKey={`${s.id}_eval`}
                name={`${s.name} (eval)`}
                stroke={s.color}
                strokeWidth={visible ? 1.5 : 0}
                strokeDasharray="4 3"
                strokeOpacity={0.5}
                dot={false}
                isAnimationActive={false}
                connectNulls
                hide={!visible}
                legendType="none"
              />,
            ];
          })}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
