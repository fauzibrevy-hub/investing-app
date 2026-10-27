//! XBRL parser and `idx-dei` header on the three synthetic taxonomy fixtures
//! (research plan §7.2 "parser on three synthetic taxonomies", PRD A1.1, A1.2).
//! Every value asserted here is the invented value written into the fixture.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use finance_app::research::xbrl::{self, AvailableSource, CtxKind, Fact, Period};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/research/xbrl")
}

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn find<'a>(facts: &'a [Fact], element: &str, kind: CtxKind, end: &str, dims: &str) -> &'a Fact {
    facts
        .iter()
        .find(|f| f.element == element && f.ctx_kind == kind && f.end == end && f.dims == dims)
        .unwrap_or_else(|| panic!("no fact {element} {kind:?} {end} [{dims}]"))
}

#[test]
fn general_interim_filing_header_and_sampled_facts() {
    let p = xbrl::parse_filing(&fixtures().join("general/AAAA_2025_Q2.xbrl")).unwrap();
    let f = &p.filing;
    assert_eq!(f.id, "AAAA_2025_Q2");
    assert_eq!(
        (f.ticker.as_str(), f.fy, f.period),
        ("AAAA", 2025, Period::Q2)
    );
    assert_eq!(f.taxonomy, "general");
    assert_eq!(f.industry_code.as_deref(), Some("C141"));
    assert_eq!(
        (f.period_start, f.period_end),
        (day("2025-01-01"), day("2025-06-30"))
    );
    assert!(!f.audited);
    assert_eq!(f.rounding, "Satuan Penuh");
    assert_eq!(f.currency, "USD");
    assert_eq!(f.filed_at, None);
    assert_eq!(f.available_from, day("2025-08-29"));
    assert_eq!(f.available_source, AvailableSource::Estimated);
    assert_eq!(f.file_hash.len(), 64);
    assert!(f.source_path.ends_with("AAAA_2025_Q2.xbrl"));
    assert_eq!(p.entity_name.as_deref(), Some("PT Contoh Umum Tbk"));

    // Twenty sampled facts, as the PDF check in A1.2 would do by hand.
    let facts = &p.facts;
    let samples: &[(&str, CtxKind, &str, f64)] = &[
        (
            "idx-cor:CashAndCashEquivalents",
            CtxKind::Instant,
            "2025-06-30",
            48_120_000.0,
        ),
        (
            "idx-cor:CashAndCashEquivalents",
            CtxKind::Instant,
            "2024-12-31",
            41_700_000.0,
        ),
        (
            "idx-cor:TradeReceivables",
            CtxKind::Instant,
            "2025-06-30",
            22_600_000.0,
        ),
        (
            "idx-cor:Inventories",
            CtxKind::Instant,
            "2025-06-30",
            30_500_000.0,
        ),
        (
            "idx-cor:Assets",
            CtxKind::Instant,
            "2025-06-30",
            410_300_000.0,
        ),
        (
            "idx-cor:Assets",
            CtxKind::Instant,
            "2024-12-31",
            388_000_000.0,
        ),
        (
            "idx-cor:Liabilities",
            CtxKind::Instant,
            "2025-06-30",
            150_200_000.0,
        ),
        (
            "idx-cor:Equity",
            CtxKind::Instant,
            "2025-06-30",
            260_100_000.0,
        ),
        (
            "idx-cor:Equity",
            CtxKind::Instant,
            "2024-12-31",
            245_500_000.0,
        ),
        (
            "idx-cor:Equity",
            CtxKind::Instant,
            "2023-12-31",
            230_000_000.0,
        ),
        (
            "idx-cor:LongTermBankLoans",
            CtxKind::Instant,
            "2025-06-30",
            60_000_000.0,
        ),
        (
            "idx-cor:SalesAndRevenue",
            CtxKind::Duration,
            "2025-06-30",
            180_400_000.0,
        ),
        (
            "idx-cor:SalesAndRevenue",
            CtxKind::Duration,
            "2024-06-30",
            165_200_000.0,
        ),
        (
            "idx-cor:SalesAndRevenue",
            CtxKind::Duration,
            "2024-12-31",
            340_900_000.0,
        ),
        (
            "idx-cor:CostOfSalesAndRevenue",
            CtxKind::Duration,
            "2025-06-30",
            -126_300_000.0,
        ),
        (
            "idx-cor:GrossProfit",
            CtxKind::Duration,
            "2025-06-30",
            54_100_000.0,
        ),
        (
            "idx-cor:ProfitLoss",
            CtxKind::Duration,
            "2025-06-30",
            22_150_000.0,
        ),
        (
            "idx-cor:ProfitLoss",
            CtxKind::Duration,
            "2024-12-31",
            41_300_000.0,
        ),
        (
            "idx-cor:NetCashFlowsReceivedFromUsedInOperatingActivities",
            CtxKind::Duration,
            "2025-06-30",
            26_700_000.0,
        ),
        (
            "idx-cor:AcquisitionOfPropertyPlantAndEquipment",
            CtxKind::Duration,
            "2025-06-30",
            -9_800_000.0,
        ),
    ];
    for (el, kind, end, value) in samples {
        let f = find(facts, el, *kind, end, "");
        assert_eq!(f.value, *value, "{el} {end}");
        assert_eq!(f.unit, "USD");
        assert_eq!(f.decimals, Some(0));
        if *kind == CtxKind::Duration {
            assert!(f.start.as_deref().unwrap().ends_with("-01-01"));
        } else {
            assert_eq!(f.start, None);
        }
    }
    let eps = find(
        facts,
        "idx-cor:BasicEarningsLossPerShareFromContinuingOperations",
        CtxKind::Duration,
        "2025-06-30",
        "",
    );
    assert_eq!(
        (eps.value, eps.unit.as_str(), eps.decimals),
        (0.0443, "USD/shares", Some(4))
    );
    let parent = find(
        facts,
        "idx-cor:Equity",
        CtxKind::Instant,
        "2025-06-30",
        "idx-cor:ComponentsOfEquityAxis=idx-cor:EquityAttributableToParentEntityMember",
    );
    assert_eq!(parent.value, 255_000_000.0);
    // nil OCI, segment text block, counterparty name → skipped, plus the non-numeric dei text.
    assert!(p.skipped_non_numeric >= 3);
    assert_eq!(p.conflicts, 0, "the repeated Cash line has the same value");
    assert_eq!(
        facts
            .iter()
            .filter(|f| f.element == "idx-cor:CashAndCashEquivalents" && f.end == "2025-06-30")
            .count(),
        1
    );
}

