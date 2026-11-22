import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "@/lib/utils"
import { Slot } from "radix-ui"

/*
 * Buttons (DESIGN.md, Components): rounded rectangles with 10 px corners, 32 px tall (44 pt on
 * touch, index.css), 13 px at weight 500. Small buttons are 28 px with 8 px corners.
 * - default: the ink primary, at most one per view.
 * - secondary: a raised tone (--secondary) with ink text: toolbar actions and most buttons.
 * - outline: transparent with a field-strength edge, for a button that must read as one on a busy
 *   surface.
 * - ghost: no fill until hovered: row actions.
 * - destructive: red ink on the red tint.
 * In the page toolbar (Page, data-slot="toolbar") outline and ghost buttons take the secondary
 * look, so every toolbar item is a filled tonal button beside the one ink primary; in the phone
 * bar every button is a 44 pt circle.
 */
const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center rounded-md border border-transparent bg-clip-padding text-sm font-medium whitespace-nowrap transition-[color,background-color,border-color,box-shadow,opacity,transform] duration-150 outline-none select-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background active:not-aria-[haspopup]:scale-[0.98] disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-2 aria-invalid:ring-destructive/30 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/88",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-accent aria-expanded:bg-accent dark:hover:bg-[color-mix(in_oklch,var(--accent),var(--foreground)_6%)] dark:aria-expanded:bg-[color-mix(in_oklch,var(--accent),var(--foreground)_6%)]",
        outline:
          "border-input bg-transparent hover:bg-muted aria-expanded:bg-muted in-data-[slot=toolbar]:border-transparent in-data-[slot=toolbar]:bg-secondary in-data-[slot=toolbar]:hover:bg-accent in-data-[slot=toolbar]:aria-expanded:bg-accent",
        ghost:
          "hover:bg-accent hover:text-foreground aria-expanded:bg-accent aria-expanded:text-foreground in-data-[slot=toolbar]:bg-secondary",
        destructive:
          "bg-negative-tint text-negative-ink hover:bg-[color-mix(in_oklch,var(--destructive)_20%,var(--background))] focus-visible:ring-destructive",
        link: "text-foreground underline underline-offset-4 decoration-foreground/30 hover:decoration-foreground",
      },
      size: {
        default:
          "h-8 gap-1.5 px-3 has-data-[icon=inline-end]:pr-2.5 has-data-[icon=inline-start]:pl-2.5",
        xs: "h-6 gap-1 rounded-sm px-2 text-xs has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-7 gap-1 rounded-sm px-2.5 text-xs has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2 [&_svg:not([class*='size-'])]:size-3.5",
        lg: "h-9 gap-1.5 px-3.5 has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3",
        icon: "size-8",
        "icon-xs": "size-6 rounded-sm [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-7 rounded-sm",
        "icon-lg": "size-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
