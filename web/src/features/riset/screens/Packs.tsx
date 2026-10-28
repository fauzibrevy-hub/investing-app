import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Button } from "../../../components/Button";
import { cn } from "../../../lib/cn";
import { BackHeader } from "../../../shell/BackHeader";
import { reloadPacks, usePacks, useRules, useUniverse } from "../../research/api";
import { driverStatusText } from "../../research/components/gapReason";
import { ScreenFooter } from "../../research/desk/DeskFooter";
import { formatDate, formatValue } from "../../research/format";
import type { DriverStatus, MetricDirection, MetricLayer, PackDefinition, PackDefinitionMetric, PackDriver, PacksStatus, RulesResponse, UniverseItem } from "../../research/types";
import { metricLabels, readableFormula } from "../packFormula";
import { Block, Empty, FIELD, LoadError, Loading, PAGE } from "../ui";

const LAYERS: Record<MetricLayer, string> = { xbrl: "XBRL", kpi: "KPI", driver: "Pendorong" };
const DIRECTIONS: Record<MetricDirection, string> = { higher: "lebih tinggi lebih baik", lower: "lebih rendah lebih baik", neutral: "netral" };
const PERIODS: Record<string, string> = { instant: "posisi akhir periode", standalone: "kuartal", ttm: "12 bulan terakhir", average4q: "rata-rata 4 kuartal" };
const CADENCES: Record<string, string> = { quarterly: "kuartalan", annual: "tahunan" };
/** Titik status pendorong; kata selalu tertulis di sampingnya. */
const DRIVER_DOTS: Record<DriverStatus, string> = { tersedia: "bg-status-safe-fill", "belum diambil": "bg-status-warn-fill", TBD: "bg-text-3" };
const ROLE_WORDS: Record<UniverseItem["role"], string> = { held: "dipegang", manual: "manual", peer: "pembanding" };

const unitText = (unit: string) => (unit === "ccy" ? "mata uang laporan" : unit);
const num = (v: number) => formatValue(v, Number.isInteger(v) ? 0 : 1);

/** Rumus untuk lapisan XBRL, jenis isian untuk KPI, nama pendorong untuk lapisan pendorong. */
export function inputOf(m: PackDefinitionMetric): string {
  if (m.layer === "xbrl") return `${m.formula ?? "—"}${m.period ? ` · ${PERIODS[m.period] ?? m.period}` : ""}`;
  if (m.layer === "kpi") return `Isian pemilik${m.cadence ? ` · ${CADENCES[m.cadence] ?? m.cadence}` : ""}`;
  return `Pendorong ${m.driver ?? ""}`.trim();
}

/** Satu metrik, tiga baris: nama dan satuan; lapisan dan arah; kenapa penting. Rumus di balik **Rumus**: bacaan dulu, penanda di bawahnya. */
function MetricItem({ m, labels }: { m: PackDefinitionMetric; labels: ReadonlyMap<string, string> }) {
  const readable = readableFormula(m, labels);
  return (
    <li data-metric={m.id} className="flex flex-col gap-0.5 border-b border-border-hairline py-2 last:border-b-0">
      <span className="flex min-w-0 items-baseline gap-2">
        <span className="min-w-0 break-words text-body font-semibold text-text">{m.label}</span>
        <span className="shrink-0 text-caption text-text-2">{unitText(m.unit)}</span>
      </span>
      <span className="text-caption text-text-2">{`${LAYERS[m.layer]} · ${DIRECTIONS[m.direction]}`}</span>
      <span className="text-label text-text">{m.why}</span>
      <details>
        <summary className="flex min-h-11 min-w-11 w-fit cursor-pointer items-center text-caption font-bold text-accent">Rumus</summary>
        {readable ? <p className="break-words pb-1 text-caption text-text">{readable}</p> : null}
        <p className="break-words pb-2 text-caption text-text-2">{readable ? `Penanda: ${inputOf(m)}` : inputOf(m)}</p>
        {m.note ? <p className="break-words pb-2 text-caption text-text-2">{m.note}</p> : null}
      </details>
    </li>
  );
}