#[test]
fn infrastructure_annual_filing_is_audited_in_millions_with_segments() {
    let p = xbrl::parse_filing(&fixtures().join("infrastructure/BBBB_2024_FY.xbrl")).unwrap();
    let f = &p.filing;
    assert_eq!(f.id, "BBBB_2024_FY");
    assert_eq!(f.period, Period::FY);
    assert_eq!(f.taxonomy, "infrastructure");
    assert_eq!(f.industry_code.as_deref(), Some("J121"));
    assert!(f.audited);
    assert_eq!(f.rounding, "Jutaan");
    assert_eq!(f.currency, "IDR");
    assert_eq!(f.available_from, day("2025-03-31"));
    let facts = &p.facts;
    let samples: &[(&str, CtxKind, &str, &str, f64)] = &[
        (
            "idx-cor:CashAndCashEquivalents",
            CtxKind::Instant,
            "2024-12-31",
            "",
            95e6,
        ),
        (
            "idx-cor:CashAndCashEquivalents",
            CtxKind::Instant,
            "2023-12-31",
            "",
            80e6,
        ),
        ("idx-cor:Assets", CtxKind::Instant, "2024-12-31", "", 870e6),
        (
            "idx-cor:Liabilities",
            CtxKind::Instant,
            "2024-12-31",
            "",
            410e6,
        ),
        ("idx-cor:Equity", CtxKind::Instant, "2024-12-31", "", 460e6),
        ("idx-cor:Equity", CtxKind::Instant, "2023-12-31", "", 410e6),
        ("idx-cor:Equity", CtxKind::Instant, "2022-12-31", "", 372e6),
        ("idx-cor:Vessels", CtxKind::Instant, "2024-12-31", "", 520e6),
        (
            "idx-cor:Vessels",
            CtxKind::Instant,
            "2024-12-31",
            "idx-cor:SegmentsAxis=idx-cor:ShippingSegmentMember",
            500e6,
        ),
        (
            "idx-cor:LongTermBankLoans",
            CtxKind::Instant,
            "2024-12-31",
            "",
            300e6,
        ),
        (
            "idx-cor:SalesAndRevenue",
            CtxKind::Duration,
            "2024-12-31",
            "",
            640e6,
        ),
        (
            "idx-cor:SalesAndRevenue",
            CtxKind::Duration,
            "2023-12-31",
            "",
            560e6,
        ),
        (
            "idx-cor:SalesAndRevenue",
            CtxKind::Duration,
            "2024-12-31",
            "idx-cor:SegmentsAxis=idx-cor:ShippingSegmentMember",
            500e6,
        ),
        (
            "idx-cor:FreightAndHandlingCosts",
            CtxKind::Duration,
            "2024-12-31",
            "",
            -210e6,
        ),
        (
            "idx-cor:GrossProfit",
            CtxKind::Duration,
            "2024-12-31",
            "",
            230e6,
        ),
        (
            "idx-cor:ProfitLossFromOperatingActivities",
            CtxKind::Duration,
            "2024-12-31",
            "",
            120e6,
        ),
        (
            "idx-cor:ProfitLoss",
            CtxKind::Duration,
            "2024-12-31",
            "",
            84e6,
        ),
        (
            "idx-cor:ProfitLoss",
            CtxKind::Duration,
            "2023-12-31",
            "",
            70e6,
        ),
        (
            "idx-cor:NetCashFlowsReceivedFromUsedInOperatingActivities",
            CtxKind::Duration,
            "2024-12-31",
            "",
            150e6,
        ),
        (
            "idx-cor:DepreciationOfPropertyPlantAndEquipment",
            CtxKind::Duration,
            "2024-12-31",
            "",
            48e6,
        ),
    ];
    for (el, kind, end, dims, value) in samples {
        let f = find(facts, el, *kind, end, dims);
        assert_eq!(f.value, *value, "{el} {end} [{dims}]");
        assert_eq!((f.unit.as_str(), f.decimals), ("IDR", Some(-6)));
    }
    let eps = find(
        facts,
        "idx-cor:BasicEarningsLossPerShareFromContinuingOperations",
        CtxKind::Duration,
        "2024-12-31",
        "",
    );
    assert_eq!(
        (eps.value, eps.unit.as_str(), eps.decimals),
        (42.0, "IDR/shares", Some(0))
    );
    assert!(
        !facts.iter().any(|f| f.element == "idx-cor:DividendsPaid"),
        "nil dividends are not a fact"
    );
}

