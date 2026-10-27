import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DonutChart } from "./DonutChart";
import {
  arcPath,
  centerLabelLayout,
  centerMaxWidth,
  computeArcs,
  OTHER_LABEL,
  polar,
  prepareSegments,
  ringGeometry,
} from "./donutGeometry";

// Data moni-03: total Rp6,48 jt, 7 kategori.
const MONI = [
  { label: "Kebutuhan anak", value: 1_840_000 },
  { label: "Makan & dapur", value: 1_710_000 },
  { label: "Daycare", value: 1_250_000 },
  { label: "Tagihan", value: 820_000 },
  { label: "Transportasi", value: 460_000 },
  { label: "Jajan", value: 250_000 },
  { label: "Kesehatan", value: 150_000 },
];

describe("prepareSegments", () => {
  it("urut terbesar → terkecil, warna sesuai peringkat (cat-1 = terbesar)", () => {
    const segs = prepareSegments([...MONI].reverse());
    expect(segs.map((s) => s.label)).toEqual(MONI.map((d) => d.label));
    expect(segs.map((s) => s.cat)).toEqual([1, 2, 3, 4, 5, 6, 7]);
    expect(segs.reduce((a, s) => a + s.share, 0)).toBeCloseTo(1, 10);
  });

  it("segmen < 1,5% digabung ke Lainnya (cat-8)", () => {
    const segs = prepareSegments([
      { label: "A", value: 980 },
      { label: "B", value: 10 },
      { label: "C", value: 10 },
    ]);
    expect(segs.map((s) => [s.label, s.cat])).toEqual([
      ["A", 1],
      [OTHER_LABEL, 8],
    ]);
    expect(segs[1].merged).toEqual(["B", "C"]);
    expect(segs[1].value).toBe(20);
  });

  it("maks 7 segmen + Lainnya", () => {
    const data = Array.from({ length: 10 }, (_, i) => ({ label: `K${i}`, value: 100 - i }));
    const segs = prepareSegments(data);
    expect(segs).toHaveLength(8);
    expect(segs[7]).toMatchObject({ label: OTHER_LABEL, cat: 8, merged: ["K7", "K8", "K9"] });
  });

  it("nilai 0/negatif dibuang; semua kosong → []", () => {
    expect(prepareSegments([{ label: "A", value: 0 }])).toEqual([]);
    expect(prepareSegments([{ label: "A", value: 5 }, { label: "B", value: -3 }]).map((s) => s.label)).toEqual(["A"]);
  });
});

describe("geometri ring (§5.1)", () => {
  const ring = ringGeometry(164, 22);

  it("diameter 164, tebal 22 → r tengah 71; celah = 2 / 71 rad", () => {
    expect(ring.rMid).toBe(71);
    expect(ring.gapDeg).toBeCloseTo((2 / 71) * (180 / Math.PI), 10);
    expect(ring.gapDeg).toBeCloseTo(1.614, 3);
  });

  it("desktop 220 / 28 → r tengah 96", () => {
    expect(ringGeometry(220, 28).rMid).toBe(96);
  });

  it("mulai jam 12, searah jarum jam, sudut proporsional, celah 2 px di tiap batas", () => {
    const segs = prepareSegments(MONI);
    const arcs = computeArcs(segs, ring);
    expect(arcs[0].start).toBe(0);
    expect(arcs.at(-1)!.end).toBeCloseTo(360, 10);
    for (let i = 0; i < arcs.length; i++) {
      expect(arcs[i].end - arcs[i].start).toBeCloseTo(segs[i].share * 360, 10);
      expect(arcs[i].drawStart - arcs[i].start).toBeCloseTo(ring.gapDeg / 2, 10);
      expect(arcs[i].end - arcs[i].drawEnd).toBeCloseTo(ring.gapDeg / 2, 10);
      if (i > 0) {
        // panjang busur celah di r tengah = 2 px
        const gapArcPx = ((arcs[i].drawStart - arcs[i - 1].drawEnd) * Math.PI * ring.rMid) / 180;
        expect(gapArcPx).toBeCloseTo(2, 10);
      }
    }
    // Kebutuhan anak 28,4% → berakhir di ~102,2°
    expect(arcs[0].end).toBeCloseTo((1_840_000 / 6_480_000) * 360, 10);
  });

  it("satu segmen: ring penuh tanpa celah", () => {
    const arcs = computeArcs([{ share: 1 }], ring);
    expect(arcs[0]).toEqual({ start: 0, end: 360, drawStart: 0, drawEnd: 360 });
  });

  it("polar: 0° = atas, 90° = kanan", () => {
    const [x0, y0] = polar(100, 100, 50, 0);
    expect([x0, y0].map((n) => Math.round(n))).toEqual([100, 50]);
    const [x1, y1] = polar(100, 100, 50, 90);
    expect([x1, y1].map((n) => Math.round(n))).toEqual([150, 100]);
  });

  it("arcPath: flag large-arc untuk busur > 180°, sweep searah jarum jam", () => {
    expect(arcPath(0, 0, 10, 0, 90)).toMatch(/A 10 10 0 0 1 /);
    expect(arcPath(0, 0, 10, 0, 200)).toMatch(/A 10 10 0 1 1 /);
    expect(arcPath(0, 0, 10, 10, 10)).toBe("");
  });
});

