import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Spinner } from "../components/Spinner";
import { ApiError, checkSession, onUnauthorized } from "../lib/api";
import { BottomNav } from "./BottomNav";
import { owner } from "./navState";
import { UnlockScreen } from "./UnlockScreen";

type GateState = "checking" | "locked" | "expired" | "unlocked";

/**
 * Kerangka selama sesi diperiksa (bisa sampai 6 detik di jaringan setengah mati): navigasi sudah tampil dan hanya isinya
 * yang menunggu, jadi layar tidak kosong. Layout mobile saja (D13); tanpa data; navigasi hanya mengganti hash.
 */
function CheckingShell() {
  const active = owner(window.location.hash.replace(/^#/, ""));
  const waiting = (
    <div className="flex min-h-[50dvh] items-center justify-center text-accent" role="status" aria-busy="true" aria-label="Memeriksa sesi">
      <Spinner size={24} />
    </div>
  );
  return (
    <>
      <main className="mx-auto min-h-dvh max-w-mobile bg-bg pb-[calc(106px+env(safe-area-inset-bottom))] pl-[var(--gutter-left)] pr-[var(--gutter-right)] pt-[calc(env(safe-area-inset-top)+12px)]">{waiting}</main>
      <div className="fixed inset-x-0 z-20 mx-auto w-[calc(min(100vw,480px)-30px)]" style={{ bottom: "var(--nav-bottom)" }}>
        <BottomNav active={active} />
      </div>
    </>
  );
}

/**
 * Menampilkan layar buka kunci jika sesi tidak valid. Saat server tidak terjangkau (offline),
 * app tetap dibuka agar tampilan baca dari cache service worker bisa dipakai (prd.md §7);
 * 401 berikutnya akan mengunci lagi.
 */
export function AuthGate({ children }: { children: ReactNode }) {
  const [state, setState] = useState<GateState>("checking");

  const check = useCallback(async () => {
    try {
      const result = await checkSession();
      setState(result === "valid" ? "unlocked" : "locked");
    } catch (err) {
      if (err instanceof ApiError && (err.kind === "network" || err.kind === "timeout" || err.kind === "server")) setState("unlocked");
      else setState("locked");
    }
  }, []);

  useEffect(() => {
    void check();
    return onUnauthorized(() => setState((s) => (s === "unlocked" ? "expired" : s === "checking" ? s : "locked")));
  }, [check]);

  if (state === "checking") return <CheckingShell />;
  if (state === "locked" || state === "expired") {
    return <UnlockScreen notice={state === "expired" ? "Sesi berakhir. Buka kunci lagi untuk melanjutkan." : undefined} onUnlocked={() => setState("unlocked")} />;
  }
  return <>{children}</>;
}
