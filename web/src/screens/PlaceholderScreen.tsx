import { Card } from "../components/Card";
import { SectionHeader } from "../components/SectionHeader";

/** Kerangka layar sampai layar paritas dibangun di P3 (plan.md L23–L28). */
export function PlaceholderScreen({ title }: { title: string }) {
  return (
    <div className="flex flex-col gap-6">
      <SectionHeader variant="screen" title={title} />
      <Card tone="sunken" status="empty" emptyMessage="Layar ini belum tersedia di versi ini." minHeight={120} />
    </div>
  );
}
