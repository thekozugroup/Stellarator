import { cn } from "@/lib/utils"

/**
 * The Stellarator mark: one bold strand that winds twice around a ring and crosses itself five
 * times, passing over and under in turn, so its outline is a ring with five lobes. It comes from
 * the earlier mark's two coils braided around a ring. Drawn in the current text color, as five
 * identical pieces turned 72 degrees apart. desktop/icon-src/make_icon.py draws the same mark for
 * the app icon, and `python3 make_icon.py --logo` prints PIECE.
 */
const PIECE =
  "M20.24 15.21C20.44 15.92 20.57 16.71 20.52 17.53C20.48 18.34 20.27 19.18 19.83 19.90C19.40 20.63 18.74 21.24 17.97 21.60C17.19 21.95 16.32 22.06 15.50 21.98C14.68 21.89 13.89 21.61 13.21 21.25C12.53 20.89 11.94 20.45 11.45 20.02C10.97 19.58 10.58 19.16 10.26 18.81C9.93 18.45 9.68 18.16 9.46 17.93C9.24 17.70 9.05 17.52 8.86 17.35C8.67 17.19 8.48 17.04 8.29 16.90C8.09 16.76 7.88 16.64 7.65 16.51C7.42 16.38 7.16 16.25 6.83 16.10L7.93 13.75C8.29 13.91 8.61 14.07 8.92 14.24C9.22 14.40 9.49 14.58 9.75 14.76C10.01 14.94 10.26 15.13 10.51 15.34C10.76 15.56 11.02 15.79 11.27 16.06C11.53 16.33 11.79 16.62 12.07 16.94C12.35 17.25 12.65 17.58 13.00 17.91C13.35 18.23 13.74 18.55 14.17 18.81C14.59 19.06 15.04 19.26 15.47 19.34C15.90 19.43 16.30 19.42 16.63 19.33C16.95 19.24 17.21 19.07 17.42 18.82C17.63 18.57 17.79 18.24 17.87 17.83C17.95 17.42 17.95 16.94 17.86 16.45Z"

export function Logo({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      className={cn("size-5 shrink-0", className)}
    >
      {[0, 72, 144, 216, 288].map((angle) => (
        <path key={angle} d={PIECE} transform={`rotate(${angle} 12 12)`} />
      ))}
    </svg>
  )
}
