import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { Route, Routes, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { Plus, Trash2, X } from "lucide-react";
import { Button } from "../../components/Button";
import { DataTable, type Column } from "../../components/DataTable";
import { FormField } from "../../components/FormField";
import { SectionHeader } from "../../components/SectionHeader";
import { Sheet } from "../../components/Sheet";
import { Skeleton } from "../../components/Skeleton";
import { ListState } from "../../components/TransactionRow";
import { api, ApiError, MESSAGES } from "../../lib/api";
import { cn } from "../../lib/cn";
import { MONTHS_ID_SHORT } from "../../lib/finance";
import { invalidate, useResource } from "../../lib/useResource";
import { useShell } from "../../shell/AppShell";
import { InvestTabs } from "../portfolio/PortfolioScreen";
import {
  SCENARIOS,
  STORY_STATUSES,
  THESIS_TYPES,
  type Draft,
  type DraftInput,
  type DraftsResponse,
  type ThesisDetail,
  type ThesisSummary,
  type VaultDraft,
} from "./types";

type Kind = "dossier" | "draft" | "vault";
type Filter = "all" | "dossier" | "draft";

interface Item {
  key: string;
  kind: Kind;
  ticker: string;
  title: string;
  story: string | null;
  updated: string | null;
  href: string;
}

const KIND_LABEL: Record<Kind, string> = { dossier: "Dossier", draft: "Draft", vault: "Draft vault" };

/** "2026-10-12…" → "12 Okt". */
export function shortDate(iso: string | null): string | null {
  const m = iso ? /^(\d{4})-(\d{2})-(\d{2})/.exec(iso) : null;
  return m ? `${Number(m[3])} ${MONTHS_ID_SHORT[Number(m[2]) - 1]}` : null;
}

function useThesisItems() {
  const summary = useResource<ThesisSummary>("/api/thesis/summary");
  const drafts = useResource<DraftsResponse>("/api/thesis/drafts");
  const items = useMemo<Item[]>(() => {
    const out: Item[] = [];
    for (const d of drafts.data?.drafts ?? [])
      out.push({ key: `draft:${d.id}`, kind: "draft", ticker: d.ticker, title: d.title || d.thesis_statement, story: d.story_status, updated: d.updated_at, href: `#/thesis/draft/${encodeURIComponent(d.id)}` });
    for (const i of summary.data?.issuers ?? [])
      out.push({ key: `dossier:${i.ticker}`, kind: "dossier", ticker: i.ticker, title: i.verdict ?? i.company ?? "—", story: i.narrative_status, updated: i.review.last_date, href: `#/thesis/${i.ticker}` });
    for (const v of drafts.data?.vault_drafts ?? [])
      out.push({ key: `vault:${v.id}`, kind: "vault", ticker: "Vault", title: v.title, story: v.status_lifecycle || null, updated: null, href: `#/thesis/draft/${encodeURIComponent(v.id)}` });
    return out;
  }, [summary.data, drafts.data]);
  // Satu sumber gagal tidak mengosongkan yang lain (prd.md §7): error hanya jika keduanya gagal.
  const status: "loading" | "error" | "ready" =
    summary.status === "loading" || drafts.status === "loading" ? "loading" : summary.status === "error" && drafts.status === "error" ? "error" : "ready";
  return { summary, drafts, items, status, reload: () => (summary.reload(), drafts.reload()) };
}

const matches = (f: Filter, k: Kind) => f === "all" || (f === "dossier" ? k === "dossier" : k !== "dossier");

export function ThesisScreen() {
  return (
    <Routes>
      <Route path="new" element={<EditorScreen />} />
      <Route path="draft/:id" element={<DraftScreen />} />
      <Route path=":ticker" element={<DossierScreen />} />
      <Route path="*" element={<ThesisList />} />
    </Routes>
  );
}

function useFilter(): [Filter, (f: Filter) => void] {
  const [params, setParams] = useSearchParams();
  const f = params.get("jenis");
  return [f === "dossier" || f === "draft" ? f : "all", (v) => setParams(v === "all" ? {} : { jenis: v }, { replace: true })];
}

function FilterChips({ value, onChange }: { value: Filter; onChange: (f: Filter) => void }) {
  const opts: [Filter, string][] = [
    ["all", "Semua"],
    ["dossier", "Dossier"],
    ["draft", "Draft"],
  ];
  return (
    <div className="hscroll gap-2" role="group" aria-label="Filter tesis">
      {opts.map(([v, l]) => (
        <Button key={v} variant="chip" selected={value === v} onClick={() => onChange(v)}>
          {l}
        </Button>
      ))}
    </div>
  );
}

/** Kesalahan baca vault (mis. berkas iCloud belum terunduh) ditampilkan apa adanya, bukan disembunyikan. */
function SourceNotice({ t }: { t: ReturnType<typeof useThesisItems> }) {
  if (t.summary.status === "error") return <p className="text-caption font-semibold text-status-over">Dossier vault gagal dimuat</p>;
  if (t.summary.data && !t.summary.data.root_ok) return <p className="text-caption text-text-2">Folder dossier di vault tidak ditemukan</p>;
  if (t.drafts.status === "error") return <p className="text-caption font-semibold text-status-over">Draft gagal dimuat</p>;
  return null;
}

function ThesisList() {
  const { isDesktop } = useShell();
  return isDesktop ? <ThesisDesktop /> : <ThesisMobile />;
}

function ThesisMobile() {
  const t = useThesisItems();
  const [filter, setFilter] = useFilter();
  const list = t.items.filter((i) => matches(filter, i.kind));
  return (
    <div className="flex flex-col">
      <SectionHeader variant="screen" title="Investasi" />
      <InvestTabs active="thesis" />
      <div className="mt-4">
        <FilterChips value={filter} onChange={setFilter} />
      </div>
      <div className="mt-2">
        <SourceNotice t={t} />
      </div>
      <div className="mt-3 flex flex-col gap-card-row">
        {t.status === "loading" ? (
          Array.from({ length: 3 }, (_, i) => <Skeleton key={i} height={92} className="rounded-md" />)
        ) : t.status === "error" ? (
          <ListState status="error" onRetry={t.reload} />
        ) : list.length === 0 ? (
          <ListState status="empty" message={filter === "dossier" ? "Belum ada dossier tesis di vault" : filter === "draft" ? "Belum ada draft tesis" : "Belum ada tesis"} />
        ) : (
          list.map((i) => <ThesisCard key={i.key} item={i} />)
        )}
      </div>
      <div className="sticky bottom-[calc(var(--nav-bottom)+90px)] mt-6">
        <Button fullWidth icon={<Plus />} onClick={() => (window.location.hash = "#/thesis/new")}>
          Tulis draft tesis
        </Button>
      </div>
    </div>
  );
}

function ThesisCard({ item }: { item: Item }) {
  const meta = [item.story ? `Story: ${item.story}` : null, item.updated ? `diperbarui ${shortDate(item.updated) ?? item.updated}` : null].filter(Boolean).join(" · ");
  return (
    <a href={item.href} className="block rounded-md border border-border-hairline bg-bg p-4 active:opacity-[0.85]">
      <span className="flex min-h-[60px] flex-col gap-1">
        <span className="flex items-baseline justify-between gap-3">
          <span className="text-body-strong text-text">{item.ticker}</span>
          <span className="text-caption text-text-2">{KIND_LABEL[item.kind]}</span>
        </span>
        <span className="line-clamp-2 text-body text-text">{item.title}</span>
        {meta ? <span className="truncate text-caption text-text-2">{meta}</span> : null}
      </span>
    </a>
  );
}

/* ---------------------------------------------------------------- desktop */

function ThesisDesktop() {
  const shell = useShell();
  const t = useThesisItems();
  const [filter, setFilter] = useFilter();
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<string | null>(null);
  const needle = q.trim().toLowerCase();
  const list = t.items.filter((i) => matches(filter, i.kind) && (!needle || `${i.ticker} ${i.title}`.toLowerCase().includes(needle)));

  const open = (i: Item) => {
    setSel(i.key);
    if (i.kind === "dossier") shell.openDetail({ title: i.ticker, content: <DossierBody ticker={i.ticker} /> });
    else shell.openDetail({ title: i.ticker, content: <DraftBody id={i.key.slice(i.key.indexOf(":") + 1)} onDeleted={shell.closeDetail} /> });
  };
  const write = () => shell.openDetail({ title: "Tulis draft tesis", wide: true, content: <DraftEditor inPanel onSaved={shell.closeDetail} /> });

  const columns: Column<Item>[] = [
    { key: "ticker", header: "Ticker", width: "96px", render: (i) => <span className="font-bold">{i.ticker}</span> },
    { key: "kind", header: "Jenis", width: "104px", render: (i) => KIND_LABEL[i.kind] },
    { key: "title", header: "Judul", render: (i) => i.title },
    { key: "story", header: "Story status", width: "180px", render: (i) => i.story ?? "—" },
    { key: "upd", header: "Diperbarui", width: "112px", render: (i) => shortDate(i.updated) ?? "—" },
  ];

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        variant="screen"
        title="Tesis"
        trailing={
          <Button icon={<Plus />} onClick={write} className="h-11 px-5">
            Tulis draft tesis
          </Button>
        }
      />
      <div className="flex flex-wrap items-center gap-4">
        <FilterChips value={filter} onChange={setFilter} />
        <input
          type="search"
          aria-label="Cari ticker atau judul"
          placeholder="Cari ticker atau judul"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          className="h-11 w-72 rounded-sm border border-transparent bg-surface-sunken px-3 text-body text-text placeholder:text-text-2 focus-visible:border-accent"
        />
        <SourceNotice t={t} />
      </div>
      <DataTable
        caption="Tesis"
        columns={columns}
        rows={list}
        rowKey={(i) => i.key}
        status={t.status}
        onRetry={t.reload}
        onRowClick={open}
        selectedKey={sel}
        emptyMessage={filter === "dossier" ? "Belum ada dossier tesis di vault" : "Belum ada tesis"}
      />
    </div>
  );
}

