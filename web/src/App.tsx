import { HashRouter, Navigate, Route, Routes, useLocation, useParams } from "react-router-dom";
import { ErrorBoundary } from "./components/ErrorBoundary";
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
 */
function Screens() {
  const { pathname } = useLocation();
  return (
    <AppShell>
      <ErrorBoundary resetKey={pathname}>
        <Routes>
          <Route path="/" element={<Navigate to="/beranda" replace />} />
          <Route path="/beranda" element={<PlaceholderScreen title="Beranda" />} />
          <Route path="/portofolio" element={<PlaceholderScreen title="Portofolio" />} />
          <Route path="/perusahaan" element={<PlaceholderScreen title="Perusahaan" />} />
          <Route path="/perusahaan/tambah" element={<Pushed title="Tambah emiten" />} />
          <Route path="/perusahaan/:ticker" element={<CompanyPlaceholder />} />
          <Route path="/alat" element={<PlaceholderScreen title="Alat" />} />
          <Route path="/alat/bandingkan" element={<Pushed title="Bandingkan" />} />
          <Route path="/alat/kalibrasi" element={<Pushed title="Kalibrasi" />} />
          <Route path="/alat/paket" element={<Pushed title="Paket & aturan" />} />
          <Route path="*" element={<Navigate to="/beranda" replace />} />
        </Routes>
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
