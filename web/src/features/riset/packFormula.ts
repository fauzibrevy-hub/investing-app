import type { PackDefinition, PackDefinitionMetric } from "../research/types";

/** Fungsi rumus paket → kata bacaan; fungsi lain ditulis apa adanya. */
const FUNCTIONS: Record<string, string> = { ttm: "TTM", avg4q: "rata-rata 4 kuartal" };

/** Kunci label unsur XBRL di peta `metricLabels`, terpisah dari id metrik. */
const elementKey = (element: string) => `element:${element}`;

/** Unsur XBRL dari satu ikatan (`idx-cor:Equity`, `abs:idx-cor:Capex`); `null` untuk ikatan metrik, KPI, dan angka literal. */
const elementOf = (binding: string): string | null => {
  const bare = binding.replace(/^(?:abs|neg):/, "");
  return /^(?:metric|kpi):/.test(bare) || !/^[A-Za-z][\w-]*:[A-Za-z]/.test(bare) ? null : bare;
};

/**
 * Id metrik paket (termasuk inti) → label bacaan, untuk penanda yang menunjuk metrik lain. Metrik yang rumusnya satu
 * penanda saja (`Revenue`) juga memberi label Indonesia pada unsur XBRL yang diikat penanda itu, supaya penanda lain yang
 * memakai unsur yang sama dibaca dengan nama itu.
 */
export function metricLabels(packs: readonly PackDefinition[]): Map<string, string> {
  const labels = new Map<string, string>();
  for (const p of packs) {
    for (const d of p.dimensions) {
      for (const m of d.metrics) {
        if (!labels.has(m.id)) labels.set(m.id, m.label);
        const only = m.formula?.trim() ?? "";
        const single = /^[A-Za-z_][A-Za-z0-9_]*$/.test(only) ? m.bindings?.[only] : undefined;
        for (const b of single ?? []) {
          const element = elementOf(b);
          if (element && !labels.has(elementKey(element))) labels.set(elementKey(element), m.label);
        }
      }
    }
  }
  return labels;
}

/** `WadiahDemand` → `Wadiah demand`, `NpfGross` → `Npf gross`; singkatan huruf besar tetap. */
export function humanize(identifier: string): string {
  const words = identifier.split(/(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])|_/).filter(Boolean);
  return words.map((w, i) => (w.length > 1 && w === w.toUpperCase() ? w : i === 0 ? w[0].toUpperCase() + w.slice(1) : w.toLowerCase())).join(" ");
}

/**
 * Rumus metrik XBRL dibaca manusia: penanda yang terikat ke metrik atau KPI lain, atau ke unsur XBRL yang sudah diberi
 * label oleh metrik lain, memakai label itu; sisanya ditulis kata demi kata dari nama penanda. `null` bila metrik tidak punya rumus. Rumus bertanda tetap ada
 * untuk yang ingin memeriksa.
 */
export function readableFormula(m: Pick<PackDefinitionMetric, "formula" | "bindings">, labels: ReadonlyMap<string, string>): string | null {
  if (!m.formula) return null;
  const text = m.formula.replace(/([A-Za-z_][A-Za-z0-9_]*)(\s*\()?/g, (_all, name: string, call: string | undefined) => {
    if (call) return `${FUNCTIONS[name] ?? name}(`;
    const bound = m.bindings?.[name] ?? [];
    const target = bound.map((b) => /^(?:metric|kpi):(.+)$/.exec(b)?.[1]).find((id) => id && labels.has(id));
    if (target) return labels.get(target) as string;
    // Unsur XBRL yang sudah diberi label Indonesia oleh metrik lain; selebihnya kata dari nama penanda.
    const element = bound.map((b) => elementOf(b)).find((e) => e && labels.has(elementKey(e)));
    return element ? (labels.get(elementKey(element)) as string) : humanize(name);
  });
  return text.replace(/\s+/g, " ").trim();
}
