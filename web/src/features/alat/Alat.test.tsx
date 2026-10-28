import { render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it } from "vitest";
import desk from "../../../../tests/fixtures/research/canon/desk.json";
import { mockApi } from "../../test/mockApi";
import { Alat, default as AlatDefault } from "./Alat";
import { ToolRow } from "./ToolRow";

function renderAlat() {
  return render(
    <MemoryRouter initialEntries={["/alat"]}>
      <Routes>
        <Route path="/alat" element={<Alat />} />
        <Route path="/alat/:tool" element={<p>layar alat</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  mockApi({ "/api/research/desk": desk });
});

describe("Alat #/alat (design.md §3.5)", () => {
  it("is exported by name and as default", () => {
    expect(AlatDefault).toBe(Alat);
  });

  it("header 'Alat', then exactly three rows, then the footer", async () => {
    renderAlat();
    expect(screen.getByRole("heading", { level: 1, name: "Alat" })).toBeInTheDocument();
    const list = screen.getByRole("list", { name: "Alat" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(3);
    expect(await screen.findByText("Bukan saran investasi")).toBeInTheDocument();
    expect(screen.getByRole("contentinfo")).toHaveTextContent("Bukan saran investasi");
  });

  it("each row names the tool and its one-line purpose, in order", () => {
    renderAlat();
    const rows = within(screen.getByRole("list", { name: "Alat" })).getAllByRole("listitem");
    expect(rows.map((r) => [r.querySelector('[data-line="1"]')?.textContent, r.querySelector('[data-line="2"]')?.textContent])).toEqual([
      ["Bandingkan", "Satu paket, beberapa emiten berdampingan"],
      ["Kalibrasi", "Klaim dan level Anda, dan apa yang terjadi sesudahnya"],
      ["Paket & aturan", "Metrik per industri, sumber pendorong, aturan"],
    ]);
  });

  it("each row is a link to its tool screen (§2.2)", () => {
    renderAlat();
    const links = within(screen.getByRole("list", { name: "Alat" })).getAllByRole("link");
    expect(links.map((l) => l.getAttribute("href"))).toEqual(["/alat/bandingkan", "/alat/kalibrasi", "/alat/paket"]);
  });

  it("every row is a whole-row target of at least 64 px with a chevron", () => {
    renderAlat();
    const links = within(screen.getByRole("list", { name: "Alat" })).getAllByRole("link");
    for (const link of links) {
      expect(link).toHaveClass("min-h-16", "w-full");
      expect(link.querySelector("svg[aria-hidden='true']")).not.toBeNull();
    }
  });

  it("a row's accessible name is its name followed by its purpose", () => {
    renderAlat();
    expect(screen.getByRole("link", { name: "Bandingkan Satu paket, beberapa emiten berdampingan" })).toBeInTheDocument();
  });

  it("the footer names the data ages once the desk has loaded", async () => {
    renderAlat();
    expect(await screen.findByText(/^Umur data · Harga: /)).toBeInTheDocument();
  });

  it("a failing desk still shows the three tools and the footer without ages", async () => {
    mockApi({});
    renderAlat();
    await screen.findByText("Bukan saran investasi");
    expect(within(screen.getByRole("list", { name: "Alat" })).getAllByRole("listitem")).toHaveLength(3);
    expect(screen.queryByText(/^Umur data/)).toBeNull();
  });

  it("phone layout only: no desktop breakpoint classes and no 720 px cap", () => {
    const { container } = renderAlat();
    expect(container.innerHTML).not.toMatch(/\b(sm|md|lg|xl|2xl):|max-w-\[720px\]/);
  });
});

describe("ToolRow", () => {
  it("renders a two-line link row", () => {
    render(
      <MemoryRouter>
        <ToolRow to="/alat/x" name="Nama" purpose="Tujuan" />
      </MemoryRouter>,
    );
    const link = screen.getByRole("link", { name: "Nama Tujuan" });
    expect(link).toHaveAttribute("href", "/alat/x");
    expect(link.querySelector('[data-line="1"]')).toHaveTextContent("Nama");
    expect(link.querySelector('[data-line="2"]')).toHaveTextContent("Tujuan");
  });
});
