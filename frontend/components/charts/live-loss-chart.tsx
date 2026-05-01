"use client";

import { useMemo, useState } from "react";
import {
  CartesianGrid,
  Line,
  LineChart,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { chartTokens } from "@/lib/chart-tokens";
import type { RunMetric } from "@/lib/types";

type SeriesKey = "loss" | "eval_loss" | "lr" | "grad_norm" | "reward_mean" | "percent_correct";

const SERIES_CONFIG: Record<
  SeriesKey,
  { label: string; color: string; dashed?: boolean; strokeWidth?: number }
> = {
  loss: { label: "loss", color: "var(--color-chart-1)" },
  eval_loss: { label: "eval_loss", color: "var(--color-chart-2)", dashed: true },
  lr: { label: "lr", color: "var(--color-chart-3)", dashed: true },
  grad_norm: { label: "grad_norm", color: "var(--color-chart-4)" },
  reward_mean: { label: "reward_mean", color: "var(--color-chart-5)", strokeWidth: 2 },
  percent_correct: { label: "% correct", color: "oklch(0.72 0.18 145)", dashed: true },
};

const DEFAULT_ENABLED: SeriesKey[] = ["loss", "eval_loss", "reward_mean", "percent_correct"];

export interface LiveLossChartMetric {
  step: number;
  loss?: number | null;
  eval_loss?: number | null;
  lr?: number | null;
  grad_norm?: number | null;
  reward_mean?: number | null;
  percent_correct?: number | null;
}

export function LiveLossChart({
  metrics,
  height = 320,
}: {
  metrics: (RunMetric | LiveLossChartMetric)[];
  height?: number;
}) {
  const [enabled, setEnabled] = useState<Set<SeriesKey>>(new Set(DEFAULT_ENABLED));

  const data = useMemo(
    () =>
      metrics.map((m) => ({
        step: m.step,
        loss: (m as LiveLossChartMetric).loss ?? null,
        eval_loss: (m as LiveLossChartMetric).eval_loss ?? null,
        lr: (m as LiveLossChartMetric).lr ?? null,
        grad_norm: (m as LiveLossChartMetric).grad_norm ?? null,
        reward_mean: (m as LiveLossChartMetric).reward_mean ?? null,
        percent_correct: (m as LiveLossChartMetric).percent_correct ?? null,
      })),
    [metrics],
  );

  // Only render series that have at least one non-null value.
  const presentSeries = useMemo<SeriesKey[]>(() => {
    return (Object.keys(SERIES_CONFIG) as SeriesKey[]).filter((key) =>
      data.some((d) => d[key] != null),
    );
  }, [data]);

  const toggleSeries = (key: SeriesKey) => {
    setEnabled((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  if (!metrics.length) {
    return (
      <div
        className="flex items-center justify-center rounded-lg border border-dashed text-sm text-muted-foreground"
        style={{ height }}
      >
        Waiting for first metric...
      </div>
    );
  }

  const { gridStroke, tickFill, tooltipBg, tooltipBorder, borderRadiusPx } = chartTokens;

  return (
    <div className="space-y-3">
      {/* Toggle row */}
      <div className="flex flex-wrap gap-2">
        {presentSeries.map((key) => {
          const cfg = SERIES_CONFIG[key];
          const active = enabled.has(key);
          return (
            <button
              key={key}
              type="button"
              onClick={() => toggleSeries(key)}
              className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[11px] transition-opacity ${
                active ? "opacity-100" : "opacity-40"
              }`}
              style={{ borderColor: cfg.color }}
              aria-pressed={active}
            >
              <span
                className="inline-block h-0.5 w-4"
                style={{
                  background: cfg.dashed
                    ? `repeating-linear-gradient(90deg, ${cfg.color} 0 4px, transparent 4px 7px)`
                    : cfg.color,
                }}
              />
              <span className="font-mono text-foreground">{cfg.label}</span>
            </button>
          );
        })}
      </div>

      <ResponsiveContainer width="100%" height={height}>
        <LineChart data={data} margin={{ top: 8, right: 16, left: 0, bottom: 8 }}>
          <CartesianGrid strokeDasharray="2 4" stroke={gridStroke} />
          <XAxis
            dataKey="step"
            tick={{ fill: tickFill, fontSize: 11 }}
            stroke={gridStroke}
          />
          <YAxis
            tick={{ fill: tickFill, fontSize: 11 }}
            stroke={gridStroke}
            width={52}
          />
          <Tooltip
            contentStyle={{
              background: tooltipBg,
              border: `1px solid ${tooltipBorder}`,
              borderRadius: borderRadiusPx,
              fontSize: 12,
            }}
          />
          {presentSeries
            .filter((key) => enabled.has(key))
            .map((key) => {
              const cfg = SERIES_CONFIG[key];
              return (
                <Line
                  key={key}
                  type="monotone"
                  dataKey={key}
                  name={cfg.label}
                  stroke={cfg.color}
                  strokeWidth={cfg.strokeWidth ?? 2}
                  dot={false}
                  strokeDasharray={cfg.dashed ? "4 3" : undefined}
                  isAnimationActive={false}
                  connectNulls={false}
                />
              );
            })}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
