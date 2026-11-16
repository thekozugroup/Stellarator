import { useState } from "react"
import { ChevronsUpDown } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { cn } from "@/lib/utils"

/**
 * Pick a model the endpoint listed, or type any other model id. The typed id is offered as the
 * first choice when it matches nothing, so a model the server does not list can still be used.
 */
export function ModelCombobox({
  id,
  value,
  onChange,
  models,
  invalid,
  describedBy,
  disabled,
}: {
  id: string
  value: string
  onChange: (model: string) => void
  models: string[]
  invalid?: boolean
  describedBy?: string
  disabled?: boolean
}) {
  const [open, setOpen] = useState(false)
  const [search, setSearch] = useState("")
  const typed = search.trim()
  const offerTyped = typed !== "" && !models.includes(typed)

  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (o) setSearch("")
      }}
    >
      <PopoverTrigger asChild>
        <Button
          id={id}
          type="button"
          variant="outline"
          role="combobox"
          aria-expanded={open}
          aria-invalid={invalid || undefined}
          aria-describedby={describedBy}
          disabled={disabled}
          className={cn(
            "h-8 w-full justify-between border-input px-2.5 font-normal dark:border-input pointer-coarse:h-11",
            !value && "text-muted-foreground",
          )}
        >
          <span className={cn("min-w-0 truncate", value && "font-mono text-[13px]")}>{value || "Choose or type a model id"}</span>
          <ChevronsUpDown className="text-muted-foreground" aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-(--radix-popover-trigger-width) min-w-72 p-0">
        <Command loop>
          <CommandInput value={search} onValueChange={setSearch} placeholder="Search or type a model id…" aria-label="Model id" />
          <CommandList>
            <CommandEmpty>{models.length ? "No listed model matches." : "No models listed. Type a model id."}</CommandEmpty>
            {offerTyped ? (
              <CommandGroup>
                <CommandItem
                  value={`__typed__${typed}`}
                  onSelect={() => {
                    onChange(typed)
                    setOpen(false)
                  }}
                >
                  <span className="min-w-0 truncate">
                    Use <span className="font-mono">{typed}</span>
                  </span>
                </CommandItem>
              </CommandGroup>
            ) : null}
            {models.length ? (
              <CommandGroup heading="Listed by this endpoint">
                {models.map((m) => (
                  <CommandItem
                    key={m}
                    value={m}
                    data-checked={m === value}
                    onSelect={() => {
                      onChange(m)
                      setOpen(false)
                    }}
                  >
                    <span className="min-w-0 truncate font-mono text-[13px]" title={m}>
                      {m}
                    </span>
                  </CommandItem>
                ))}
              </CommandGroup>
            ) : null}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}