#[test]
fn financial_sharia_interim_filing_with_financing_dimensions_and_ratio_tags() {
    let p = xbrl::parse_filing(&fixtures().join("financial_sharia/CCCC_2025_Q3.xbrl")).unwrap();
    let f = &p.filing;
    assert_eq!(f.id, "CCCC_2025_Q3");
    assert_eq!(f.period, Period::Q3);
    assert_eq!(f.taxonomy, "financial_sharia");
    assert_eq!(f.industry_code.as_deref(), Some("G111"));
    assert!(!f.audited, "limited review is not an audit");
    assert_eq!(f.available_from, day("2025-10-30"));
    let facts = &p.facts;
    let samples: &[(&str, CtxKind, &str, &str, f64)] = &[
        ("idx-cor:Cash", CtxKind::Instant, "2025-09-30", "", 12e6),
        (
            "idx-cor:CurrentAccountsWithBankIndonesia",
            CtxKind::Instant,
            "2025-09-30",
            "",
            40e6,
        ),
        (
            "idx-cor:WadiahDemandDeposits",
            CtxKind::Instant,
            "2025-09-30",
            "",
            90e6,
        ),
        (
            "idx-cor:WadiahDemandDeposits",
            CtxKind::Instant,
            "2024-12-31",
            "",
            85e6,
        ),
        (
            "idx-cor:MudharabahCurrentAccount",
            CtxKind::Instant,
            "2025-09-30",
            "",
            30e6,
        ),
        (
            "idx-cor:MudharabahSavingsDeposits",
            CtxKind::Instant,
            "2025-09-30",
            "",
            160e6,
        ),
        (
            "idx-cor:MudharabahTimeDeposits",
            CtxKind::Instant,
            "2025-09-30",
            "",
            220e6,
        ),
        (
            "idx-cor:DepositsFromCustomers",
            CtxKind::Instant,
            "2025-09-30",
            "",
            500e6,
        ),
        (
            "idx-cor:DepositsFromCustomers",
            CtxKind::Instant,
            "2024-12-31",
            "",
            472e6,
        ),
        (
            "idx-cor:Financing",
            CtxKind::Instant,
            "2025-09-30",
            "",
            430e6,
        ),
        (
            "idx-cor:Financing",
            CtxKind::Instant,
            "2025-09-30",
            "idx-cor:TypeOfFinancingAxis=idx-cor:MurabahahMember",
            300e6,
        ),
        (
            "idx-cor:Financing",
            CtxKind::Instant,
            "2025-09-30",
            "idx-cor:TypeOfFinancingAxis=idx-cor:MusyarakahMember",
            130e6,
        ),
        (
            "idx-cor:AllowanceForImpairmentLossesFinancing",
            CtxKind::Instant,
            "2025-09-30",
            "",
            -14e6,
        ),
        ("idx-cor:Assets", CtxKind::Instant, "2025-09-30", "", 640e6),
        ("idx-cor:Equity", CtxKind::Instant, "2025-09-30", "", 72e6),
        (
            "idx-cor:IncomeFromFundManagementAsMudharib",
            CtxKind::Duration,
            "2025-09-30",
            "",
            39e6,
        ),
        (
            "idx-cor:IncomeFromFundManagementAsMudharib",
            CtxKind::Duration,
            "2024-09-30",
            "",
            35e6,
        ),
        (
            "idx-cor:IncomeFromFundManagementAsMudharib",
            CtxKind::Duration,
            "2024-12-31",
            "",
            47e6,
        ),
        (
            "idx-cor:ThirdPartiesShareOnReturnOfTemporarySyirkahFunds",
            CtxKind::Duration,
            "2025-09-30",
            "",
            -12e6,
        ),
        (
            "idx-cor:ProfitLoss",
            CtxKind::Duration,
            "2025-09-30",
            "",
            9e6,
        ),
    ];
    for (el, kind, end, dims, value) in samples {
        let f = find(facts, el, *kind, end, dims);
        assert_eq!(f.value, *value, "{el} {end} [{dims}]");
        assert_eq!((f.unit.as_str(), f.decimals), ("IDR", Some(-6)));
    }
    let npf = find(
        facts,
        "idx-cor:NonPerformingFinancingGross",
        CtxKind::Instant,
        "2025-09-30",
        "",
    );
    assert_eq!(
        (npf.value, npf.unit.as_str(), npf.decimals),
        (2.35, "pure", Some(2))
    );
    assert!(
        !facts
            .iter()
            .any(|f| f.element == "idx-cor:MudharabahTimeDepositsInterestRate"),
        "a rate range written as text is not a number"
    );
    assert!(!facts.iter().any(|f| f.element.ends_with("TextBlock")));
}

