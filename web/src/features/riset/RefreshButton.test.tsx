import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { json, mockApi } from "../../test/mockApi";
import { RefreshButton } from "./RefreshButton";

afterEach(() => vi.unstubAllGlobals());

const calls = (fetchMock: ReturnType<typeof mockApi>) => fetchMock.mock.calls.map(([input, init]) => `${(init as RequestInit | undefined)?.method ?? "GET"} ${String(input)}`);

describe("Perbarui data", () => {
  it("a ticker gap refreshes that ticker only", async () => {
    const fetchMock = mockApi({ "/api/research/refresh": () => json({ started: true }, 202) });
    render(<RefreshButton ticker="BUMI" />);
    fireEvent.click(screen.getByRole("button", { name: "Perbarui data" }));
    await screen.findByRole("status");
    expect(calls(fetchMock).filter((c) => c.includes("/refresh"))).toEqual(["POST /api/research/refresh?ticker=BUMI"]);
    expect(screen.getByRole("status")).toHaveTextContent("Pembaruan BUMI berjalan");
  });

  it("a driver gap refreshes the whole market, because a single-ticker refresh skips drivers", async () => {
    const fetchMock = mockApi({ "/api/research/refresh": () => json({ started: true }, 202) });
    render(<RefreshButton scope="market" />);
    fireEvent.click(screen.getByRole("button", { name: "Perbarui data" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Pembaruan data pasar berjalan"));
    expect(calls(fetchMock).filter((c) => c.includes("/refresh"))).toEqual(["POST /api/research/refresh"]);
  });

  it("is a real button with a visible container, at least 44 px high", () => {
    mockApi({});
    render(<RefreshButton scope="market" />);
    const button = screen.getByRole("button", { name: "Perbarui data" });
    expect(button).toHaveClass("bg-surface-sunken", "min-h-11");
  });
});
