import { cn } from "@/lib/utils";

interface PageContainerProps {
  children: React.ReactNode;
  className?: string;
}

/**
 * Consistent page-level spacing wrapper: centered, max-width, 8px-grid padding.
 * Wraps the immediate content of each route page (not the layout).
 */
export function PageContainer({ children, className }: PageContainerProps) {
  return (
    <div className={cn("mx-auto w-full max-w-[1400px] px-6 py-6 space-y-6 motion-safe:animate-in motion-safe:fade-in motion-safe:duration-200", className)}>
      {children}
    </div>
  );
}
