import type { Config } from "tailwindcss";

// Semua nilai menunjuk ke CSS custom properties di src/styles/tokens.css (design.md §1).
const v = (name: string) => `var(--${name})`;

const colorNames = [
  "bg",
  "surface-summary",
  "surface-sunken",
  "surface-icon",
  "surface-account-1",
  "surface-account-2",
  "surface-account-3",
  "surface-tint-blue",
  "surface-tint-green",
  "surface-inverse",
  "track",
  "border-hairline",
  "border-strong",
  "grid",
  "text",
  "text-2",
  "text-3",
  "text-on-accent",
  "accent",
  "accent-soft",
  "money-out",
  "money-in",
  "status-safe",
  "status-warn",
  "status-over",
  "status-neutral",
  "status-safe-fill",
  "status-warn-fill",
  "status-over-fill",
  "series-compare",
  "tooltip-date",
  "nav-inactive",
  ...[1, 2, 3, 4, 5, 6, 7, 8].flatMap((i) => [`cat-${i}`, `cat-${i}-tint`]),
];

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    screens: {
      // §2 kontrak: < 1024 px mobile, ≥ 1024 px desktop; ≥ 1280 px panel detail menempel.
      lg: "1024px",
      xl: "1280px",
      // design.md §4: pane Klaim & Jurnal menjadi kolom tetap 360 px.
      "2xl": "1440px",
    },
    colors: {
      transparent: "transparent",
      current: "currentColor",
      white: "#ffffff",
      ...Object.fromEntries(colorNames.map((n) => [n, v(n)])),
    },
    fontFamily: {
      sans: v("font-sans"),
    },
    fontSize: {
      // §1.4: [size, { lineHeight, letterSpacing, fontWeight }]
      hero: ["36px", { lineHeight: "40px", letterSpacing: "-0.5px", fontWeight: "800" }],
      title: ["20px", { lineHeight: "26px", letterSpacing: "-0.2px", fontWeight: "800" }],
      "title-desktop": ["24px", { lineHeight: "30px", letterSpacing: "-0.2px", fontWeight: "800" }],
      "donut-total": ["22px", { lineHeight: "28px", letterSpacing: "-0.3px", fontWeight: "800" }],
      stat: ["17px", { lineHeight: "22px", letterSpacing: "0", fontWeight: "800" }],
      "body-strong": ["16px", { lineHeight: "22px", letterSpacing: "0", fontWeight: "700" }],
      body: ["16px", { lineHeight: "22px", letterSpacing: "0", fontWeight: "500" }],
      label: ["14px", { lineHeight: "20px", letterSpacing: "0", fontWeight: "500" }],
      caption: ["13px", { lineHeight: "18px", letterSpacing: "0", fontWeight: "500" }],
      axis: ["12px", { lineHeight: "16px", letterSpacing: "0", fontWeight: "500" }],
      table: ["13px", { lineHeight: "18px", letterSpacing: "0", fontWeight: "500" }],
      "table-head": ["11px", { lineHeight: "16px", letterSpacing: "0.6px", fontWeight: "700" }],
      // §3.1 input nominal di sheet: 28 px 800
      amount: ["28px", { lineHeight: "34px", letterSpacing: "-0.3px", fontWeight: "800" }],
    },
    extend: {
      spacing: {
        "4.5": "18px",
        "card-row": "10px",
        sidebar: v("sidebar-width"),
        detail: v("detail-width"),
      },
      borderRadius: {
        sm: v("radius-sm"),
        md: v("radius-md"),
        lg: v("radius-lg"),
        pill: v("radius-pill"),
      },
      borderColor: {
        DEFAULT: v("border-hairline"),
      },
      boxShadow: {
        none: v("shadow-none"),
        float: v("shadow-float"),
        fab: v("shadow-fab"),
        popover: v("shadow-popover"),
      },
      maxWidth: {
        mobile: "480px",
        content: "1040px",
      },
    },
  },
  plugins: [],
} satisfies Config;
