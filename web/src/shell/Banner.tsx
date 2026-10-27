import type { ReactNode } from "react";
import { cn } from "../lib/cn";

interface BannerProps {
  children: ReactNode;
  action?: { label: string; onClick: () => void };
  className?: string;
}

/** Banner status (offline, versi baru). Latar `surface-inverse`, tanpa gradient/glow. */
export function Banner({ children, action, className }: BannerProps) {
  return (
    <div role="status" className={cn("flex min-h-11 items-center justify-between gap-3 rounded-sm bg-surface-inverse pl-3 text-label text-white", action ? "pr-1" : "pr-3", className)}>
      <span className="min-w-0 py-2">{children}</span>
      {action ? (
        <button type="button" onClick={action.onClick} className="on-accent inline-flex min-h-11 shrink-0 items-center px-3 font-bold">
          {action.label}
        </button>
      ) : null}
    </div>
  );
}
