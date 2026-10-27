import { useEffect, useState } from "react";
import { applyUpdate, onUpdateAvailable } from "../pwa/register";
import { Banner } from "./Banner";

export function UpdateBanner({ className }: { className?: string }) {
  const [available, setAvailable] = useState(false);
  useEffect(() => onUpdateAvailable(setAvailable), []);
  if (!available) return null;
  return (
    <Banner className={className} action={{ label: "Muat ulang", onClick: applyUpdate }}>
      Versi baru tersedia
    </Banner>
  );
}
