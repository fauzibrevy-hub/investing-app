//! Seven synthetic tickers across the five industry packs plus one with no pack (research
//! plan §7.2 golden): 18 filings each (2022Q1 to 2026Q2) so TTM and ten-year-style
//! percentiles have history. Every number is `base + step × quarter index`, in millions,
//! and stays under ten digits. Names of elements match the packs; values are invented.

#![allow(dead_code)]

use std::path::Path;

use crate::xbrl_fixture::{write, Spec, F};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// Standalone per quarter; the filing carries the year-to-date sum.
    Flow,
    /// Value at the quarter end.
    Instant,
}

#[derive(Clone)]
pub struct El {
    pub element: &'static str,
    pub kind: Kind,
    /// `IDR`, `USD` or `Pure`.
    pub unit: &'static str,
    pub decimals: i32,
    pub base: f64,
    pub step: f64,
}

const fn flow(element: &'static str, base: f64, step: f64) -> El {
    El {
        element,
        kind: Kind::Flow,
        unit: "ccy",
        decimals: -6,
        base,
        step,
    }
}

const fn inst(element: &'static str, base: f64, step: f64) -> El {
    El {
        element,
        kind: Kind::Instant,
        unit: "ccy",
        decimals: -6,
        base,
        step,
    }
}

const fn pure(element: &'static str, base: f64, step: f64) -> El {
    El {
        element,
        kind: Kind::Instant,
        unit: "Pure",
        decimals: 2,
        base,
        step,
    }
}

pub struct TickerSpec {
    pub ticker: &'static str,
    pub industry: &'static str,
    pub subindustry: &'static str,
    pub currency: &'static str,
    pub elements: Vec<El>,
    /// `(gross base, gross step, accumulated depreciation base, step)` of vessels in PPE.
    pub vessels: Option<(f64, f64, f64, f64)>,
}

/// Quarters on disk: 2022Q1 to 2026Q2.
pub const FIRST_FY: i32 = 2022;
pub const LAST: (i32, u32) = (2026, 2);

fn quarter_end(fy: i32, q: u32) -> String {
    match q {
        1 => format!("{fy}-03-31"),
        2 => format!("{fy}-06-30"),
        3 => format!("{fy}-09-30"),
        _ => format!("{fy}-12-31"),
    }
}

fn vessel_contexts(ticker: &str, end: &str) -> String {
    let ent = format!("{}_maker", ticker.to_lowercase());
    let ctx = |id: &str, amount: &str| {
        format!(
            "<context id=\"{id}\"><entity><identifier scheme=\"http://www.idx.co.id/xbrl\">{ent}</identifier></entity><period><instant>{end}</instant></period><scenario><xbrldi:explicitMember dimension=\"idx-cor:CarryingAmountPropertyPlantEquipmentAxis\">idx-cor:{amount}</xbrldi:explicitMember><xbrldi:explicitMember dimension=\"idx-cor:ClassesPropertyPlantEquipmentAxis\">idx-cor:VesselsDirectlyOwnedMember</xbrldi:explicitMember></scenario></context>"
        )
    };
    ctx(
        "CurrentYearInstant_VesselsGross",
        "CarryingAmountGrossMember",
    ) + &ctx(
        "CurrentYearInstant_VesselsAccDep",
        "CarryingAmountAccumulatedDepreciationMember",
    )
}

