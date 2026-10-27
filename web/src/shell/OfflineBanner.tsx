import { useEffect, useState } from "react";
import { onCacheInfo } from "../lib/api";
import { formatDataTime } from "../lib/time";
import { Banner } from "./Banner";

/** "Offline · data per <waktu>" saat respons GET datang dari cache service worker (design.md §10). */
export function OfflineBanner({ className }: { className?: string }) {
  const [cachedAt, setCachedAt] = useState<Date | null>(null);
  const [offline, setOffline] = useState(() => typeof navigator !== "undefined" && navigator.onLine === false);

  useEffect(() => {
    const off = onCacheInfo((info) => {
      if (info.fromCache) setCachedAt((prev) => (prev && info.cachedAt && prev < info.cachedAt ? prev : info.cachedAt));
      else setCachedAt(null);
    });
    const goOffline = () => setOffline(true);
    const goOnline = () => setOffline(false);
    window.addEventListener("offline", goOffline);
    window.addEventListener("online", goOnline);
    return () => {
      off();
      window.removeEventListener("offline", goOffline);
      window.removeEventListener("online", goOnline);
    };
  }, []);

  if (!cachedAt && !offline) return null;
  return <Banner className={className}>{cachedAt ? `Offline · data per ${formatDataTime(cachedAt)}` : "Offline"}</Banner>;
}