const NPF_TAG: &str = r#"<idx-cor:NonPerformingFinancingGross id="IXF4220000E02_0700_00001_01_0001" decimals="2" contextRef="CurrentYearInstant">2.35<"#;

/// The sharia fixture with its NPF fact written the way a bank's export delivered it:
/// typed `IDR`, ÷ 10⁶ applied three times to a fraction (BRIS 2026 Q1: `1.79e-20` for 1,79 %).
fn sharia_with_money_typed_ratio(dir: &Path, value: &str) -> PathBuf {
    let text =
        std::fs::read_to_string(fixtures().join("financial_sharia/CCCC_2025_Q3.xbrl")).unwrap();
    assert!(text.contains(NPF_TAG), "fixture drifted");
    let tag = format!(
        r#"<idx-cor:NonPerformingFinancingGross id="IXF4220000E02_0700_00001_01_0001" decimals="INF" contextRef="CurrentYearInstant" unitRef="IDR">{value}<"#
    );
    let path = dir.join("CCCC_2025_Q3.xbrl");
    std::fs::write(&path, text.replace(NPF_TAG, &tag)).unwrap();
    path
}

#[test]
fn a_ratio_typed_as_money_is_stored_as_percent_points_under_pure() {
    let dir = tempfile::tempdir().unwrap();
    for (filed, percent) in [
        ("0.0000000000000000000179", 1.79),
        ("0.00000000000184", 1.84),
        ("0.0000019", 1.9),
        ("0.018", 1.8),
        ("2.21", 2.21),
    ] {
        let path = sharia_with_money_typed_ratio(dir.path(), filed);
        let p = xbrl::parse_filing(&path).unwrap();
        let npf = find(
            &p.facts,
            "idx-cor:NonPerformingFinancingGross",
            CtxKind::Instant,
            "2025-09-30",
            "",
        );
        assert_eq!(npf.unit, "pure", "{filed}");
        assert!(
            (npf.value - percent).abs() < 1e-9,
            "{filed} -> {}",
            npf.value
        );
        // money facts of the same filing keep their IDR unit and value
        let profit = find(
            &p.facts,
            "idx-cor:ProfitLoss",
            CtxKind::Duration,
            "2025-09-30",
            "",
        );
        assert_eq!((profit.value, profit.unit.as_str()), (9e6, "IDR"));
    }
}

