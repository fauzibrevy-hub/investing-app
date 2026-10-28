import { useEffect, useRef, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { Card } from "../../components/Card";
import { Skeleton } from "../../components/Skeleton";
import { markSeen, useDossier } from "../research/api";
import { FactDrawerProvider } from "../research/components/FactDrawer";
import { AngkaKunci } from "./blocks/AngkaKunci";
import { DataBelumAda } from "./blocks/DataBelumAda";
import { Footer } from "./blocks/Footer";
import { Harga } from "./blocks/Harga";
import { Header } from "./blocks/Header";
import { Jurnal } from "./blocks/Jurnal";
import { Keuangan } from "./blocks/Keuangan";
import { Klaim } from "./blocks/Klaim";
import { KuartalTerakhir } from "./blocks/KuartalTerakhir";
import { Sorotan } from "./blocks/Sorotan";
import { Struktur } from "./blocks/Struktur";
import { COLLAPSIBLE, sectionTarget } from "./sections";
import { PAGE } from "./ui";

function NotFound({ ticker }: { ticker: string }) {
  return (
    <div className={PAGE}>
      <Card status="empty" emptyMessage={`${ticker} tidak ada di daftar riset`} minHeight={160} emptyAction={<Link to="/riset" className="inline-flex min-h-11 items-center text-label font-bold text-accent">Kembali ke Riset</Link>} />
    </div>
  );
}

function CompanyBody({ ticker }: { ticker: string }) {
  const dossier = useDossier(ticker);
  const [params] = useSearchParams();
  const [open, setOpen] = useState({ jurnal: COLLAPSIBLE.jurnal ?? false, harga: COLLAPSIBLE.harga ?? true, keuangan: COLLAPSIBLE.keuangan ?? false });
  const ready = dossier.status === "ready" && Boolean(dossier.data);
  const target = sectionTarget(params.get("section"));

  // Dossier dibuka = ditinjau (`TERAKHIR DITINJAU` di Desk). Gagal ditandai bukan galat bagi layar ini.
  const seen = useRef(false);
  useEffect(() => {
    if (!ready || seen.current) return;
    seen.current = true;
    markSeen(ticker).catch(() => undefined);
  }, [ready, ticker]);

  // `?section=` membuka blok lipat yang dituju lalu menggulirkannya ke atas; blok yang dimuat belakangan
  // bergeser, jadi gulir diulang setelah render berikutnya.
  useEffect(() => {
    if (!ready || !target) return;
    if (target in open && !open[target as keyof typeof open]) setOpen((o) => ({ ...o, [target]: true }));
    const id = requestAnimationFrame(() => document.getElementById(target)?.scrollIntoView?.({ block: "start" }));
    return () => cancelAnimationFrame(id);
    // `open` sengaja tidak jadi dependensi: menutup blok tidak boleh menggulir ulang.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, target]);

  if (dossier.error?.kind === "not_found" && !dossier.data) return <NotFound ticker={ticker} />;
  if (dossier.status === "error" && !dossier.data) return <div className={PAGE}><Card status="error" errorMessage={dossier.error?.message ?? "Gagal memuat riset"} onRetry={dossier.reload} minHeight={160} /></div>;
  if (!dossier.data) {
    return (
      <div role="status" aria-busy="true" className={PAGE}>
        <span className="sr-only">{`Memuat riset ${ticker}…`}</span>
        <Skeleton height={44} />
        <Skeleton height={120} />
        <Skeleton height={240} />
      </div>
    );
  }
  const data = dossier.data;
  const toggle = (id: keyof typeof open) => () => setOpen((o) => ({ ...o, [id]: !o[id] }));
  return (
    <div className={PAGE}>
      <Header ticker={ticker} dossier={data} />
      <Struktur ticker={ticker} name={data.name} note={data.business_note} />
      <AngkaKunci ticker={ticker} />
      <KuartalTerakhir ticker={ticker} />
      <Sorotan ticker={ticker} />
      <Klaim ticker={ticker} dossier={data} />
      <DataBelumAda ticker={ticker} />
      <Jurnal ticker={ticker} open={open.jurnal} onToggle={toggle("jurnal")} />
      <Harga ticker={ticker} held={data.role === "held"} open={open.harga} onToggle={toggle("harga")} />
      <Keuangan ticker={ticker} dossier={data} open={open.keuangan} onToggle={toggle("keuangan")} />
      <Footer dossier={data} />
    </div>
  );
}

/** `#/riset/:ticker`: halaman perusahaan satu kolom (ui-slicing.md §1). Satu `FactDrawer` bersama untuk semua `FactChip` di bawahnya. */
export function CompanyPage() {
  const { ticker = "" } = useParams();
  const upper = ticker.toUpperCase();
  return (
    <FactDrawerProvider>
      <CompanyBody key={upper} ticker={upper} />
    </FactDrawerProvider>
  );
}
