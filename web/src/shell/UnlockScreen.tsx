import { useEffect, useState, type FormEvent } from "react";
import { Lock } from "lucide-react";
import { Button } from "../components/Button";
import { FormField } from "../components/FormField";
import { ApiError, checkSandbox, createSession } from "../lib/api";
import { cn } from "../lib/cn";

interface UnlockScreenProps {
  /** Default `createSession` (POST /api/session). Bisa diganti di katalog. */
  onUnlock?: (token: string) => Promise<void>;
  onUnlocked?: () => void;
  /** Default `checkSandbox` (GET /api/health, D7): true menampilkan baris uji coba di bawah field. */
  loadSandbox?: () => Promise<boolean>;
  /** Pesan awal, mis. dari sesi yang kedaluwarsa. */
  notice?: string;
  /** Katalog: tampil di dalam bingkai, bukan setinggi layar. */
  embedded?: boolean;
  /** Katalog: state awal error/loading tanpa interaksi. */
  initialError?: string;
  initialBusy?: boolean;
}

function messageFor(err: unknown): string {
  if (err instanceof ApiError) {
    if (err.kind === "unauthorized" || err.kind === "forbidden" || err.kind === "validation") return "Token salah. Periksa lagi lalu coba ulang.";
    if (err.kind === "network" || err.kind === "timeout") return "Server tidak bisa dihubungi. Periksa koneksi lalu coba lagi.";
  }
  return "Gagal membuka kunci. Coba lagi.";
}

/**
 * Layar "Buka kunci" (prd.md §5, design.md §3.6): token diminta sekali lalu ditukar dengan cookie sesi.
 * Token hanya ada di state input ini dan dikosongkan segera setelah dikirim.
 * Selama uji coba berdampingan (backend `INVESTING_SANDBOX=1`) satu baris di bawah field mengatakannya.
 */
export function UnlockScreen({
  onUnlock = createSession,
  onUnlocked,
  loadSandbox = checkSandbox,
  notice,
  embedded,
  initialError,
  initialBusy = false,
}: UnlockScreenProps) {
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | undefined>(initialError);
  const [busy, setBusy] = useState(initialBusy);
  const [sandbox, setSandbox] = useState(false);

  useEffect(() => {
    let live = true;
    void loadSandbox().then((on) => {
      if (live) setSandbox(on);
    });
    return () => {
      live = false;
    };
  }, [loadSandbox]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (busy) return;
    const value = token.trim();
    if (!value) {
      setError("Masukkan token akses.");
      return;
    }
    setBusy(true);
    setError(undefined);
    setToken("");
    try {
      await onUnlock(value);
      onUnlocked?.();
    } catch (err) {
      setError(messageFor(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main
      className={cn(
        "mx-auto flex max-w-[400px] flex-col justify-center px-4.5",
        embedded ? "py-8" : "min-h-dvh pb-[env(safe-area-inset-bottom)] pt-[calc(env(safe-area-inset-top)+12px)]",
      )}
    >
      <span className="mb-6 inline-flex h-12 w-12 items-center justify-center rounded-pill bg-surface-icon text-accent" aria-hidden="true">
        <Lock size={24} strokeWidth={1.75} />
      </span>
      <h1 className="text-title text-text">Buka kunci Investasi</h1>
      <p className="mt-2 text-label text-text-2">Masukkan token akses sekali. Token tidak disimpan di perangkat ini; sesi berlaku 30 hari.</p>
      {notice ? <p className="mt-4 rounded-sm bg-surface-sunken px-3 py-2.5 text-label text-text">{notice}</p> : null}
      <form className="mt-6 flex flex-col gap-6" onSubmit={submit} noValidate>
        <FormField
          label="Token akses"
          type="password"
          name="token"
          autoComplete="current-password"
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          value={token}
          onChange={(e) => setToken(e.target.value)}
          error={error}
        />
        {sandbox ? <p className="-mt-3 text-label text-text-2">Uji coba: perubahan di sini tidak dibawa ke data asli.</p> : null}
        <Button type="submit" variant="primary" fullWidth loading={busy}>
          Buka kunci
        </Button>
      </form>
    </main>
  );
}
