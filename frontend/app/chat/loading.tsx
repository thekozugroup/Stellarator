import { Skeleton } from "@/components/ui/skeleton";

export default function ChatLoading() {
  return (
    <div className="grid h-[calc(100vh-0px)] grid-cols-1 bg-background lg:grid-cols-[260px_1fr]">
      {/* Thread list skeleton */}
      <aside className="hidden h-full border-r border-border/60 bg-sidebar/30 lg:block">
        <div className="border-b border-border/50 p-2 space-y-2">
          <Skeleton className="h-8 w-full rounded-md" />
          <Skeleton className="h-8 w-full rounded-md" />
        </div>
        <div className="p-2 space-y-1">
          {Array.from({ length: 10 }).map((_, i) => (
            <Skeleton key={i} className="h-9 w-full rounded-md" />
          ))}
        </div>
      </aside>

      {/* Welcome skeleton */}
      <section className="flex h-full flex-col">
        <div className="border-b border-border/50 px-5 py-2.5 flex items-center justify-between">
          <div className="space-y-1">
            <Skeleton className="h-4 w-40" />
            <Skeleton className="h-3 w-24" />
          </div>
          <div className="flex gap-2">
            <Skeleton className="h-7 w-28 rounded-md" />
            <Skeleton className="h-7 w-24 rounded-md" />
          </div>
        </div>

        <div className="flex flex-1 items-start justify-center pt-16 px-4">
          <div className="w-full max-w-2xl space-y-6">
            <div className="flex items-center gap-3">
              <Skeleton className="size-9 rounded-full" />
              <div className="space-y-1">
                <Skeleton className="h-4 w-40" />
                <Skeleton className="h-3 w-56" />
              </div>
            </div>
            <div className="grid grid-cols-2 gap-2">
              {Array.from({ length: 4 }).map((_, i) => (
                <Skeleton key={i} className="h-16 w-full rounded-lg" />
              ))}
            </div>
          </div>
        </div>

        <div className="border-t border-border/50 px-4 py-3">
          <Skeleton className="mx-auto h-20 w-full max-w-3xl rounded-md" />
        </div>
      </section>
    </div>
  );
}
