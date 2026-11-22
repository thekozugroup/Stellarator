import { useQuery } from "@tanstack/react-query"
import { api } from "@/lib/api"
import type { EndpointPreset } from "@/lib/types"
import { cn } from "@/lib/utils"

/** Presets rarely change while the app runs; a failed fetch just hides the preset row. */
export function useEndpointPresets() {
  return useQuery({ // taste-ignore: missing-states -- a hook; callers hide the preset row while loading or on error
    queryKey: ["endpoint-presets"],
    queryFn: () => api.get<EndpointPreset[]>("/api/endpoint-presets"),
    staleTime: 5 * 60_000,
    retry: false,
  })
}

/**
 * The one-tap choices at the top of Add endpoint, as chips (DESIGN.md, Chips): 28 px capsules in
 * the raised tone; the picked one in the accent tint. `value` is a preset id, "custom", or null
 * before a pick.
 */
export function PresetRow({
  presets,
  value,
  onPick,
}: {
  presets: EndpointPreset[]
  value: string | null
  onPick: (preset: EndpointPreset | null) => void
}) {
  return (
    <div role="group" aria-label="Preset" className="flex flex-wrap gap-1.5">
      {[...presets.map((p) => ({ id: p.id as string, name: p.name, preset: p as EndpointPreset | null })), { id: "custom", name: "Custom", preset: null }].map((o) => (
        <button
          key={o.id}
          type="button"
          aria-pressed={value === o.id}
          className={cn(
            "inline-flex h-7 items-center rounded-full bg-secondary px-3 text-sm whitespace-nowrap text-foreground transition-colors duration-150 select-none",
            "outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
            "aria-pressed:bg-periwinkle-tint aria-pressed:font-medium aria-pressed:text-periwinkle-ink",
          )}
          onClick={() => onPick(o.preset)}
        >
          {o.name}
        </button>
      ))}
    </div>
  )
}
