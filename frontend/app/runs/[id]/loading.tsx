import { Skeleton } from "@/components/ui/skeleton";

export default function RunDetailLoading() {
  return (
    <div className="flex h-full gap-0">
      {/* Aside skeleton */}
      <aside className="w-64 shrink-0 border-r border-border/60 bg-sidebar/30 p-4 space-y-4">
        <Skeleton className="h-5 w-36" />
        <Skeleton className="h-4 w-24" />
        <div className="space-y-2 pt-2">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="flex justify-between">
              <Skeleton className="h-3 w-20" />
              <Skeleton className="h-3 w-16" />
            </div>
          ))}
        </div>
        <Skeleton className="h-px w-full" />
        <div className="space-y-2">
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} className="h-8 w-full" />
          ))}
        </div>
      </aside>

      {/* Main area: tabs skeleton */}
      <div className="flex-1 min-w-0 p-6 space-y-4">
        {/* Tab bar */}
        <div className="flex gap-2 border-b border-border/60 pb-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-8 w-20 rounded" />
          ))}
        </div>
        {/* Chart */}
        <Skeleton className="h-64 w-full rounded-lg" />
        {/* Metrics table */}
        <div className="space-y-2">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="flex gap-4">
              <Skeleton className="h-3 w-24" />
              <Skeleton className="h-3 flex-1" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
