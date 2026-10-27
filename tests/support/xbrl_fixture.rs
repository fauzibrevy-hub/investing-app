//! Generates synthetic IDX-shaped `.xbrl` files for integration tests (research plan §7.2
//! "ingest of a real-shaped fixture directory"). Every number is invented and stays under
//! ten digits. Used by `tests/research_facts.rs` and `tests/research_scheduler.rs`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// One fact to write: `(element local name, context id, value, decimals, unit id)`.
pub type F = (&'static str, &'static str, f64, i32, &'static str);

pub struct Spec {
    pub ticker: &'static str,
    pub fy: i32,
    /// `Q1`, `Q2`, `Q3` or `FY`.
    pub period: &'static str,
    pub industry: &'static str,
    pub subindustry: &'static str,
    pub currency: &'static str,
    pub rounding: &'static str,
    pub audited: bool,
    pub facts: Vec<F>,
    /// Raw `<context>` elements appended after the standard six (dimensioned instants).
    pub extra_contexts: String,
}

fn quarter_end(fy: i32, period: &str) -> String {
    match period {
        "Q1" => format!("{fy}-03-31"),
        "Q2" => format!("{fy}-06-30"),
        "Q3" => format!("{fy}-09-30"),
        _ => format!("{fy}-12-31"),
    }
}

/// The contexts of an IDX filing: current and prior year-to-date, prior full year, and the
/// three instants, under the names the real files use.
fn contexts(ticker: &str, fy: i32, period: &str) -> String {
    let ent = format!("{}_maker", ticker.to_lowercase());
    let end = quarter_end(fy, period);
    let prev_end = quarter_end(fy - 1, period);
    let ctx = |id: &str, body: String| {
        format!(
            "<context id=\"{id}\"><entity><identifier scheme=\"http://www.idx.co.id/xbrl\">{ent}</identifier></entity><period>{body}</period></context>"
        )
    };
    let mut s = String::new();
    s += &ctx("CurrentYearInstant", format!("<instant>{end}</instant>"));
    s += &ctx(
        "PriorEndYearInstant",
        format!("<instant>{}-12-31</instant>", fy - 1),
    );
    s += &ctx(
        "Prior2YearsInstant",
        format!("<instant>{}-12-31</instant>", fy - 2),
    );
    s += &ctx(
        "CurrentYearDuration",
        format!("<startDate>{fy}-01-01</startDate><endDate>{end}</endDate>"),
    );
    s += &ctx(
        "PriorYearDuration",
        format!(
            "<startDate>{}-01-01</startDate><endDate>{prev_end}</endDate>",
            fy - 1
        ),
    );
    if period != "FY" {
        s += &ctx(
            "PriorEndYearDuration",
            format!(
                "<startDate>{}-01-01</startDate><endDate>{}-12-31</endDate>",
                fy - 1,
                fy - 1
            ),
        );
    }
    s
}

pub fn render(spec: &Spec) -> String {
    let period_text = match spec.period {
        "Q1" => "Kuartal I / First Quarter",
        "Q2" => "Kuartal II / Second Quarter",
        "Q3" => "Kuartal III / Third Quarter",
        _ => "Tahunan / Annual",
    };
    let report = if spec.audited {
        "Diaudit / Audited"
    } else {
        "Tidak Diaudit / Unaudit"
    };
    let cur = spec.currency;
    let end = quarter_end(spec.fy, spec.period);
    let dei = [
        ("EntityCode", spec.ticker.to_string()),
        ("EntityName", format!("PT {} Sintetis Tbk", spec.ticker)),
        ("EntityMainIndustry", spec.industry.to_string()),
        ("Subindustry", spec.subindustry.to_string()),
        (
            "PeriodOfFinancialStatementsSubmissions",
            period_text.to_string(),
        ),
        ("CurrentPeriodStartDate", format!("{}-01-01", spec.fy)),
        ("CurrentPeriodEndDate", end.clone()),
        (
            "DescriptionOfPresentationCurrency",
            format!("Mata uang / {cur}"),
        ),
        (
            "LevelOfRoundingUsedInFinancialStatements",
            spec.rounding.to_string(),
        ),
        ("TypeOfReportOnFinancialStatements", report.to_string()),
    ];
    let mut s = String::from(
        r#"<?xml version="1.0"?>
<xbrl xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:idx-dei="http://www.idx.co.id/xbrl/taxonomy/2020-01-01/dei" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns="http://www.xbrl.org/2003/instance" xmlns:iso4217="http://www.xbrl.org/2003/iso4217" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:idx-cor="http://www.idx.co.id/xbrl/taxonomy/2020-01-01/cor" xmlns:xbrldi="http://xbrl.org/2006/xbrldi"><link:schemaRef xlink:type="simple" xlink:href="Taxonomy.xsd"/>"#,
    );
    s += &format!("<unit id=\"{cur}\"><measure>iso4217:{cur}</measure></unit>");
    s += &format!("<unit id=\"{cur}PerShares\"><divide><unitNumerator><measure>iso4217:{cur}</measure></unitNumerator><unitDenominator><measure>shares</measure></unitDenominator></divide></unit>");
    s += "<unit id=\"Shares\"><measure>shares</measure></unit>";
    s += "<unit id=\"Pure\"><measure>pure</measure></unit>";
    s += &contexts(spec.ticker, spec.fy, spec.period);
    s += &spec.extra_contexts;
    for (i, (k, v)) in dei.iter().enumerate() {
        s += &format!(
            "<idx-dei:{k} id=\"IXF1000000E01_{:04}_00001_00_0001\" contextRef=\"CurrentYearInstant\">{v}</idx-dei:{k}>",
            i + 1
        );
    }
    for (i, (el, ctx, value, decimals, unit)) in spec.facts.iter().enumerate() {
        let text = if value.fract() == 0.0 {
            format!("{}", *value as i64)
        } else {
            value.to_string()
        };
        s += &format!(
            "<idx-cor:{el} id=\"IXF4220000E02_{:04}_00001_01_0001\" decimals=\"{decimals}\" contextRef=\"{ctx}\" unitRef=\"{unit}\">{text}</idx-cor:{el}>",
            600 + i
        );
    }
    s += "<idx-cor:NotesTextBlock contextRef=\"CurrentYearDuration\">&lt;p&gt;Teks sintetis.&lt;/p&gt;</idx-cor:NotesTextBlock>";
    s += &format!("<idx-cor:OtherComprehensiveIncome xsi:nil=\"true\" contextRef=\"CurrentYearDuration\" unitRef=\"{cur}\"/>");
    s += "</xbrl>\n";
    s
}

/// Writes the filing at `<dir>/<fy>/<period>/<TICKER>_<fy>_<period>.xbrl` like the scraper.
pub fn write(dir: &Path, spec: &Spec) -> PathBuf {
    let sub = dir.join(spec.fy.to_string()).join(spec.period);
    std::fs::create_dir_all(&sub).unwrap();
    let path = sub.join(format!("{}_{}_{}.xbrl", spec.ticker, spec.fy, spec.period));
    std::fs::write(&path, render(spec)).unwrap();
    path
}

/// Year-to-date revenue and profit plus equity for one ticker across quarters, in millions.
pub struct Year {
    pub fy: i32,
    /// YTD revenue Q1..FY.
    pub revenue: [f64; 4],
    /// YTD profit Q1..FY.
    pub profit: [f64; 4],
    /// Equity at each quarter end.
    pub equity: [f64; 4],
}

/// Writes the four filings of `year` for a general-taxonomy ticker in IDR millions, each
/// carrying the prior-year comparatives from `prev` (when given). `revenue_override`
/// replaces the comparative YTD revenue the filing reports for a prior quarter, to plant a
/// restatement: `(quarter, value)`.
pub fn general_year(
    dir: &Path,
    ticker: &'static str,
    year: &Year,
    prev: Option<&Year>,
    skip: &[&str],
    restate_prev_revenue: Option<(usize, f64)>,
) -> Vec<PathBuf> {
    const M: f64 = 1e6;
    let mut out = Vec::new();
    for (qi, period) in ["Q1", "Q2", "Q3", "FY"].into_iter().enumerate() {
        if skip.contains(&period) {
            continue;
        }
        let mut facts: Vec<F> = vec![
            (
                "SalesAndRevenue",
                "CurrentYearDuration",
                year.revenue[qi] * M,
                -6,
                "IDR",
            ),
            (
                "ProfitLoss",
                "CurrentYearDuration",
                year.profit[qi] * M,
                -6,
                "IDR",
            ),
            (
                "Equity",
                "CurrentYearInstant",
                year.equity[qi] * M,
                -6,
                "IDR",
            ),
            (
                "Assets",
                "CurrentYearInstant",
                year.equity[qi] * 2.0 * M,
                -6,
                "IDR",
            ),
            (
                "NetCashFlowsReceivedFromUsedInOperatingActivities",
                "CurrentYearDuration",
                year.profit[qi] * 1.2 * M,
                -6,
                "IDR",
            ),
            (
                "BasicEarningsLossPerShareFromContinuingOperations",
                "CurrentYearDuration",
                year.profit[qi] / 10.0,
                0,
                "IDRPerShares",
            ),
        ];
        if let Some(p) = prev {
            let mut rev = p.revenue[qi];
            if let Some((rq, v)) = restate_prev_revenue {
                if rq == qi {
                    rev = v;
                }
            }
            facts.push(("SalesAndRevenue", "PriorYearDuration", rev * M, -6, "IDR"));
            facts.push((
                "ProfitLoss",
                "PriorYearDuration",
                p.profit[qi] * M,
                -6,
                "IDR",
            ));
            facts.push(("Equity", "PriorEndYearInstant", p.equity[3] * M, -6, "IDR"));
            facts.push((
                "Assets",
                "PriorEndYearInstant",
                p.equity[3] * 2.0 * M,
                -6,
                "IDR",
            ));
            if period != "FY" {
                facts.push((
                    "SalesAndRevenue",
                    "PriorEndYearDuration",
                    p.revenue[3] * M,
                    -6,
                    "IDR",
                ));
                facts.push((
                    "ProfitLoss",
                    "PriorEndYearDuration",
                    p.profit[3] * M,
                    -6,
                    "IDR",
                ));
            }
        }
        out.push(write(
            dir,
            &Spec {
                ticker,
                fy: year.fy,
                period,
                industry: "Umum / General",
                subindustry: "C141. Construction Machinery &amp; Heavy Vehicles",
                currency: "IDR",
                rounding: "Jutaan / In Million",
                audited: period == "FY",
                facts,
                extra_contexts: String::new(),
            },
        ));
    }
    out
}