/// Writes every filing of `spec` under `dir`; returns how many files were written.
pub fn write_ticker(dir: &Path, spec: &TickerSpec) -> usize {
    let mut n = 0;
    for fy in FIRST_FY..=LAST.0 {
        for q in 1..=4u32 {
            if (fy, q) > LAST {
                break;
            }
            let k = ((fy - FIRST_FY) * 4) as f64 + (q - 1) as f64;
            let period = ["Q1", "Q2", "Q3", "FY"][(q - 1) as usize];
            let mut facts: Vec<F> = Vec::new();
            for e in &spec.elements {
                let unit = if e.unit == "ccy" {
                    spec.currency
                } else {
                    e.unit
                };
                let scale = if e.unit == "Pure" { 1.0 } else { 1e6 };
                let value = match e.kind {
                    Kind::Instant => (e.base + e.step * k) * scale,
                    Kind::Flow => {
                        // Year-to-date: the standalone quarters of this year so far.
                        let first_k = ((fy - FIRST_FY) * 4) as f64;
                        let mut ytd = 0.0;
                        let mut kk = first_k;
                        while kk <= k {
                            ytd += e.base + e.step * kk;
                            kk += 1.0;
                        }
                        ytd * scale
                    }
                };
                let ctx = match e.kind {
                    Kind::Instant => "CurrentYearInstant",
                    Kind::Flow => "CurrentYearDuration",
                };
                facts.push((
                    e.element,
                    ctx,
                    (value * 100.0).round() / 100.0,
                    e.decimals,
                    unit,
                ));
            }
            let mut extra_contexts = String::new();
            if let Some((gb, gs, ab, as_)) = spec.vessels {
                extra_contexts = vessel_contexts(spec.ticker, &quarter_end(fy, q));
                facts.push((
                    "PropertyPlantAndEquipment",
                    "CurrentYearInstant_VesselsGross",
                    (gb + gs * k) * 1e6,
                    -6,
                    spec.currency,
                ));
                facts.push((
                    "PropertyPlantAndEquipment",
                    "CurrentYearInstant_VesselsAccDep",
                    (ab + as_ * k) * 1e6,
                    -6,
                    spec.currency,
                ));
            }
            write(
                dir,
                &Spec {
                    ticker: spec.ticker,
                    fy,
                    period,
                    industry: spec.industry,
                    subindustry: spec.subindustry,
                    currency: spec.currency,
                    rounding: "Jutaan / In Million",
                    audited: period == "FY",
                    facts,
                    extra_contexts,
                },
            );
            n += 1;
        }
    }
    n
}

const GENERAL: &str = "Umum / General";

pub fn bank() -> TickerSpec {
    TickerSpec {
        ticker: "BANK",
        industry: "Keuangan dan Syariah / Financial and Sharia",
        subindustry: "G111. Banks",
        currency: "IDR",
        elements: vec![
            inst("WadiahDemandDeposits", 40.0, 1.0),
            inst("WadiahSavingsDeposits", 30.0, 1.0),
            inst("MudharabahCurrentAccount", 20.0, 0.5),
            inst("MudharabahSavingsDeposits", 50.0, 1.0),
            inst("MudharabahTimeDeposits", 80.0, 2.0),
            inst("MurabahahReceivablesThirdParties", 90.0, 2.0),
            inst("MurabahahReceivablesRelatedParties", 2.0, 0.0),
            inst("MusyarakahFinancingThirdParties", 60.0, 1.5),
            inst("MusyarakahFinancingRelatedParties", 1.0, 0.0),
            inst("MudharabahFinancingThirdParties", 5.0, 0.1),
            inst("IjarahReceivablesThirdParties", 3.0, 0.0),
            inst("QardhFundsThirdParties", 4.0, 0.0),
            inst("IstishnaReceivablesThirdParties", 1.0, 0.0),
            inst("AllowanceForImpairmentLossesForShariaReceivables", 6.0, 0.1),
            inst("MarketableSecuritiesThirdParties", 30.0, 0.5),
            inst(
                "PlacementsWithBankIndonesiaAndOtherBanksThirdParties",
                10.0,
                0.0,
            ),
            inst("Equity", 70.0, 1.0),
            inst("CashAndCashEquivalents", 15.0, 0.0),
            inst("Assets", 400.0, 8.0),
            pure("NonPerformingFinancingGross", 2.4, -0.02),
            pure("NonPerformingFinancingNet", 0.9, -0.01),
            flow("MurabahahAndIstishnaIncome", 4.0, 0.1),
            flow("MudharabahProfitSharing", 0.3, 0.0),
            flow("MusyrakahProfitSharing", 2.0, 0.05),
            flow("IjarahIncome", 0.1, 0.0),
            flow("UjrahIncomeFromQardhFinancing", 0.05, 0.0),
            flow("RevenueFromFundManagementAsMudharib", 7.0, 0.15),
            flow(
                "ThirdPartiesShareOnReturnOfTemporarySyirkahFunds",
                2.0,
                0.04,
            ),
            flow("AllowancesForImpairmentLossesOnEarningsAssets", 0.5, 0.01),
            flow("GeneralAndAdministrativeExpenses", 1.2, 0.02),
            flow("OtherOperatingExpenses", 1.0, 0.02),
            flow("OtherOperatingIncome", 0.8, 0.02),
            flow("ProfitLoss", 1.5, 0.05),
            flow("ProfitFromOperation", 1.9, 0.06),
            flow(
                "NetCashFlowsReceivedFromUsedInOperatingActivities",
                1.0,
                0.05,
            ),
        ],
        vessels: None,
    }
}

