import { cn } from "../lib/cn";

interface SkeletonProps {
  width?: number | string;
  height?: number | string;
  /** Lingkaran (ikon) alih-alih `radius-sm`. */
  circle?: boolean;
  className?: string;
}

/** Skeleton blok `surface-sunken`, `radius-sm`, pulse opacity tanpa shimmer gradient (design.md §8). */
export function Skeleton({ width, height, circle, className }: SkeletonProps) {
  return (
    <span
      aria-hidden="true"
      data-skeleton=""
      className={cn("block animate-skeleton bg-surface-sunken", circle ? "rounded-pill" : "rounded-sm", className)}
      style={{ width, height }}
    />
  );
}
