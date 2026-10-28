import { ChevronLeft } from "lucide-react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { TAB_LABELS } from "./nav";
import { backTarget, locationHash, navStore, owner, TAB_ROOTS } from "./navState";

/**
 * Back control of a pushed screen (design.md §2.1, §2.4 rules 8–9, §5): "‹ <owner tab>", 44 px, accessible name
 * "Kembali ke <owner tab>". Goes back one history entry when that entry has the same owner tab, else replaces this
 * entry with the owner tab's stored route (its root if none).
 */
export function BackHeader() {
  const location = useLocation();
  const navigate = useNavigate();
  const tab = owner(locationHash(location));
  if (!tab) return null;
  const label = TAB_LABELS[tab];
  return (
    <Link
      to={TAB_ROOTS[tab]}
      aria-label={`Kembali ke ${label}`}
      onClick={(e) => {
        if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
        e.preventDefault();
        const target = backTarget(navStore, tab, window.history.state);
        if (target === -1) navigate(-1);
        else navigate(target, { replace: true });
      }}
      className="-ml-2 inline-flex h-11 w-fit items-center gap-1 pl-1 pr-3 text-label font-bold text-accent"
    >
      <ChevronLeft aria-hidden="true" size={20} strokeWidth={1.75} />
      {label}
    </Link>
  );
}