pub fn coal() -> TickerSpec {
    TickerSpec {
        ticker: "COAL",
        industry: GENERAL,
        subindustry: "A121. Coal Production",
        currency: "USD",
        elements: vec![
            flow("SalesAndRevenue", 120.0, 2.0),
            flow("CostOfSalesAndRevenue", 80.0, 1.0),
            flow("MiningCost", 50.0, 0.8),
            flow("RoyaltiesToGovernment", 12.0, 0.2),
            flow("GrossProfit", 40.0, 1.0),
            flow("ProfitLossBeforeIncomeTax", 20.0, 0.5),
            flow("ProfitLoss", 15.0, 0.4),
            flow("Depreciation", 8.0, 0.1),
            flow("Amortization", 1.0, 0.0),
            flow(
                "NetCashFlowsReceivedFromUsedInOperatingActivities",
                22.0,
                0.5,
            ),
            flow(
                "PaymentsForAcquisitionOfPropertyPlantAndEquipment",
                9.0,
                0.1,
            ),
            inst("NonCurrentDeferredStrippingCosts", 30.0, 0.5),
            inst("CurrentProvisionsForRestorationAndRehabilitation", 2.0, 0.0),
            inst(
                "NonCurrentProvisionsForRestorationAndRehabilitation",
                25.0,
                0.3,
            ),
            inst("LongTermBankLoans", 100.0, -1.0),
            inst("CurrentMaturitiesOfBankLoans", 10.0, 0.0),
            inst("LongTermBondsPayable", 50.0, 0.0),
            inst("CashAndCashEquivalents", 40.0, 1.0),
            inst("Equity", 300.0, 5.0),
            inst("Assets", 600.0, 6.0),
        ],
        vessels: None,
    }
}

pub fn heavy() -> TickerSpec {
    TickerSpec {
        ticker: "HEVY",
        industry: GENERAL,
        subindustry: "C141. Construction Machinery &amp; Heavy Vehicles",
        currency: "IDR",
        elements: vec![
            flow("SalesAndRevenue", 300.0, 3.0),
            flow("CostOfSalesAndRevenue", 230.0, 2.0),
            flow("GrossProfit", 70.0, 1.0),
            flow("ProfitLossBeforeIncomeTax", 40.0, 0.5),
            flow("ProfitLoss", 30.0, 0.4),
            flow("Depreciation", 12.0, 0.1),
            flow(
                "NetCashFlowsReceivedFromUsedInOperatingActivities",
                45.0,
                0.5,
            ),
            flow(
                "PaymentsForAcquisitionOfPropertyPlantAndEquipment",
                20.0,
                0.3,
            ),
            inst("CashAndCashEquivalents", 150.0, 2.0),
            inst("ShortTermBankLoans", 20.0, 0.0),
            inst("LongTermBankLoans", 60.0, 0.0),
            inst("CurrentMaturitiesOfBankLoans", 15.0, 0.0),
            inst("Equity", 400.0, 5.0),
        ],
        vessels: None,
    }
}

