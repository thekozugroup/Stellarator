"use client"

import * as React from "react"
import { type VariantProps } from "class-variance-authority"
import { cn } from "@/lib/utils"
import { ToggleGroup as ToggleGroupPrimitive } from "radix-ui"

import { toggleVariants } from "@/components/ui/toggle"

const ToggleGroupContext = React.createContext<
  VariantProps<typeof toggleVariants> & {
    spacing?: number
    orientation?: "horizontal" | "vertical"
  }
>({
  size: "default",
  variant: "default",
  spacing: 2,
  orientation: "horizontal",
})

/**
 * With spacing={0} a toggle group is a macOS 26 segmented control: a capsule track
 * (--segment-track) holding capsule segments, the chosen one raised (control fill, hairline, small
 * shadow) with ink text at weight 500. Segments are 24 px tall on pointer-fine (a 28 px control),
 * 28 px elsewhere (44 pt on touch). With spacing > 0 the items are separate toggles.
 */
function ToggleGroup({
  className,
  variant,
  size,
  spacing = 2,
  orientation = "horizontal",
  children,
  ...props
}: React.ComponentProps<typeof ToggleGroupPrimitive.Root> &
  VariantProps<typeof toggleVariants> & {
    spacing?: number
    orientation?: "horizontal" | "vertical"
  }) {
  return (
    <ToggleGroupPrimitive.Root
      data-slot="toggle-group"
      data-variant={variant}
      data-size={size}
      data-spacing={spacing}
      data-orientation={orientation}
      style={{ "--gap": spacing } as React.CSSProperties}
      className={cn(
        "group/toggle-group flex w-fit flex-row items-center gap-[--spacing(var(--gap))] rounded-lg data-[size=sm]:rounded-md data-vertical:flex-col data-vertical:items-stretch",
        spacing === 0 && "gap-0 rounded-2xl bg-segment-track p-0.5 data-[size=sm]:rounded-2xl pointer-coarse:rounded-4xl pointer-coarse:data-[size=sm]:rounded-4xl",
        className
      )}
      {...props}
    >
      <ToggleGroupContext.Provider
        value={{ variant, size, spacing, orientation }}
      >
        {children}
      </ToggleGroupContext.Provider>
    </ToggleGroupPrimitive.Root>
  )
}

const SEGMENT =
  "inline-flex h-7 min-w-7 shrink-0 items-center justify-center gap-1 rounded-full border border-transparent px-2 text-sm whitespace-nowrap text-muted-foreground transition-[color,background-color,border-color,box-shadow] outline-none hover:text-foreground focus-visible:z-10 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 data-[state=on]:border-control-edge data-[state=on]:bg-control data-[state=on]:font-medium data-[state=on]:text-foreground data-[state=on]:shadow-xs pointer-fine:h-6 pointer-fine:min-w-6 pointer-fine:px-2.5 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4"

function ToggleGroupItem({
  className,
  children,
  variant = "default",
  size = "default",
  ...props
}: React.ComponentProps<typeof ToggleGroupPrimitive.Item> &
  VariantProps<typeof toggleVariants>) {
  const context = React.useContext(ToggleGroupContext)
  const segmented = context.spacing === 0

  return (
    <ToggleGroupPrimitive.Item
      data-slot="toggle-group-item"
      data-variant={context.variant || variant}
      data-size={context.size || size}
      data-spacing={context.spacing}
      className={cn(
        segmented
          ? SEGMENT
          : cn(
              "shrink-0 focus:z-10 focus-visible:z-10",
              toggleVariants({
                variant: context.variant || variant,
                size: context.size || size,
              })
            ),
        className
      )}
      {...props}
    >
      {children}
    </ToggleGroupPrimitive.Item>
  )
}

export { ToggleGroup, ToggleGroupItem }
