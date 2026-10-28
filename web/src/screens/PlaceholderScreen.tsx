import { Card } from "../components/Card";
import { SectionHeader } from "../components/SectionHeader";

/** Kerangka layar sampai layarnya dibawa di L3 (plan.md §3.3). */
export function PlaceholderScreen({ title }: { title: string }) {
  return (
    <div className="flex flex-col gap-6">
      <SectionHeader variant="screen" title={title} />
      <Card tone="sunken" status="empty" emptyMessage="Layar ini belum tersedia di versi ini." minHeight={120} />
    </div>
  );
}
