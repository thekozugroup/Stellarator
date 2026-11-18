import { InfoTip } from "@/components/shared/info-tip"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { INFO } from "./helpers"

/**
 * Simple shows the source and what was detected; Advanced shows every setting. Both edit the same
 * state, so switching never resets a choice. The choice is remembered in this browser.
 */
export type ImportMode = "simple" | "advanced"

const KEY = "stellarator:import-mode"

export function readImportMode(): ImportMode {
  try {
    return window.localStorage.getItem(KEY) === "advanced" ? "advanced" : "simple"
  } catch {
    return "simple"
  }
}

export function saveImportMode(mode: ImportMode) {
  try {
    window.localStorage.setItem(KEY, mode)
  } catch {
    /* storage blocked: the choice lasts for this page only */
  }
}

/** "Simple | Advanced", styled like the Rendered/Raw switch. */
export function ImportModeToggle({ value, onChange }: { value: ImportMode; onChange: (m: ImportMode) => void }) {
  return (
    <div className="flex items-center gap-1.5">
      <ToggleGroup
        type="single"
        size="sm"
        variant="outline"
        spacing={0}
        value={value}
        onValueChange={(v) => v && onChange(v as ImportMode)}
        aria-label="Import settings"
      >
        <ToggleGroupItem value="simple" className="px-2.5 text-xs">
          Simple
        </ToggleGroupItem>
        <ToggleGroupItem value="advanced" className="px-2.5 text-xs">
          Advanced
        </ToggleGroupItem>
      </ToggleGroup>
      <InfoTip label="Simple and Advanced">{INFO.mode}</InfoTip>
    </div>
  )
}
