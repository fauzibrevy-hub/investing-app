---
description: Draft the operational structure map of a company from an annual report or investor presentation
argument-hint: <TICKER> <path to the PDF or PPTX>
---

# Draft the structure map of a company

Arguments: `$ARGUMENTS` = `<TICKER> <path to the uploaded annual report or presentation>`.

Write `<data_dir>/research/structure/<TICKER>.draft.json`, where `<data_dir>` is `$INVESTING_DATA_DIR` (default `~/investing-app-data`). Create the folder if it is missing. Write nothing else, call no API, and never touch `research.sqlite`, `research-log.jsonl` or any token. The app loads the file when the owner taps **Muat draf** on the company page, validates it whole, and shows every node as an *usulan* the owner accepts, edits or rejects. Only accepted nodes become the map.

Run this once per company per year, or when the business changes. A later import replaces the owner's unaccepted nodes and keeps accepted ones, so a re-run may draft the whole map again.

## What the map is

A top-down tree of **how the company makes money**: business lines, what each sells and to whom, what prices it, its main costs, and what reaches the owner. Example shape for a coal miner (labels in Indonesian, invented here only to show the shape):

```
Perusahaan
├─ Segmen batubara            (segmen)
│  ├─ Batubara termal ekspor  (produk)
│  │  ├─ Pembeli di Asia      (pelanggan)
│  │  └─ Harga acuan Newcastle (pendorong_harga)
│  └─ Kontraktor tambang      (biaya)
└─ Porsi kepemilikan di anak usaha (kepemilikan)
```

## Hard rules

