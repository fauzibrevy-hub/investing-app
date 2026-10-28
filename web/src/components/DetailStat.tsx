import { StatRow } from "./StatRow";

/** Label–nilai horizontal untuk panel detail (re-export agar layar lain seragam). */
export function DetailStat({ label, value, valueClassName }: { label: string; value: string; valueClassName?: string }) {
  return <StatRow orientation="horizontal" label={label} value={value} valueClassName={valueClassName} className="min-h-11 border-b border-border-hairline" />;
}
