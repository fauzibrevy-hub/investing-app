import AxeBuilder from "@axe-core/playwright";
import type { Page } from "@playwright/test";

export interface TargetIssue {
  el: string;
  w: number;
  h: number;
}
export interface Collision {
  parent: string;
  a: string;
  b: string;
}
export interface AxeIssue {
  id: string;
  impact: string | null | undefined;
  nodes: string[];
}

/** Lebar halaman: tidak boleh ada gulir horizontal pada dokumen, `body` atau `<main>` (toleransi 0 px). */
export async function horizontalOverflow(page: Page): Promise<{ width: number; scroll: number; main: number; overflow: boolean }> {
  return page.evaluate(() => {
    const doc = document.documentElement;
    const main = document.querySelector("main");
    const width = doc.clientWidth;
    const scroll = Math.max(doc.scrollWidth, document.body.scrollWidth);
    const mainOver = main ? main.scrollWidth - main.clientWidth : 0;
    return { width, scroll, main: mainOver, overflow: scroll > width || mainOver > 0 };
  });
}

/**
 * Tabrakan teks: dua anak langsung dari induk yang sama, keduanya terlihat, keduanya bertulisan, dan kotak barisnya
 * beririsan lebih dari 1 px di kedua sumbu. Dibandingkan per kotak baris (`getClientRects`), bukan kotak pembatas:
 * elemen inline yang membungkus ke beberapa baris punya kotak pembatas berupa gabungan baris-barisnya, yang "menimpa"
 * saudara inline di baris yang sama padahal kalimatnya mengalir biasa. Elemen blok punya satu kotak, jadi perilakunya
 * tidak berubah. Elemen berposisi absolute, fixed atau sticky melayang di atas isi (bilah, hamparan) dan tidak dibandingkan
 * dengan saudara yang mengalir; dua saudara yang sama-sama melayang ditempatkan sengaja dan tidak boleh saling menimpa.
 */
export async function textCollisions(page: Page): Promise<Collision[]> {
  return page.evaluate(() => {
    const label = (el: Element) => {
      const h = el as HTMLElement;
      const text = (h.innerText ?? "").replace(/\s+/g, " ").trim().slice(0, 40);
      return `${el.tagName.toLowerCase()}${h.className && typeof h.className === "string" ? "." + h.className.split(/\s+/).slice(0, 3).join(".") : ""} "${text}"`;
    };
    /** Kotak baris elemen (satu untuk elemen blok), tanpa yang ≤ 1 px; `null` bila tak terlihat atau tak bertulisan. */
    const live = (el: Element) => {
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden" || cs.opacity === "0") return null;
      if (!((el as HTMLElement).innerText ?? "").trim()) return null;
      const rects = Array.from(el.getClientRects()).filter((r) => r.width > 1 && r.height > 1);
      if (rects.length === 0) return null;
      return { rects, floating: ["absolute", "fixed", "sticky"].includes(cs.position) };
    };
    const out: { parent: string; a: string; b: string }[] = [];
    for (const parent of Array.from(document.body.querySelectorAll("*"))) {
      const kids = Array.from(parent.children)
        .map((el) => ({ el, box: live(el) }))
        .filter((k): k is { el: Element; box: NonNullable<ReturnType<typeof live>> } => k.box !== null);
      for (let i = 0; i < kids.length; i++) {
        for (let j = i + 1; j < kids.length; j++) {
          if (kids[i].box.floating !== kids[j].box.floating) continue;
          const hit = kids[i].box.rects.some((a) =>
            kids[j].box.rects.some((b) => Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1),
          );
          if (hit) out.push({ parent: label(parent), a: label(kids[i].el), b: label(kids[j].el) });
        }
      }
    }
    return out;
  });
}

/**
 * Sasaran ketuk ≥ 44 × 44 px (design.md §8; hanya dipakai di layar seluler). Dikecualikan: elemen tak terlihat atau
 * `sr-only` (≤ 1 px), nonaktif, dan tautan sebaris di dalam kalimat (WCAG 2.5.8 "inline").
 */
export async function smallTargets(page: Page, min = 44): Promise<TargetIssue[]> {
  return page.evaluate((minSize) => {
    const sel = 'a[href], button, input:not([type="hidden"]), select, textarea, summary, [role="button"], [role="tab"], [role="link"], [role="switch"], [role="checkbox"], [role="radio"], [role="menuitem"]';
    const out: { el: string; w: number; h: number }[] = [];
    for (const el of Array.from(document.querySelectorAll<HTMLElement>(sel))) {
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden" || el.closest("[aria-hidden='true']") || (el as HTMLButtonElement).disabled) continue;
      let box: Element = el;
      if (el instanceof HTMLInputElement && (el.type === "checkbox" || el.type === "radio")) box = el.closest("label") ?? el;
      const r = box.getBoundingClientRect();
      if (r.width <= 1 || r.height <= 1) continue;
      if (el.tagName === "A" && cs.display === "inline" && Array.from(el.parentElement?.childNodes ?? []).some((n) => n.nodeType === 3 && (n.textContent ?? "").trim())) continue;
      if (r.width < minSize - 0.5 || r.height < minSize - 0.5) {
        const name = (el.getAttribute("aria-label") ?? el.innerText ?? el.getAttribute("title") ?? "").replace(/\s+/g, " ").trim().slice(0, 40);
        out.push({ el: `${el.tagName.toLowerCase()}${el.getAttribute("role") ? `[role=${el.getAttribute("role")}]` : ""} "${name}"`, w: Math.round(r.width * 10) / 10, h: Math.round(r.height * 10) / 10 });
      }
    }
    return out;
  }, min);
}

/** axe-core (`target-size` dimatikan di seluler, lihat di bawah), aturan WCAG 2.0/2.1/2.2 A dan AA; hanya `serious` dan `critical` yang menggagalkan. */
export async function axeSeriousCritical(page: Page, { mobile = false }: { mobile?: boolean } = {}): Promise<{ all: Record<string, number>; blocking: AxeIssue[] }> {
  // Seluler: ukuran sasaran sudah diperiksa `smallTargets` (44 px, lebih ketat dari 24 px), sedangkan `target-size` axe
  // juga menandai tombol yang sebagian tertutup komposer jurnal yang menempel (isi yang tergulir di bawahnya).
  const builder = new AxeBuilder({ page });
  if (mobile) builder.disableRules(["target-size"]);
  const result = await builder.withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"]).analyze();
  const all: Record<string, number> = {};
  for (const v of result.violations) all[v.impact ?? "none"] = (all[v.impact ?? "none"] ?? 0) + 1;
  const blocking = result.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => ({ id: v.id, impact: v.impact, nodes: v.nodes.slice(0, 5).map((n) => `${n.target.join(" ")} :: ${n.failureSummary?.split("\n")[1] ?? ""}`.slice(0, 220)) }));
  return { all, blocking };
}