1. **Every node cites its source**: `source.document` (the file name), `source.page` (the PDF page, or the slide number of a presentation, counting from 1 as the viewer shows it) and `source.snippet`, a passage copied **verbatim** from that page, at most **160 characters**. If you cannot quote a passage for a node, leave the node out.
2. **Never invent numbers.** A label or note may contain a figure only if it appears in the cited snippet. Do not estimate, convert or round.
3. **Link a node to a metric only when the document names that metric** (`metric` is one of the ids listed below for the ticker's pack or core). Do not guess a link to make the tree richer. The app shows the metric's latest reported value next to the node.
4. Labels are Indonesian, at most **80 characters**, sentence case, no verdict words ("murah", "mahal", "layak", "beli", "jual"). Notes are optional, at most **200 characters**, and state facts from the document.
5. Aim for **at most 32 nodes** (hard limit 40) and **depth at most 4** (a child of the root has depth 1). The app caps a map at 40 non-rejected nodes, so a draft of 32 leaves the owner room to add branches without rejecting proposals first. Prefer fewer, clearer branches over completeness.
6. `kind` is exactly one of: `segmen`, `produk`, `pelanggan`, `pendorong_harga`, `biaya`, `kepemilikan`, `lainnya`.
7. Ids are short and unique inside the file: letters, digits, `-`, `_`, `~` (at most 64 characters), for example `n1`, `n2`. `parent` is the id of another node **in this file**, or `null` for a child of the root. Parents must exist; no cycles.
8. Nodes are `proposed` implicitly: do not write `state`, `owner` or anything else outside the schema. The app refuses a file with extra fields, a missing source, an unknown metric, a label over 80 characters, a snippet over 160, too many nodes or too much depth, and imports nothing in that case.
9. The file's `ticker` must equal the ticker argument (upper case).

## Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Riset structure draft",
  "type": "object",
  "additionalProperties": false,
  "required": ["ticker", "nodes"],
  "properties": {
    "version": { "const": 1 },
    "ticker": { "type": "string", "pattern": "^[A-Z0-9]{4}$" },
    "nodes": {
      "type": "array",
      "maxItems": 40,
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "label", "kind", "source"],
        "properties": {
          "id": { "type": "string", "pattern": "^[A-Za-z0-9_~-]{1,64}$" },
          "parent": { "type": ["string", "null"] },
          "label": { "type": "string", "minLength": 1, "maxLength": 80 },
          "kind": {
            "enum": ["segmen", "produk", "pelanggan", "pendorong_harga", "biaya", "kepemilikan", "lainnya"]
          },
          "metric": { "type": ["string", "null"] },
          "note": { "type": ["string", "null"], "maxLength": 200 },
          "source": {
            "type": "object",
            "additionalProperties": false,
            "required": ["document", "page", "snippet"],
            "properties": {
              "document": { "type": "string", "minLength": 1, "maxLength": 200 },
              "page": { "type": "integer", "minimum": 1 },
              "snippet": { "type": "string", "minLength": 1, "maxLength": 160 }
            }
          }
        }
      }
    }
  }
}
```

Minimal valid file (invented values, only to show the shape):

```json
{
  "version": 1,
  "ticker": "ABCD",
  "nodes": [
    {
      "id": "n1",
      "parent": null,
      "label": "Segmen utama",
      "kind": "segmen",
      "source": { "document": "laporan-tahunan-2025.pdf", "page": 14, "snippet": "Segmen utama kami adalah ..." }
    },
    {
      "id": "n2",
      "parent": "n1",
      "label": "Harga jual rata-rata",
      "kind": "pendorong_harga",
      "metric": "asp_usd_t",
      "source": { "document": "laporan-tahunan-2025.pdf", "page": 37, "snippet": "Harga jual rata-rata kami ..." }
    }
  ]
}
```

## Metric ids you may link

Use an id from the ticker's pack or from core. A ticker without a pack can only link core metrics. Source of truth: `research/packs/*.toml`; if a file disagrees with this list, the file wins.

**core** (every ticker): `revenue`, `gross_profit`, `operating_profit`, `net_income`, `net_margin`, `ebitda`, `operating_cash_flow`, `capex`, `free_cash_flow`, `total_equity`, `cash`, `total_debt`, `net_debt`.

**coal_mining** (BUMI): `production_mt` (produksi), `sales_mt` (penjualan), `asp_usd_t` (harga jual rata-rata), `cash_cost_usd_t` (biaya tunai), `stripping_ratio` (nisbah kupas), `mining_cost_ratio` (biaya penambangan / pendapatan), `royalty_ratio` (royalti / pendapatan), `deferred_stripping_vs_production`, `reclamation_provisions`, `net_debt_to_ebitda`, `reserves_mt` (cadangan), `mine_life_years`.

**bank_sharia** (BRIS): `third_party_funds`, `casa_ratio`, `cost_of_funds`, `total_financing`, `fdr`, `financing_yield`, `mix_murabahah`, `mix_musyarakah`, `mix_mudharabah`, `mix_ijarah`, `mix_qardh`, `npf_gross`, `npf_net`, `coverage`, `provision_cost`, `net_margin_earning_assets`, `cost_to_income`, `car`, `tier1`.

**shipping** (LEAD): `vessel_count`, `fleet_age_years`, `utilization_pct`, `time_charter_share_pct`, `backlog_usd_m`, `backlog_tenor_years`, `day_rate_usd` (tarif harian rata-rata), `revenue_per_vessel`, `ebitda_per_vessel`, `freight_handling_ratio`, `cost_of_revenue_ratio`, `drydock_next_12m`, `vessels_gross`, `vessels_accumulated_depreciation`, `capex_to_depreciation`, `net_debt_to_ebitda`.

**shipping_tanker** (SOCI, via override): `vessel_count`, `utilization_pct`, `fleet_capacity_mdwt` (kapasitas armada, juta DWT), `shipyard_vessels_serviced` (kapal ditangani galangan), `time_charter_share_pct`, `revenue_per_vessel`, `ebitda_per_vessel`, `cost_of_revenue_ratio`, `vessels_gross`, `vessels_accumulated_depreciation`, `capex_to_depreciation`, `net_debt_to_ebitda`.

**heavy_equipment_contracting** (UNTR): `komatsu_units`, `overburden_mbcm`, `coal_extracted_mt`, `coal_sales_mt`, `coal_asp_usd_t`, `gold_oz_sold`, `gold_aisc_usd_oz`, `parts_service_ratio`, `seg_rev_machinery`, `seg_rev_contracting`, `seg_rev_coal`, `seg_rev_gold`, `seg_margin_machinery`, `seg_margin_contracting`, `seg_margin_coal`, `seg_margin_gold`, `capex_to_depreciation`, `net_cash`.

**it_distribution** (MSTI): `top_vendor_share_pct`, `hardware_share_pct`, `recurring_share_pct`, `gross_margin`, `operating_margin`, `receivable_days`, `inventory_days`, `payable_days`, `cash_conversion_cycle`, `customers_over_10pct_share`, `backlog_idr_b`, `net_cash`.

## Steps

1. Read the document fully (every page, every slide). Note the business lines, products, customers, price references (for example Newcastle or HBA for coal, charter day rates for shipping), main costs and ownership structure the document itself describes.
2. Build the tree within the limits above. When the document is silent on a branch, omit it; the owner can add branches by hand.
3. Re-check each node: snippet copied verbatim and at most 160 characters, page correct, label at most 80 characters, no figure that is not in the snippet, `metric` only where the document names it.
4. Write the JSON file (valid JSON, UTF-8, no comments, no trailing commas).
5. Report to the owner in Indonesian: the file path, the number of nodes, which nodes you left out because you could not quote them, and that **Muat draf** on the company page imports it.