function DriverList({ drivers }: { drivers: PackDriver[] }) {
  return (
    <Block id="pendorong" title="Pendorong">
      {drivers.length === 0 ? (
        <Empty>Paket ini tidak memakai pendorong eksternal.</Empty>
      ) : (
        <ul>
          {drivers.map((d) => (
            <li key={d.id} className="flex min-h-11 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border-hairline py-2 text-label last:border-b-0">
              <span className="font-semibold text-text">{d.label}</span>
              <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-text">
                <span aria-hidden="true" className={cn("h-2 w-2 shrink-0 rounded-pill", DRIVER_DOTS[d.status])} />
                {driverStatusText(d.status)}
              </span>
              {d.source !== "TBD" ? <span className="text-text-2">{d.source}</span> : null}
              {d.cadence ? <span className="text-text-2">{d.cadence}</span> : null}
              {d.compare_metric ? <span className="text-text-2">{`dibandingkan dengan ${d.compare_metric}`}</span> : null}
              {d.note ? <span className="basis-full text-caption text-text-2">{d.note}</span> : null}
            </li>
          ))}
        </ul>
      )}
    </Block>
  );
}

function PackDetail({ pack, universe, labels }: { pack: PackDefinition; universe: UniverseItem[]; labels: ReadonlyMap<string, string> }) {
  const memberText = (ticker: string) => {
    const item = universe.find((u) => u.ticker === ticker);
    return item ? `${ticker} (${ROLE_WORDS[item.role]})` : ticker;
  };
  return (
    <div className="flex min-w-0 flex-col gap-6">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-title text-text">{pack.label}</h2>
        <p className="text-caption text-text-2">
          {`${pack.id} · versi ${pack.version}`}
          {pack.industry_codes.length ? ` · kode industri ${pack.industry_codes.join(", ")}` : ""}
          {pack.taxonomies.length ? ` · taksonomi ${pack.taxonomies.join(", ")}` : ""}
        </p>
        <p className="text-caption text-text-2">{pack.members.length ? `Anggota: ${pack.members.map(memberText).join(", ")}` : "Belum ada emiten pada paket ini."}</p>
      </div>
      {pack.dimensions.map((d) => (
        <section key={d.id} aria-label={`Dimensi ${d.label}`} className="flex flex-col rounded-md border border-border-hairline px-4 pb-1 pt-3">
          <h3 className="text-body-strong text-text">{d.label}</h3>
          <ul>
            {d.metrics.map((m) => (
              <MetricItem key={m.id} m={m} labels={labels} />
            ))}
          </ul>
        </section>
      ))}
      <DriverList drivers={pack.drivers} />
    </div>
  );
}

function RulesCard({ rules }: { rules: RulesResponse }) {
  const rows: Array<[string, string]> = [
    ["Tingkat diskonto (%)", rules.discount_rates.map(num).join(" · ")],
    ["Pertumbuhan akhir (%)", num(rules.terminal_growth)],
    ["Horizon DCF (tahun)", num(rules.horizon_years)],
    ["Jendela siklus (tahun)", rules.cycle_windows.map(num).join(" · ")],
    ["Minimum riwayat siklus (tahun)", num(rules.min_cycle_years)],
    ["Jendela persentil (tahun)", `${num(rules.percentile_window_years)} · minimum ${num(rules.percentile_min_years)}`],
    ["Batas laporan POJK (hari setelah akhir periode)", `Q1 +${rules.pojk_offsets.q1} · Q2 +${rules.pojk_offsets.q2} · Q3 +${rules.pojk_offsets.q3} · FY +${rules.pojk_offsets.fy}`],
    ["Versi aturan", rules.rules_version],
    ["Versi aturan turunan", rules.derive_rules_version],
  ];
  return (
    <Block id="aturan" title="Aturan">
      {rules.error ? (
        <p role="alert" className="text-label text-status-over">{`Berkas aturan bermasalah: ${rules.error}`}</p>
      ) : null}
      <dl className="grid grid-cols-1 gap-x-6 gap-y-2">
        {rows.map(([k, v]) => (
          <div key={k} className="flex flex-col">
            <dt className="text-caption text-text-2">{k}</dt>
            <dd className="num text-label text-text">{v}</dd>
          </div>
        ))}
      </dl>
      <div>
        <h3 className="text-table-head uppercase text-text-2">Riwayat perubahan</h3>
        <ul className="mt-1 flex flex-col gap-1">
          {rules.changelog.map((c) => (
            <li key={c.version} className="text-label text-text">
              <span className="font-semibold">{c.version}</span>
              <span className="text-text-2">{` · ${formatDate(c.date)}`}</span>
              <span className="block text-caption text-text-2">{c.note}</span>
            </li>
          ))}
        </ul>
      </div>
    </Block>
  );
}

