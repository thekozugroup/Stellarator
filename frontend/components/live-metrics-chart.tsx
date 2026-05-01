"use client";

import { useMemo } from "react";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { chartTokens } from "@/lib/chart-tokens";
import type { RunMetric } from "@/lib/types";

export function LiveMetricsChart({
  metrics,
  height = 320,
}: {
  metrics: RunMetric[];
  height?: number;
}) {
  const data = useMemo(
    () =>
      metrics.map((m) => ({
        step: m.step,
        loss: m.loss ?? null,
        eval_loss: m.eval_loss ?? null,
        lr: m.lr ?? null,
      })),
    [metrics],
  );

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

  const { gridStroke, tickFill, tooltipBg, tooltipBorder, borderRadiusPx } =
    chartTokens;

  return (
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
        <Line
          type="monotone"
          dataKey="loss"
          stroke="var(--color-chart-1)"
          strokeWidth={2}
          dot={false}
          isAnimationActive={false}
        />
        <Line
          type="monotone"
          dataKey="eval_loss"
          stroke="var(--color-chart-2)"
          strokeWidth={2}
          dot={false}
          strokeDasharray="4 3"
          isAnimationActive={false}
        />
      </LineChart>
    </ResponsiveContainer>
  );
}
