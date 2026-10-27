import { ChartColumn, ChartPie, FileText, House, ScanSearch, type LucideIcon } from "lucide-react";

export type RouteKey = "home" | "report" | "portfolio" | "thesis" | "research" | "idx" | "riset";

export interface NavEntry {
  key: string;
  label: string;
  icon: LucideIcon;
  href: string;
  /** Route yang membuat entri ini aktif. */
  routes: RouteKey[];
}

/** 4 tab mobile + FAB di tengah (design.md §2). Ikon ASUMSI: lucide, stroke 1.75. */
export const MOBILE_TABS: NavEntry[] = [
  { key: "home", label: "Beranda", icon: House, href: "#/home", routes: ["home"] },
  { key: "report", label: "Laporan", icon: ChartColumn, href: "#/report", routes: ["report"] },
  { key: "invest", label: "Investasi", icon: ChartPie, href: "#/portfolio", routes: ["portfolio", "thesis"] },
  { key: "research", label: "Riset", icon: ScanSearch, href: "#/riset", routes: ["riset", "research", "idx"] },
];

/** 5 entri sidebar desktop (design.md §2). Ikon Tesis/Riset: ASUMSI. Macro dilipat ke Desk Riset sebagai kartu konteks (prd.md §9). */
export const DESKTOP_ENTRIES: NavEntry[] = [
  { key: "home", label: "Beranda", icon: House, href: "#/home", routes: ["home"] },
  { key: "report", label: "Laporan", icon: ChartColumn, href: "#/report", routes: ["report"] },
  { key: "portfolio", label: "Portofolio", icon: ChartPie, href: "#/portfolio", routes: ["portfolio"] },
  { key: "thesis", label: "Tesis", icon: FileText, href: "#/thesis", routes: ["thesis"] },
  { key: "research", label: "Riset", icon: ScanSearch, href: "#/riset", routes: ["riset", "research", "idx"] },
];

export const ROUTE_TITLES: Record<RouteKey, string> = {
  home: "Beranda",
  report: "Laporan",
  portfolio: "Portofolio",
  thesis: "Tesis",
  research: "Riset",
  idx: "Riset",
  riset: "Riset",
};