#[test]
fn reingest_rebuilds_filings_that_the_plain_pass_skips_as_unchanged() {
    use finance_app::research::store::Store;
    let dir = tempfile::tempdir().unwrap();
    let xbrl_dir = dir.path().join("xbrl");
    std::fs::create_dir(&xbrl_dir).unwrap();
    sharia_with_money_typed_ratio(&xbrl_dir, "0.0000000000000000000179");
    let store = Store::open(&dir.path().join("research.sqlite")).unwrap();
    let now = chrono::Utc::now();

    let first = xbrl::ingest_new(&store, &xbrl_dir, now).unwrap();
    assert_eq!(first.ingested.len(), 1);
    let id = &first.ingested[0];
    let stored = |s: &Store| {
        s.facts_of_filing(id, Some("idx-cor:NonPerformingFinancingGross"))
            .unwrap()[0]
            .value
    };
    assert!((stored(&store) - 1.79).abs() < 1e-9);

    let plain = xbrl::ingest_new(&store, &xbrl_dir, now).unwrap();
    assert_eq!((plain.ingested.len(), plain.unchanged), (0, 1));
    let again = xbrl::reingest_all(&store, &xbrl_dir, now).unwrap();
    assert_eq!((again.ingested.len(), again.unchanged), (1, 0));
    assert!((stored(&store) - 1.79).abs() < 1e-9);
}

#[test]
fn an_unreadable_file_is_an_error_not_an_empty_filing() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("ZZZZ_2025_Q1.xbrl");
    std::fs::write(&bad, "<html><body>IDX download page</body></html>").unwrap();
    let err = xbrl::parse_filing(&bad).unwrap_err();
    assert!(err.to_string().contains("not an XBRL instance"), "{err}");

    let headless = dir.path().join("YYYY_2025_Q1.xbrl");
    std::fs::write(
        &headless,
        r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:c="http://www.idx.co.id/xbrl/taxonomy/2020-01-01/cor">
<context id="c"><entity><identifier scheme="s">x</identifier></entity><period><instant>2025-03-31</instant></period></context>
<c:Cash contextRef="c" decimals="0">1</c:Cash></xbrl>"#,
    )
    .unwrap();
    let err = xbrl::parse_filing(&headless).unwrap_err();
    assert!(err.to_string().contains("idx-dei header has no"), "{err}");
    assert!(xbrl::parse_filing(&dir.path().join("missing.xbrl")).is_err());
}

/// Read-only smoke run over a real directory; prints counts only, never values.
/// `FINANCE_IDX_XBRL_DIR=... cargo test --test research_xbrl -- --ignored --nocapture`.
#[test]
#[ignore]
fn smoke_real_directory() {
    let Some(dir) = std::env::var_os("FINANCE_IDX_XBRL_DIR") else {
        return;
    };
    let mut files = Vec::new();
    let mut stack = vec![PathBuf::from(dir)];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "xbrl") {
                files.push(p);
            }
        }
    }
    files.sort();
    let (mut ok, mut failed, mut facts, mut skipped, mut conflicts) =
        (0, 0, 0usize, 0usize, 0usize);
    let mut taxonomies = std::collections::BTreeMap::new();
    for f in &files {
        match xbrl::parse_filing(f) {
            Ok(p) => {
                ok += 1;
                facts += p.facts.len();
                skipped += p.skipped_non_numeric;
                conflicts += p.conflicts;
                *taxonomies.entry(p.filing.taxonomy.clone()).or_insert(0) += 1;
            }
            Err(e) => {
                failed += 1;
                eprintln!("UNPARSED {}: {e}", f.display());
            }
        }
    }
    eprintln!(
        "parsed {ok} of {} files, {failed} failed; facts {facts}, skipped non-numeric {skipped}, conflicting duplicates {conflicts}; taxonomies {taxonomies:?}",
        files.len()
    );
}