function SourceCard({ status, rules }: { status: PacksStatus; rules?: RulesResponse }) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const reload = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const r = await reloadPacks();
      setMessage(r.rules.error ? { text: `${r.reloaded} paket dimuat; aturan gagal: ${r.rules.error}`, error: true } : { text: `${r.reloaded} paket dimuat, aturan ${r.rules.version}`, error: false });
    } catch {
      setMessage({ text: "Gagal memuat ulang paket", error: true });
    } finally {
      setBusy(false);
    }
  };
  const where = status.dir ? status.dir : "tertanam di aplikasi";
  return (
    <Block id="sumber" title="Sumber definisi">
      <p className="text-label text-text">Definisi paket dan aturan diedit lewat berkas, bukan di aplikasi.</p>
      <dl className="flex flex-col gap-1 text-label">
        <div className="flex flex-wrap gap-x-2">
          <dt className="text-text-2">Folder paket</dt>
          <dd className="break-all text-text">{`${where} (${status.source})`}</dd>
        </div>
        {rules ? (
          <div className="flex flex-wrap gap-x-2">
            <dt className="text-text-2">Berkas aturan</dt>
            <dd className="break-all text-text">{`${rules.file} (${rules.source})`}</dd>
          </div>
        ) : null}
        <div className="flex flex-wrap gap-x-2">
          <dt className="text-text-2">Termuat</dt>
          <dd className="num text-text">{`${status.loaded} paket${status.files ? ` dari ${status.files} berkas` : ""}`}</dd>
        </div>
      </dl>
      {status.invalid.length > 0 ? (
        <ul className="text-caption text-status-over">
          {status.invalid.map((i) => (
            <li key={i.file} className="break-words">{`${i.file}: ${i.error}`}</li>
          ))}
        </ul>
      ) : null}
      <div>
        <Button variant="sunken" loading={busy} onClick={reload}>
          Muat ulang
        </Button>
      </div>
      <p role="status" aria-live="polite" className={cn("text-caption", message?.error ? "text-status-over" : "text-text-2")}>
        {message?.text ?? ""}
      </p>
    </Block>
  );
}

/**
 * Paket & aturan `#/alat/paket?pack=` (ui-slicing.md §5): daftar paket menjadi satu pilihan di atas; paket terpilih
 * tampil sebagai satu kartu per dimensi dengan metrik tiga baris dan rumus di balik **Rumus**; pendorong dengan status
 * sumbernya; aturan di bawah; **Muat ulang** tetap. Data: `GET /packs`, `GET /rules`.
 */
export function Packs() {
  const [params, setParams] = useSearchParams();
  const packs = usePacks();
  const rules = useRules();
  const universe = useUniverse();
  const list = packs.data?.packs ?? [];
  const ordered = [...list.filter((p) => p.id !== "core"), ...list.filter((p) => p.id === "core")];
  const selected = ordered.find((p) => p.id === params.get("pack")) ?? ordered[0];

  let detail;
  if (packs.status === "error" && !packs.data) detail = <LoadError message="Gagal memuat paket" onRetry={packs.reload} />;
  else if (!packs.data) detail = <Loading label="Memuat paket…" rows={2} height={160} />;
  else if (!selected) detail = <Empty>Belum ada paket.</Empty>;
  else detail = <PackDetail pack={selected} universe={universe.data?.items ?? []} labels={metricLabels(list)} />;

  return (
    <div className={PAGE}>
      <header className="flex flex-col gap-1">
        <BackHeader />
        <h1 className="text-title text-text">Paket & aturan</h1>
      </header>
      {ordered.length ? (
        <label className="flex flex-col gap-1 text-label font-semibold text-text">
          Paket
          <select value={selected?.id ?? ""} onChange={(e) => setParams({ pack: e.target.value })} className={FIELD}>
            {ordered.map((p) => (
              <option key={p.id} value={p.id}>{`${p.label} · versi ${p.version}`}</option>
            ))}
          </select>
        </label>
      ) : null}
      {detail}
      {rules.data ? <RulesCard rules={rules.data} /> : rules.status === "error" ? <LoadError message="Gagal memuat aturan" onRetry={rules.reload} /> : <Loading label="Memuat aturan…" rows={1} height={160} />}
      {packs.data ? <SourceCard status={packs.data.status} rules={rules.data} /> : null}
      <ScreenFooter asOf={packs.data?.status.loaded_at || rules.data?.loaded_at} />
    </div>
  );
}