/* ---------------------------------------------------------------- dossier */

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <SectionHeader title={title} level={3} />
      {children}
    </section>
  );
}

const Para = ({ children }: { children: ReactNode }) => <p className="whitespace-pre-line text-body text-text">{children}</p>;

/** Dossier vault, baca saja (design.md §3.4): header + Lens, Falsifiers, Review log, Evidence. */
export function DossierBody({ ticker }: { ticker: string }) {
  const r = useResource<ThesisDetail>(`/api/thesis/${encodeURIComponent(ticker)}`);
  if (r.status === "loading") return <Skeleton height={320} />;
  if (r.status === "error") {
    return r.error?.kind === "not_found" ? <ListState status="empty" message={`Belum ada dossier ${ticker} di vault`} /> : <ListState status="error" onRetry={r.reload} />;
  }
  const d = r.data!;
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        {d.company ? <p className="text-label text-text-2">{d.company}</p> : null}
        <Para>{d.alasan_masuk ?? d.verdict_text ?? d.verdict ?? "Thesis statement belum ditulis"}</Para>
        <p className="text-caption text-text-2">{[d.status, d.narrative_status ? `Story: ${d.narrative_status}` : null, d.tahap ? `Tahap: ${d.tahap}` : null].filter(Boolean).join(" · ")}</p>
      </div>
      <Section title="Lens">
        {d.lens.metrics.length ? (
          <div className="flex flex-col">
            {d.lens.metrics.map((m) => (
              <div key={m.metrik} className="flex min-h-11 items-start justify-between gap-4 border-b border-border-hairline py-2">
                <span className="text-label font-semibold text-text">{m.metrik}</span>
                <span className="text-right text-label text-text-2">{m.definisi || m.alasan}</span>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-label text-text-2">Lens belum diisi</p>
        )}
      </Section>
      <Section title="Falsifiers">
        {d.falsifier_items.length ? (
          <ol className="m-0 flex list-decimal flex-col gap-2 pl-5">
            {d.falsifier_items.map((f) => (
              <li key={f.n} className="text-body text-text">
                {f.text}
                {f.dates.length ? <span className="block text-caption text-text-2">{f.dates.join(", ")}</span> : null}
              </li>
            ))}
          </ol>
        ) : (
          <p className="text-label text-text-2">Belum ada falsifier</p>
        )}
      </Section>
      <Section title="Review log">
        {d.review_log.length ? (
          d.review_log.map((e, i) => (
            <div key={i} className="flex flex-col gap-1 border-b border-border-hairline pb-3">
              <p className="text-label font-bold text-text">{[e.date, e.title].filter(Boolean).join(" · ")}</p>
              <ul className="m-0 flex list-disc flex-col gap-1 pl-5">
                {e.bullets.map((b, j) => (
                  <li key={j} className={cn("text-label text-text", b.level === 1 && "ml-4")}>
                    {b.text}
                  </li>
                ))}
              </ul>
            </div>
          ))
        ) : (
          <p className="text-label text-text-2">Belum ada review</p>
        )}
      </Section>
      <Section title="Evidence">
        {d.doc_evidence.length ? (
          d.doc_evidence.map((e, i) => (
            <div key={i} className="flex min-h-11 items-start justify-between gap-4 border-b border-border-hairline py-2">
              <span className="flex flex-col">
                <span className="text-label font-semibold text-text">{`${e.metrik} · ${e.periode}`}</span>
                <span className="text-caption text-text-2">{[e.dokumen, e.hal ? `hal. ${e.hal}` : null].filter(Boolean).join(" · ")}</span>
              </span>
              <span className="num whitespace-nowrap text-right text-label text-text">{`${e.raw} ${e.unit}`.trim()}</span>
            </div>
          ))
        ) : (
          <p className="text-label text-text-2">Belum ada evidence</p>
        )}
      </Section>
    </div>
  );
}

function BackHeader({ title, href = "#/thesis" }: { title: string; href?: string }) {
  return (
    <div className="flex items-center gap-1">
      <a href={href} className="-ml-3 inline-flex min-h-11 items-center px-3 text-label font-semibold text-text-2">
        Kembali
      </a>
      <h1 className="min-w-0 truncate text-title text-text">{title}</h1>
    </div>
  );
}

function DossierScreen() {
  const { ticker = "" } = useParams();
  const t = ticker.toUpperCase();
  return (
    <div className="flex flex-col gap-4">
      <BackHeader title={t} />
      <DossierBody ticker={t} />
    </div>
  );
}

/* ---------------------------------------------------------------- draft detail + hapus */

function DraftBody({ id, onDeleted }: { id: string; onDeleted: () => void }) {
  const r = useResource<Draft | VaultDraft>(`/api/thesis/drafts/${encodeURIComponent(id)}`);
  const [confirm, setConfirm] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (r.status === "loading") return <Skeleton height={240} />;
  if (r.status === "error") return r.error?.kind === "not_found" ? <ListState status="empty" message="Draft tidak ditemukan" /> : <ListState status="error" onRetry={r.reload} />;
  const d = r.data!;
  const vault = "is_readonly" in d && d.is_readonly;

  const remove = async () => {
    setDeleting(true);
    setError(null);
    try {
      await api.delete(`/api/thesis/drafts/${encodeURIComponent(id)}`);
      setConfirm(false);
      invalidate("/api/thesis/");
      onDeleted();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : MESSAGES.server);
    } finally {
      setDeleting(false);
    }
  };

  const lists: [string, string[]][] = vault
    ? [["Falsifiers", (d as VaultDraft).falsifiers.map((f) => Object.values(f).filter(Boolean).slice(-1)[0] ?? "")]]
    : [
        ["Falsifiers", (d as Draft).falsifiers],
        ["Trigger tambah posisi", (d as Draft).triggers_add],
        ["Trigger keluar", (d as Draft).triggers_exit],
      ];

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <p className="text-body-strong text-text">{d.title}</p>
        <Para>{d.thesis_statement}</Para>
        <p className="text-caption text-text-2">
          {vault
            ? `Draft vault · ${(d as VaultDraft).file_info.name} · baca saja`
            : [(d as Draft).thesis_type, (d as Draft).is_primary ? "tesis utama" : null, `Skenario: ${(d as Draft).scenario_status}`, `Story: ${(d as Draft).story_status}`, `diperbarui ${shortDate((d as Draft).updated_at)}`]
                .filter(Boolean)
                .join(" · ")}
        </p>
      </div>
      {lists.map(([label, xs]) => (
        <Section key={label} title={label}>
          {xs.length ? (
            <ul className="m-0 flex list-disc flex-col gap-1 pl-5">
              {xs.map((x, i) => (
                <li key={i} className="text-body text-text">
                  {x}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-label text-text-2">—</p>
          )}
        </Section>
      ))}
      {!vault && (d as Draft).notes ? (
        <Section title="Catatan">
          <Para>{(d as Draft).notes}</Para>
        </Section>
      ) : null}
      {!vault ? (
        <Button variant="ghost" tone="danger" icon={<Trash2 />} onClick={() => setConfirm(true)} className="self-start">
          Hapus draft
        </Button>
      ) : null}
      <Sheet
        open={confirm}
        title="Hapus draft?"
        onClose={() => (deleting ? undefined : setConfirm(false))}
        footer={
          <div className="flex gap-3">
            <Button variant="sunken" className="flex-1" onClick={() => setConfirm(false)} disabled={deleting}>
              Batal
            </Button>
            <Button className="flex-1 !bg-status-over" onClick={remove} loading={deleting}>
              Hapus
            </Button>
          </div>
        }
      >
        <p className="text-body text-text">{`Draft "${d.title}" dihapus dari daftar draft. Dossier di vault tidak tersentuh.`}</p>
        {error ? (
          <p role="alert" className="mt-3 text-label font-semibold text-status-over">
            {error}
          </p>
        ) : null}
      </Sheet>
    </div>
  );
}

function DraftScreen() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const decoded = decodeURIComponent(id);
  return (
    <div className="flex flex-col gap-4">
      <BackHeader title="Draft tesis" href="#/thesis?jenis=draft" />
      <DraftBody id={decoded} onDeleted={() => navigate("/thesis?jenis=draft")} />
    </div>
  );
}

/* ---------------------------------------------------------------- editor */

const STORAGE_PREFIX = "finance.thesisDraft.";
const NEW_KEY = "_baru";

const emptyDraft = (): DraftInput => ({
  ticker: "",
  title: "",
  thesis_statement: "",
  thesis_type: "fundamental",
  is_primary: false,
  scenario_initial: "Menunggu",
  story_status_initial: "Belum terkonfirmasi",
  falsifiers: [""],
  triggers_add: [""],
  triggers_exit: [""],
  notes: "",
});

function readLocal(key: string): DraftInput | null {
  try {
    const raw = window.localStorage.getItem(STORAGE_PREFIX + key);
    return raw ? { ...emptyDraft(), ...(JSON.parse(raw) as Partial<DraftInput>) } : null;
  } catch {
    return null;
  }
}
function writeLocal(key: string, value: DraftInput | null) {
  try {
    if (value) window.localStorage.setItem(STORAGE_PREFIX + key, JSON.stringify(value));
    else window.localStorage.removeItem(STORAGE_PREFIX + key);
  } catch {
    /* storage diblokir: draft tetap di form selama sesi */
  }
}

function ListField({ label, values, onChange, addLabel }: { label: string; values: string[]; onChange: (v: string[]) => void; addLabel: string }) {
  return (
    <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
      <legend className="mb-2 text-label font-semibold text-text">{label}</legend>
      {values.map((v, i) => (
        <div key={i} className="flex items-center gap-2">
          <input
            aria-label={`${label} ${i + 1}`}
            value={v}
            onChange={(e) => onChange(values.map((x, j) => (j === i ? e.target.value : x)))}
            className="h-12 min-w-0 flex-1 rounded-sm border border-transparent bg-surface-sunken px-3 text-body text-text focus-visible:border-accent"
          />
          <Button variant="icon" aria-label={`Hapus ${label.toLowerCase()} ${i + 1}`} icon={<X />} onClick={() => onChange(values.length > 1 ? values.filter((_, j) => j !== i) : [""])} />
        </div>
      ))}
      <Button variant="ghost" icon={<Plus />} onClick={() => onChange([...values, ""])} className="self-start">
        {addLabel}
      </Button>
    </fieldset>
  );
}

function SelectField({ label, value, options, onChange }: { label: string; value: string; options: readonly string[]; onChange: (v: string) => void }) {
  return (
    <FormField label={label}>
      {(a11y) => (
        <select {...a11y} value={value} onChange={(e) => onChange(e.target.value)} className="h-12 w-full rounded-sm border border-transparent bg-surface-sunken px-3 text-body text-text focus-visible:border-accent">
          {options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      )}
    </FormField>
  );
}

/**
 * Editor draft (design.md §3.4). Isian disimpan lokal per ticker sampai berhasil terkirim;
 * `POST /api/thesis/drafts` hanya menulis file draft, tidak pernah vault (prd.md §3.3).
 */
export function DraftEditor({ onSaved, inPanel = false }: { onSaved: () => void; inPanel?: boolean }) {
  const [form, setForm] = useState<DraftInput>(() => readLocal(NEW_KEY) ?? emptyDraft());
  const [errors, setErrors] = useState<Partial<Record<keyof DraftInput | "_", string>>>({});
  const [saving, setSaving] = useState(false);
  const storageKey = form.ticker.trim().toUpperCase() || NEW_KEY;

  useEffect(() => {
    const blank = JSON.stringify(form) === JSON.stringify(emptyDraft());
    writeLocal(storageKey, blank ? null : form);
    if (storageKey !== NEW_KEY) writeLocal(NEW_KEY, null);
  }, [form, storageKey]);

  const set = <K extends keyof DraftInput>(k: K, v: DraftInput[K]) => {
    setErrors((e) => ({ ...e, [k]: undefined, _: undefined }));
    setForm((f) => ({ ...f, [k]: v }));
  };

  const onTickerBlur = () => {
    const key = form.ticker.trim().toUpperCase();
    if (!key) return;
    const stored = readLocal(key);
    if (stored && !form.title && !form.thesis_statement) setForm(stored);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const next: typeof errors = {};
    if (!/^[A-Z0-9\-_]{2,10}$/.test(form.ticker.trim().toUpperCase())) next.ticker = "Ticker 2–10 huruf/angka, mis. BBCA";
    if (!form.title.trim()) next.title = "Judul tesis wajib diisi";
    if (!form.thesis_statement.trim()) next.thesis_statement = "Thesis statement wajib diisi";
    if (Object.keys(next).length) return setErrors(next);
    setSaving(true);
    const clean = (xs: string[]) => xs.map((x) => x.trim()).filter(Boolean);
    try {
      await api.post("/api/thesis/drafts", {
        ...form,
        ticker: form.ticker.trim().toUpperCase(),
        title: form.title.trim(),
        thesis_statement: form.thesis_statement.trim(),
        falsifiers: clean(form.falsifiers),
        triggers_add: clean(form.triggers_add),
        triggers_exit: clean(form.triggers_exit),
      });
      writeLocal(storageKey, null);
      writeLocal(NEW_KEY, null);
      invalidate("/api/thesis/");
      onSaved();
    } catch (err) {
      const apiErr = err instanceof ApiError ? err : null;
      const fields: typeof errors = {};
      for (const [k, m] of Object.entries(apiErr?.fields ?? {})) if (k in form) fields[k as keyof DraftInput] = m;
      fields._ = apiErr?.message ?? MESSAGES.server;
      setErrors(fields);
    } finally {
      setSaving(false);
    }
  };

  return (
    <form onSubmit={submit} noValidate className="flex flex-col gap-5">
      <FormField label="Ticker" autoCapitalize="characters" autoComplete="off" value={form.ticker} error={errors.ticker} onBlur={onTickerBlur} onChange={(e) => set("ticker", e.target.value.toUpperCase())} />
      <FormField label="Judul" value={form.title} error={errors.title} onChange={(e) => set("title", e.target.value)} />
      <FormField label="Thesis statement" multiline rows={6} value={form.thesis_statement} error={errors.thesis_statement} onChange={(e) => set("thesis_statement", e.target.value)} />
      <SelectField label="Tipe tesis" value={form.thesis_type} options={THESIS_TYPES} onChange={(v) => set("thesis_type", v)} />
      <button
        type="button"
        role="switch"
        aria-checked={form.is_primary}
        onClick={() => set("is_primary", !form.is_primary)}
        className="flex min-h-11 items-center justify-between gap-3 text-left text-label font-semibold text-text"
      >
        Tesis utama
        <span className={cn("relative h-7 w-12 rounded-pill transition-colors", form.is_primary ? "bg-accent" : "bg-border-strong")} aria-hidden="true">
          <span className={cn("absolute top-0.5 h-6 w-6 rounded-pill bg-bg transition-[left]", form.is_primary ? "left-[22px]" : "left-0.5")} />
        </span>
      </button>
      <SelectField label="Skenario awal" value={form.scenario_initial} options={SCENARIOS} onChange={(v) => set("scenario_initial", v)} />
      <SelectField label="Story status awal" value={form.story_status_initial} options={STORY_STATUSES} onChange={(v) => set("story_status_initial", v)} />
      <ListField label="Falsifiers" values={form.falsifiers} onChange={(v) => set("falsifiers", v)} addLabel="Tambah falsifier" />
      <ListField label="Trigger tambah posisi" values={form.triggers_add} onChange={(v) => set("triggers_add", v)} addLabel="Tambah trigger" />
      <ListField label="Trigger keluar" values={form.triggers_exit} onChange={(v) => set("triggers_exit", v)} addLabel="Tambah trigger" />
      <FormField label="Catatan" multiline rows={4} value={form.notes} onChange={(e) => set("notes", e.target.value)} hint="Isian tersimpan di perangkat ini sampai draft terkirim" />
      {errors._ ? (
        <p role="alert" className="text-label font-semibold text-status-over">
          {errors._}
        </p>
      ) : null}
      {/* Mobile: di atas bottom nav + FAB (menonjol 26 px); panel desktop: dasar panel. */}
      <div className={cn("sticky bg-bg pt-3", inPanel ? "bottom-0 pb-3" : "bottom-[calc(var(--nav-bottom)+90px)]")}>
        <Button type="submit" fullWidth loading={saving}>
          Simpan draft
        </Button>
      </div>
    </form>
  );
}

function EditorScreen() {
  const navigate = useNavigate();
  return (
    <div className="flex flex-col gap-4">
      <BackHeader title="Tulis draft tesis" />
      <DraftEditor onSaved={() => navigate("/thesis?jenis=draft")} />
    </div>
  );
}
