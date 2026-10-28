import { HashRouter, Navigate, Route, Routes, useLocation, useParams } from "react-router-dom";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Alat } from "./features/alat/Alat";
import { Beranda } from "./features/beranda/Beranda";
import { Calibration } from "./features/riset/screens/Calibration";
import { Compare } from "./features/riset/screens/Compare";
import { Packs } from "./features/riset/screens/Packs";
import { HideAmountsProvider } from "./lib/privacy";
import { AppShell } from "./shell/AppShell";
import { AuthGate } from "./shell/AuthGate";
import { BackHeader } from "./shell/BackHeader";
import { PlaceholderScreen } from "./screens/PlaceholderScreen";

/** A pushed screen (design.md §2.1): back control to its owner tab, then the screen. */
function Pushed({ title }: { title: string }) {
  return (
    <div className="flex flex-col gap-2">
      <BackHeader />
      <PlaceholderScreen title={title} />
    </div>
  );
}

function CompanyPlaceholder() {
  const { ticker = "" } = useParams();
  return <Pushed title={ticker.toUpperCase()} />;
}

/**
 * The one route table (design.md §2.2). Each L3 lane replaces only its own lines (plan.md §3.3).
 * `?from=<tab>` on a pushed route is read by the shell (design.md §2.4), not here.
 * Beranda and Alat (list and the three tools) are real screens; the three tools draw their own `BackHeader`.
 * Exported for the guard tests, which mount it on a `MemoryRouter`.
 */
export function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/beranda" replace />} />
      <Route path="/beranda" element={<Beranda />} />
      <Route path="/portofolio" element={<PlaceholderScreen title="Portofolio" />} />
      <Route path="/perusahaan" element={<PlaceholderScreen title="Perusahaan" />} />
      <Route path="/perusahaan/tambah" element={<Pushed title="Tambah emiten" />} />
      <Route path="/perusahaan/:ticker" element={<CompanyPlaceholder />} />
      <Route path="/alat" element={<Alat />} />
      <Route path="/alat/bandingkan" element={<Compare />} />
      <Route path="/alat/kalibrasi" element={<Calibration />} />
      <Route path="/alat/paket" element={<Packs />} />
      <Route path="*" element={<Navigate to="/beranda" replace />} />
    </Routes>
  );
}

function Screens() {
  const { pathname } = useLocation();
  return (
    <AppShell>
      <ErrorBoundary resetKey={pathname}>
        <AppRoutes />
      </ErrorBoundary>
    </AppShell>
  );
}

export function App() {
  return (
    <HideAmountsProvider>
      <AuthGate>
        <HashRouter>
          <Screens />
        </HashRouter>
      </AuthGate>
    </HideAmountsProvider>
  );
}
