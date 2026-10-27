import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { registerServiceWorker } from "./pwa/register";
import "./styles/index.css";

const root = createRoot(document.getElementById("root")!);

// Katalog komponen hanya ada di `npm run dev`; di build produksi cabang ini dibuang (import.meta.env.DEV = false).
if (import.meta.env.DEV && window.location.pathname.replace(/\/+$/, "") === "/__components") {
  void import("./catalog/Catalog").then(({ Catalog }) =>
    root.render(
      <StrictMode>
        <Catalog />
      </StrictMode>,
    ),
  );
} else {
  root.render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
  if (import.meta.env.PROD) registerServiceWorker();
}