describe("label tengah (§5.1)", () => {
  it("3 baris (20/28/18) dengan jarak 4 px, tercentang vertikal", () => {
    const { tops, height } = centerLabelLayout();
    expect(height).toBe(74);
    expect(tops).toEqual([-37, -13, 19]);
    // simetris: tepi atas baris 1 dan tepi bawah baris 3 berjarak sama dari pusat
    expect(-tops[0]).toBe(tops[2] + 18);
  });

  it("lebar maks = diameter dalam − 16 = 104 px di mobile", () => {
    expect(centerMaxWidth(164, 22)).toBe(104);
  });
});

describe("DonutChart interaksi", () => {
  const renderMoni = (props = {}) => render(<DonutChart data={MONI} title="Pengeluaran" size="mobile" {...props} />);
  const lines = (c: HTMLElement) => [...c.querySelectorAll("[data-center-line]")].map((n) => n.textContent);

  it("default: label total, posisi baris sesuai layout", () => {
    const { container } = renderMoni();
    expect(lines(container)).toEqual(["Pengeluaran", "Rp6,48 jt", "7 kategori"]);
    const c = (164 + 16) / 2;
    const tops = [...container.querySelectorAll<HTMLElement>("[data-center-line]")].map((n) => parseFloat(n.style.top));
    expect(tops).toEqual([c - 37, c - 13, c + 19]);
    expect(screen.getByRole("img").getAttribute("aria-label")).toBe("Pengeluaran Rp6,48 jt, terbesar Kebutuhan anak 28,4%");
  });

  it("tap segmen → label kategori, segmen lain opacity 0.35; tap lagi → kembali", () => {
    const onSel = vi.fn();
    const { container } = renderMoni({ onSelectedChange: onSel });
    const seg = screen.getByRole("button", { name: /^Daycare/ });
    fireEvent.click(seg);
    expect(onSel).toHaveBeenLastCalledWith(2);
    expect(seg).toHaveAttribute("aria-pressed", "true");
    expect(lines(container)).toEqual(["Daycare", "Rp1,25 jt", "19,3% dari total"]);
    const opacities = [...container.querySelectorAll("[data-arc]")].map((p) => p.getAttribute("opacity"));
    expect(opacities).toEqual(["0.35", "0.35", "1", "0.35", "0.35", "0.35", "0.35"]);
    fireEvent.click(seg);
    expect(onSel).toHaveBeenLastCalledWith(null);
    expect(lines(container)[0]).toBe("Pengeluaran");
  });

  it("tap area kosong → kembali ke total", () => {
    const { container } = renderMoni();
    fireEvent.click(screen.getByRole("button", { name: /^Tagihan/ }));
    expect(lines(container)[0]).toBe("Tagihan");
    fireEvent.click(container.querySelector("figure > div")!);
    expect(lines(container)[0]).toBe("Pengeluaran");
  });

  it("keyboard: tiap segmen tabindex 0, Enter/Space = tap, panah pindah segmen", () => {
    const { container } = renderMoni();
    const segs = screen.getAllByRole("button");
    expect(segs).toHaveLength(7);
    segs.forEach((s) => expect(s).toHaveAttribute("tabindex", "0"));

    segs[0].focus();
    fireEvent.keyDown(segs[0], { key: "Enter" });
    expect(lines(container)[0]).toBe("Kebutuhan anak");
    fireEvent.keyDown(segs[0], { key: " " });
    expect(lines(container)[0]).toBe("Pengeluaran");

    fireEvent.keyDown(segs[0], { key: "ArrowRight" });
    expect(document.activeElement).toBe(segs[1]);
    expect(lines(container)[0]).toBe("Makan & dapur");

    fireEvent.keyDown(segs[1], { key: "ArrowLeft" });
    fireEvent.keyDown(segs[0], { key: "ArrowLeft" });
    expect(document.activeElement).toBe(segs[6]);
    expect(lines(container)[0]).toBe("Kesehatan");
  });

  it("hit area diperlebar 8 px ke luar ring", () => {
    renderMoni();
    const hit = screen.getAllByRole("button")[0];
    expect(hit.getAttribute("stroke-width")).toBe("30");
    expect(hit.getAttribute("d")).toContain("A 75 75");
  });

  it("state loading / empty / error", () => {
    const onRetry = vi.fn();
    const { rerender, container } = render(<DonutChart data={MONI} title="Pengeluaran" size="mobile" status="loading" />);
    expect(container.querySelector("[data-skeleton]")).not.toBeNull();
    rerender(<DonutChart data={[]} title="Pengeluaran" size="mobile" />);
    expect(screen.getByText("Belum ada pengeluaran")).toBeInTheDocument();
    rerender(<DonutChart data={MONI} title="Pengeluaran" size="mobile" status="error" onRetry={onRetry} />);
    expect(screen.getByText("Gagal memuat")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(onRetry).toHaveBeenCalled();
  });

  it("tabel sr-only sebagai alternatif", () => {
    renderMoni();
    const table = screen.getByRole("table");
    expect(table).toHaveClass("sr-only-table");
    expect(table.querySelectorAll("tbody tr")).toHaveLength(7);
  });
});
