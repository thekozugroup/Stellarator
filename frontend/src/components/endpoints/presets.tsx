import { useQuery } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { api } from "@/lib/api"
import type { EndpointPreset } from "@/lib/types"

/** Presets rarely change while the app runs; a failed fetch just hides the preset row. */
export function useEndpointPresets() {
  return useQuery({ // taste-ignore: missing-states -- a hook; callers hide the preset row while loading or on error
    queryKey: ["endpoint-presets"],
    queryFn: () => api.get<EndpointPreset[]>("/api/endpoint-presets"),
    staleTime: 5 * 60_000,
    retry: false,
  })
}

/** The one-tap choices at the top of Add endpoint. `value` is a preset id, "custom", or null before a pick. */
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
        <Button
          key={o.id}
          type="button"
          variant="outline"
          size="sm"
          aria-pressed={value === o.id}
          className="pointer-coarse:h-11 aria-pressed:border-foreground aria-pressed:bg-muted"
          onClick={() => onPick(o.preset)}
        >
          {o.name}
        </Button>
      ))}
    </div>
  )
}
