import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Markdown } from "./Markdown";

describe("Markdown (vault text)", () => {
  it("renders paragraphs, lists, headings as bold text, and inline emphasis without raw HTML", () => {
    const { container } = render(<Markdown text={"# Inti\n\nSatu **dua** _tiga_ `x`\nbaris dua\n\n1. a\n2. b\n\n- c\n<b>d</b>"} />);
    expect(container.querySelector("h1")).toBeNull();
    expect(container.querySelector("p.font-bold")).toHaveTextContent("Inti");
    expect(container.querySelector("strong")).toHaveTextContent("dua");
    expect(container.querySelector("em")).toHaveTextContent("tiga");
    expect(container.querySelector("code")).toHaveTextContent("x");
    expect([...container.querySelectorAll("ol li")].map((li) => li.textContent)).toEqual(["a", "b"]);
    expect(container.querySelector("ul li")).toHaveTextContent("c");
    expect(container.querySelector("b")).toBeNull();
    expect(container).toHaveTextContent("<b>d</b>");
  });

  it("leaves snake_case names and lone asterisks alone", () => {
    const { container } = render(<Markdown text={"net_debt_ebitda naik, 2 * 3 * 4"} />);
    expect(container.querySelector("em")).toBeNull();
    expect(container).toHaveTextContent("net_debt_ebitda naik, 2 * 3 * 4");
  });
});
