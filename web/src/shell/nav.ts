import { Building2, ChartPie, House, Wrench, type LucideIcon } from "lucide-react";
import { TAB_ROOTS, type Tab } from "./navState";

export interface NavEntry {
  key: Tab;
  label: string;
  icon: LucideIcon;
  /** Root route of the tab (design.md §2.1). */
  root: string;
}

/** The four bottom-bar tabs (design.md §2.1, D13): lucide, stroke 1.75; no FAB. */
export const TABS_NAV: NavEntry[] = [
  { key: "beranda", label: "Beranda", icon: House, root: TAB_ROOTS.beranda },
  { key: "portofolio", label: "Portofolio", icon: ChartPie, root: TAB_ROOTS.portofolio },
  { key: "perusahaan", label: "Perusahaan", icon: Building2, root: TAB_ROOTS.perusahaan },
  { key: "alat", label: "Alat", icon: Wrench, root: TAB_ROOTS.alat },
];

export const TAB_LABELS: Record<Tab, string> = {
  beranda: "Beranda",
  portofolio: "Portofolio",
  perusahaan: "Perusahaan",
  alat: "Alat",
};
