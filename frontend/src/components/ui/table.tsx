import * as React from "react"
import { cn } from "@/lib/utils"

/*
 * A macOS list (DESIGN.md, Components): no box around it, hairline separators between rows, a
 * column header in 12 px secondary text that stays under the toolbar while the page scrolls.
 * - Rows are 32 px on pointer-fine and 44 pt on touch. Hover is a neutral tint; the selected row
 *   (aria-selected or data-state="selected") is --accent, never copper.
 * - Header cells stick at --top-chrome (the toolbar) plus --sticky-below, which a page sets on an
 *   ancestor when it keeps its own sticky bar above the table.
 * - Numbers: give the cell `numeric` and the header above it too; they right-align in tabular
 *   figures.
 * - The container does not scroll sideways, because a scroll container would stop the header
 *   from sticking to the page. Lay wide tables out with table-fixed and truncating cells, and
 *   show the table from 1024 px (lg) with one row per item below it, as Runs, Prompt sets,
 *   Endpoints, Preferences and the run's samples do: a 768 to 1023 px content column is 480 to
 *   735 px wide, too narrow for their columns.
 */
function Table({ className, ...props }: React.ComponentProps<"table">) {
  return (
    <div data-slot="table-container" className="relative w-full">
      <table
        data-slot="table"
        className={cn("w-full border-separate border-spacing-0 caption-bottom text-sm", className)}
        {...props}
      />
    </div>
  )
}

function TableHeader({ className, ...props }: React.ComponentProps<"thead">) {
  return <thead data-slot="table-header" className={className} {...props} />
}

function TableBody({ className, ...props }: React.ComponentProps<"tbody">) {
  return (
    <tbody
      data-slot="table-body"
      className={cn("[&_tr:last-child>td]:border-b-0", className)}
      {...props}
    />
  )
}

function TableFooter({ className, ...props }: React.ComponentProps<"tfoot">) {
  return (
    <tfoot
      data-slot="table-footer"
      className={cn("[&>tr>td]:border-t [&>tr>td]:border-b-0", className)}
      {...props}
    />
  )
}

function TableRow({ className, ...props }: React.ComponentProps<"tr">) {
  return (
    <tr
      data-slot="table-row"
      className={cn(
        "transition-colors duration-150 hover:bg-foreground/4 has-[a:focus-visible]:bg-foreground/4 has-aria-expanded:bg-foreground/4 aria-selected:bg-accent data-[state=selected]:bg-accent",
        className
      )}
      {...props}
    />
  )
}

function TableHead({
  className,
  numeric,
  ...props
}: React.ComponentProps<"th"> & { numeric?: boolean }) {
  return (
    <th
      data-slot="table-head"
      className={cn(
        "sticky top-[calc(var(--top-chrome)+var(--sticky-below,0px))] z-10 h-7 border-b bg-background px-3 text-left align-middle text-xs font-normal whitespace-nowrap text-muted-foreground pointer-coarse:h-11 [&:has([role=checkbox])]:pr-0",
        numeric && "text-right tabular",
        className
      )}
      {...props}
    />
  )
}

function TableCell({
  className,
  numeric,
  ...props
}: React.ComponentProps<"td"> & { numeric?: boolean }) {
  return (
    <td
      data-slot="table-cell"
      className={cn(
        "h-8 border-b px-3 py-1 align-middle whitespace-nowrap pointer-coarse:h-11 [&:has([role=checkbox])]:pr-0",
        numeric && "text-right tabular",
        className
      )}
      {...props}
    />
  )
}

function TableCaption({
  className,
  ...props
}: React.ComponentProps<"caption">) {
  return (
    <caption
      data-slot="table-caption"
      className={cn("mt-4 text-sm text-muted-foreground", className)}
      {...props}
    />
  )
}

export {
  Table,
  TableHeader,
  TableBody,
  TableFooter,
  TableHead,
  TableRow,
  TableCell,
  TableCaption,
}
