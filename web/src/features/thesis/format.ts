import { MONTHS_ID_SHORT } from "../../lib/time";

/** "2026-10-12…" → "12 Okt" (finance-app `ThesisScreen.tsx` `shortDate`). */
export function shortDate(iso: string | null): string | null {
  const m = iso ? /^(\d{4})-(\d{2})-(\d{2})/.exec(iso) : null;
  return m ? `${Number(m[3])} ${MONTHS_ID_SHORT[Number(m[2]) - 1]}` : null;
}