fn shipping(ticker: &'static str, industry: &'static str, scale: f64) -> TickerSpec {
    TickerSpec {
        ticker,
        industry,
        subindustry: "A112. Oil &amp; Gas Storage &amp; Distribution",
        currency: "IDR",
        elements: vec![
            flow("SalesAndRevenue", 50.0 * scale, 0.5 * scale),
            flow("CostOfSalesAndRevenue", 35.0 * scale, 0.3 * scale),
            flow("GrossProfit", 15.0 * scale, 0.2 * scale),
            flow("ProfitLossBeforeIncomeTax", 8.0 * scale, 0.1 * scale),
            flow("ProfitLoss", 6.0 * scale, 0.1 * scale),
            flow("Depreciation", 9.0 * scale, 0.05 * scale),
            flow(
                "NetCashFlowsReceivedFromUsedInOperatingActivities",
                14.0 * scale,
                0.1 * scale,
            ),
            flow(
                "PaymentsForAcquisitionOfPropertyAndEquipment",
                11.0 * scale,
                0.1 * scale,
            ),
            inst("CashAndCashEquivalents", 20.0 * scale, 0.0),
            inst("LongTermBankLoans", 90.0 * scale, -scale),
            inst("CurrentMaturitiesOfBankLoans", 12.0 * scale, 0.0),
            inst("Equity", 120.0 * scale, 1.0 * scale),
            inst("PropertyPlantAndEquipment", 200.0 * scale, 1.0 * scale),
        ],
        vessels: Some((260.0 * scale, 2.0 * scale, 80.0 * scale, 1.0 * scale)),
    }
}

pub fn ship() -> TickerSpec {
    shipping("SHIP", "Infrastruktur / Infrastructure", 1.0)
}

/// A second A112 company, SHIP's peer, filed under the general taxonomy.
pub fn tank() -> TickerSpec {
    shipping("TANK", GENERAL, 1.6)
}

pub fn dist() -> TickerSpec {
    TickerSpec {
        ticker: "DIST",
        industry: GENERAL,
        subindustry: "I211. Networking Equipment",
        currency: "IDR",
        elements: vec![
            flow("SalesAndRevenue", 500.0, 5.0),
            flow("CostOfSalesAndRevenue", 450.0, 4.0),
            flow("GrossProfit", 50.0, 1.0),
            flow("ProfitLossBeforeIncomeTax", 25.0, 0.5),
            flow("ProfitLoss", 19.0, 0.4),
            flow("SalesAndRevenueMoreThan10Percent", 200.0, 2.0),
            flow("Depreciation", 2.0, 0.0),
            flow(
                "NetCashFlowsReceivedFromUsedInOperatingActivities",
                15.0,
                0.3,
            ),
            flow(
                "PaymentsForAcquisitionOfPropertyPlantAndEquipment",
                3.0,
                0.0,
            ),
            inst("TradeReceivablesThirdParties", 180.0, 2.0),
            inst("TradeReceivablesRelatedParties", 5.0, 0.0),
            inst("Inventories", 120.0, 1.0),
            inst("TradePayablesThirdParties", 150.0, 1.0),
            inst("CashAndCashEquivalents", 60.0, 1.0),
            inst("ShortTermBankLoans", 30.0, 0.0),
            inst("Equity", 200.0, 3.0),
        ],
        vessels: None,
    }
}

/// An industry code no pack claims: only the core applies (A2.5).
pub fn none() -> TickerSpec {
    TickerSpec {
        ticker: "NONE",
        industry: GENERAL,
        subindustry: "Z999. Synthetic Unknown",
        currency: "IDR",
        elements: vec![
            flow("SalesAndRevenue", 80.0, 1.0),
            flow("GrossProfit", 20.0, 0.0),
            flow("ProfitLossFromOperatingActivities", 10.0, 0.0),
            flow("ProfitLoss", 7.0, 0.0),
            flow(
                "NetCashFlowsReceivedFromUsedInOperatingActivities",
                9.0,
                0.0,
            ),
            flow(
                "PaymentsForAcquisitionOfPropertyPlantAndEquipment",
                4.0,
                0.0,
            ),
            inst("CashAndCashEquivalents", 30.0, 0.0),
            inst("Equity", 90.0, 0.0),
            inst("LongTermBankLoans", 10.0, 0.0),
        ],
        vessels: None,
    }
}

pub fn all() -> Vec<TickerSpec> {
    vec![bank(), coal(), heavy(), ship(), tank(), dist(), none()]
}

/// Writes every ticker; returns the file count.
pub fn write_all(dir: &Path) -> usize {
    all().iter().map(|s| write_ticker(dir, s)).sum()
}
