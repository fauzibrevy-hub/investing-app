import { cn } from "../lib/cn";
import { MOBILE_TABS, type NavEntry, type RouteKey } from "./nav";

interface BottomNavProps {
  active: RouteKey | null;
  items?: NavEntry[];
  className?: string;
}

/**
 * Bottom nav mobile (design.md §1.8, §8): tinggi 56 px, outline 1,5 px `border-strong`, `radius-pill`,
 * 4 item (ikon 24 px + label `type-axis`), slot tengah 72 px untuk FAB. Posisi fixed diatur shell.
 * Semua ukuran dalam px dan kolom `minmax(0, 1fr)`: font akar 200% atau zoom 200% tidak mendorong tab ke luar layar;
 * di bawah 300 px lebar layar label disembunyikan (tetap jadi nama aksesibel) dan slot FAB menyempit sampai 52 px.
 */
export function BottomNav({ active, items = MOBILE_TABS, className }: BottomNavProps) {
  const left = items.slice(0, 2);
  const right = items.slice(2);
  const renderItem = (item: NavEntry) => {
    const isActive = active !== null && item.routes.includes(active);
    const Icon = item.icon;
    return (
      <a
        key={item.key}
        href={item.href}
        aria-current={isActive ? "page" : undefined}
        className={cn(
          "flex h-full min-w-0 flex-col items-center justify-center gap-1 rounded-pill px-0.5 text-axis",
          isActive ? "font-bold text-accent" : "text-nav-inactive",
        )}
      >
        <Icon size={24} strokeWidth={1.75} aria-hidden="true" />
        <span className="max-w-full truncate max-[299px]:sr-only">{item.label}</span>
      </a>
    );
  };
  return (
    <nav
      aria-label="Navigasi utama"
      className={cn(
        "grid h-[56px] grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(52px,72px)_minmax(0,1fr)_minmax(0,1fr)] items-stretch rounded-pill border-[1.5px] border-border-strong bg-bg shadow-float",
        className,
      )}
    >
      {left.map(renderItem)}
      <span aria-hidden="true" />
      {right.map(renderItem)}
    </nav>
  );
}
