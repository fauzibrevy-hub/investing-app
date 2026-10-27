import { useCallback, useState } from "react";
import { HashRouter, Navigate, Route, Routes, useLocation } from "react-router-dom";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { HideAmountsProvider } from "./lib/privacy";
import { getLastLoadedAt, invalidate, useResourceGeneration } from "./lib/useResource";
import { AppShell } from "./shell/AppShell";
import { AuthGate } from "./shell/AuthGate";
import { ROUTE_TITLES, type RouteKey } from "./shell/nav";
import { PlaceholderScreen } from "./screens/PlaceholderScreen";
import { AddTransactionHost, AddTransactionProvider } from "./screens/home/AddTransaction";
import { HomeScreen } from "./screens/home/HomeScreen";
import { ReportScreen } from "./screens/report/ReportScreen";
import { PortfolioScreen } from "./screens/portfolio/PortfolioScreen";
import { ThesisScreen } from "./screens/thesis/ThesisScreen";
import { IdxTickerRedirect, ResearchRoutes } from "./features/research/routes";
import { RisetRoutes } from "./features/riset/RisetRoutes";

const SCREENS: Partial<Record<RouteKey, () => JSX.Element>> = {
  home: HomeScreen,
  report: ReportScreen,
  portfolio: PortfolioScreen,
  thesis: ThesisScreen,
  research: ResearchRoutes,
  riset: RisetRoutes,
};

const ROUTES = Object.keys(ROUTE_TITLES) as RouteKey[];

function Screens() {
  const [addOpen, setAddOpen] = useState(false);
  const openAdd = useCallback(() => setAddOpen(true), []);
  const closeAdd = useCallback(() => setAddOpen(false), []);
  // Stempel waktu sidebar mengikuti respons GET terakhir.
  useResourceGeneration();
  const { pathname } = useLocation();
  const updatedAt = getLastLoadedAt();

  return (
    <AddTransactionProvider value={openAdd}>
      <AppShell onAdd={openAdd} sidebar={{ updatedAt, status: updatedAt ? "ready" : "loading", onRefresh: () => invalidate() }}>
        <ErrorBoundary resetKey={pathname}>
        <Routes>
          <Route path="/" element={<Navigate to="/home" replace />} />
          <Route path="/idx/:ticker" element={<IdxTickerRedirect />} />
          <Route path="/idx/*" element={<Navigate to="/research" replace />} />
          <Route path="/macro/*" element={<Navigate to="/research" replace />} />
          {ROUTES.map((key) => {
            const Screen = SCREENS[key];
            return <Route key={key} path={`/${key}/*`} element={Screen ? <Screen /> : <PlaceholderScreen title={ROUTE_TITLES[key]} />} />;
          })}
          <Route path="*" element={<Navigate to="/home" replace />} />
        </Routes>
        </ErrorBoundary>
        <AddTransactionHost open={addOpen} onOpen={openAdd} onClose={closeAdd} />
      </AppShell>
    </AddTransactionProvider>
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
