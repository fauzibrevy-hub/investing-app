import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { useLocation } from "react-router-dom";
import { useIsDesktop } from "../lib/useMediaQuery";
import { BottomNav } from "./BottomNav";
import { DesktopSidebar } from "./DesktopSidebar";
import { DetailPanel } from "./DetailPanel";
import { Fab } from "./Fab";
import { OfflineBanner } from "./OfflineBanner";
import { UpdateBanner } from "./UpdateBanner";
import { ROUTE_TITLES, type RouteKey } from "./nav";

interface DetailContent {
  title: string;
  content: ReactNode;
  wide?: boolean;
}

interface ShellApi {
  /** Desktop: buka panel detail 380 px. Mobile: layar P3 menavigasi ke layar baru sendiri. */
  openDetail: (detail: DetailContent) => void;
  closeDetail: () => void;
  isDesktop: boolean;
}

const ShellContext = createContext<ShellApi>({ openDetail: () => {}, closeDetail: () => {}, isDesktop: false });
export const useShell = () => useContext(ShellContext);

export function routeKeyFromPath(pathname: string): RouteKey | null {
  const key = pathname.split("/")[1] as RouteKey;
  return key in ROUTE_TITLES ? key : null;
}

interface AppShellProps {
  children: ReactNode;
  /** Aksi FAB / tombol "Catat transaksi" (sheet dibangun di P3). */
  onAdd?: () => void;
  sidebar?: { updatedAt?: Date | null; status?: "ready" | "loading" | "error"; onRefresh?: () => void; refreshing?: boolean };
}

/**
 * Dua layout berbeda (kontrak §2.4): < 1024 px kolom maks 480 px + bottom nav + FAB;
 * ≥ 1024 px sidebar 232 px + konten (padding 40, maks 1040) + panel detail 380 px.
 */
export function AppShell({ children, onAdd, sidebar }: AppShellProps) {
  const isDesktop = useIsDesktop();
  const active = routeKeyFromPath(useLocation().pathname);
  const [detail, setDetail] = useState<DetailContent | null>(null);
  const closeDetail = useCallback(() => setDetail(null), []);
  const api = useMemo<ShellApi>(() => ({ openDetail: setDetail, closeDetail, isDesktop }), [closeDetail, isDesktop]);

  if (isDesktop) {
    return (
      <ShellContext.Provider value={api}>
        <div className="flex h-dvh overflow-hidden bg-bg">
          <DesktopSidebar
            active={active}
            className="shrink-0"
            updatedAt={sidebar?.updatedAt}
            dataStatus={sidebar?.status}
            onRefresh={sidebar?.onRefresh}
            refreshing={sidebar?.refreshing}
          />
          {/* `relative`: elemen `.sr-only` (absolute) di dalam konten ikut terkurung di kotak gulir `main`, bukan memanjangkan dokumen. */}
          <main className="relative min-w-0 flex-1 overflow-y-auto p-10">
            <div className="mx-auto flex max-w-content flex-col gap-6">
              <UpdateBanner />
              <OfflineBanner />
              {children}
            </div>
          </main>
          {detail ? (
            <DetailPanel open title={detail.title} wide={detail.wide} onClose={closeDetail} className="xl:static xl:shrink-0">
              {detail.content}
            </DetailPanel>
          ) : null}
        </div>
      </ShellContext.Provider>
    );
  }

  return (
    <ShellContext.Provider value={api}>
      <main className="mx-auto min-h-dvh max-w-mobile bg-bg pb-[calc(106px+env(safe-area-inset-bottom))] pl-[var(--gutter-left)] pr-[var(--gutter-right)] pt-[calc(env(safe-area-inset-top)+12px)]">
        <UpdateBanner className="mb-4" />
        <OfflineBanner className="mb-4" />
        {children}
      </main>
      <div className="fixed inset-x-0 z-20 mx-auto w-[calc(min(100vw,480px)-30px)]" style={{ bottom: "var(--nav-bottom)" }}>
        <BottomNav active={active} />
        {/* Pusat FAB sejajar tepi atas nav (menonjol 26 px). */}
        <Fab onClick={onAdd} className="absolute left-1/2 top-[-26px] -translate-x-1/2" />
      </div>
    </ShellContext.Provider>
  );
}
