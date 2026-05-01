import { Skeleton } from "@/components/ui/skeleton";

export default function CompareLoading() {
  return (
    <div className="space-y-6 p-6">
      {/* Chart card skeleton */}
      <div className="rounded-lg border border-border/60 bg-card p-4 space-y-3">
        <div className="flex items-center justify-between">
          <Skeleton className="h-4 w-32" />
          <Skeleton className="h-7 w-24" />
        </div>
        <Skeleton className="h-72 w-full" />
      </div>

      {/* Diff table skeleton */}
      <div className="rounded-lg border border-border/60 bg-card overflow-hidden">
        <div className="border-b border-border/60 px-4 py-2.5">
          <Skeleton className="h-4 w-36" />
        </div>
        <div className="divide-y divide-border/40">
          {Array.from({ length: 8 }).map((_, i) => (
            <div key={i} className="flex items-center gap-4 px-4 py-2">
              <Skeleton className="h-3 w-28" />
              <Skeleton className="h-3 flex-1" />
              <Skeleton className="h-3 flex-1" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
