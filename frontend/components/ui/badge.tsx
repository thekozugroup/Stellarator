import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium transition-colors duration-[var(--duration-fast)] ease-[var(--ease-out)] focus:outline-none",
  {
    variants: {
      variant: {
        // Canonical shadcn variants
        default:
          "border-transparent bg-primary text-primary-foreground",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground",
        destructive:
          "border-transparent bg-destructive text-destructive-foreground",
        outline: "text-foreground",
        // Extended semantic variants — semantic tokens survive theme swaps
        success:
          "border-transparent bg-success/15 text-success ring-1 ring-success/30",
        warning:
          "border-transparent bg-warning/15 text-warning ring-1 ring-warning/30",
        info:
          "border-transparent bg-info/15 text-info ring-1 ring-info/30",
        muted:
          "border-transparent bg-muted text-muted-foreground ring-1 ring-border",
      },
    },
    defaultVariants: { variant: "default" },
  },
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <div className={cn(badgeVariants({ variant }), className)} {...props} />;
}

export { badgeVariants };
