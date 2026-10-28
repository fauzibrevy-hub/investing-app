import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { BottomNav } from "./BottomNav";
import { OfflineBanner } from "./OfflineBanner";
import { UpdateBanner } from "./UpdateBanner";
import { locationHash, navStore, owner, restoreScroll, TABS, tabTarget, type Tab } from "./navState";

/**
 * Rule 5 of design.md §2.4: on every location change save the scroll of the hash being left, write the new hash (and
 * whose history entry it is, for rule 8) to its owner's memory, then on the next frame scroll to the new hash's saved position (0 if none). The position is tracked
 * from scroll events, because by the time the new route has rendered the document may already be shorter.
 */
function useNavMemory(hash: string) {
  const current = useRef<{ hash: string; y: number } | null>(null);

  useEffect(() => {
    const before = window.history.scrollRestoration;
    window.history.scrollRestoration = "manual";
    const onScroll = () => {
      if (current.current) current.current.y = window.scrollY;
    };
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      window.removeEventListener("scroll", onScroll);
      window.history.scrollRestoration = before;
    };
  }, []);

  useLayoutEffect(() => {
    const prev = current.current;
    if (prev && prev.hash !== hash) navStore.saveScroll(prev.hash, prev.y);
    const idx = (window.history.state as { idx?: unknown } | null)?.idx;
    navStore.visit(hash, typeof idx === "number" ? idx : undefined);
    const target = restoreScroll(navStore, hash);
    current.current = { hash, y: target ?? window.scrollY };
    if (target === null) return;
    const frame = requestAnimationFrame(() => window.scrollTo(0, target));
    return () => cancelAnimationFrame(frame);
  }, [hash]);
}

/**
 * Mobile layout only, at every width (D13, design.md §1, §5): one `max-w-mobile` column and the bottom bar,
 * fixed and centred with the column.
 */
export function AppShell({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  const hash = locationHash(useLocation());
  const active = owner(hash);
  useNavMemory(hash);

  const targets = Object.fromEntries(TABS.map((tab) => [tab, tabTarget(navStore, tab, active)])) as Record<Tab, string>;
  const select = (tab: Tab) => {
    const target = tabTarget(navStore, tab, active);
    // Rule 7: the active tab goes to its root at the top, forgetting where the root was left.
    if (tab === active) navStore.forgetScroll(target);
    if (target === hash) window.scrollTo(0, 0);
    else navigate(target);
  };

  return (
    <>
      <main className="mx-auto min-h-dvh max-w-mobile bg-bg pb-[calc(106px+env(safe-area-inset-bottom))] pl-[var(--gutter-left)] pr-[var(--gutter-right)] pt-[calc(env(safe-area-inset-top)+12px)]">
        <UpdateBanner className="mb-4" />
        <OfflineBanner className="mb-4" />
        {children}
      </main>
      <div className="fixed inset-x-0 z-20 mx-auto w-[calc(min(100vw,480px)-30px)]" style={{ bottom: "var(--nav-bottom)" }}>
        <BottomNav active={active} targets={targets} onSelect={select} />
      </div>
    </>
  );
}
