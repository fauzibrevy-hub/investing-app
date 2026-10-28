import type { MouseEvent } from "react";
import { cn } from "../lib/cn";
import { TABS_NAV, type NavEntry } from "./nav";
import type { Tab } from "./navState";

interface BottomNavProps {
  /** Owner tab of the current route (design.md §2.4 rule 3). */
  active: Tab | null;
  items?: NavEntry[];
  /** Route each tab links to (without `#`); default its root. */
  targets?: Partial<Record<Tab, string>>;
  /** Plain left click on a tab; the shell navigates itself (rules 6, 7). Without it the link only changes the hash. */
  onSelect?: (tab: Tab, event: MouseEvent<HTMLAnchorElement>) => void;
  className?: string;
}

/**
 * Bottom nav (design.md §2.1, §5): 56 px tall, 1.5 px `border-strong` outline, `radius-pill`, four equal slots
 * (24 px icon + `type-axis` label), no FAB slot. Fixed position is the shell's job.
 * Everything in px and `minmax(0, 1fr)` columns: 200 % root font or zoom never pushes a tab off screen;
 * below 300 px of screen width the labels hide (they stay the accessible name).
 */
export function BottomNav({ active, items = TABS_NAV, targets, onSelect, className }: BottomNavProps) {
  return (
    <nav
      aria-label="Navigasi utama"
      className={cn("grid h-[56px] grid-cols-4 items-stretch rounded-pill border-[1.5px] border-border-strong bg-bg shadow-float", className)}
    >
      {items.map((item) => {
        const isActive = item.key === active;
        const Icon = item.icon;
        return (
          <a
            key={item.key}
            href={`#${targets?.[item.key] ?? item.root}`}
            aria-current={isActive ? "page" : undefined}
            onClick={(e) => {
              if (!onSelect || e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
              e.preventDefault();
              onSelect(item.key, e);
            }}
            className={cn(
              "flex h-full min-w-0 flex-col items-center justify-center gap-1 rounded-pill px-0.5 text-axis",
              isActive ? "font-bold text-accent" : "text-nav-inactive",
            )}
          >
            <Icon size={24} strokeWidth={1.75} aria-hidden="true" />
            <span className="max-w-full truncate max-[299px]:sr-only">{item.label}</span>
          </a>
        );
      })}
    </nav>
  );
}
