import { InfoTip } from "@/components/shared/info-tip"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { INFO, type ImportMode } from "./helpers"

/** "Simple | Advanced", styled like the Rendered/Raw switch. */
export function ImportModeToggle({ value, onChange }: { value: ImportMode; onChange: (m: ImportMode) => void }) {
  return (
    <div className="flex items-center gap-1.5">
      <ToggleGroup
        type="single"
        spacing={0}
        value={value}
        onValueChange={(v) => v && onChange(v as ImportMode)}
        aria-label="Import settings"
      >
        <ToggleGroupItem value="simple">
          Simple
        </ToggleGroupItem>
        <ToggleGroupItem value="advanced">
          Advanced
        </ToggleGroupItem>
      </ToggleGroup>
      <InfoTip label="Simple and Advanced">{INFO.mode}</InfoTip>
    </div>
  )
}
