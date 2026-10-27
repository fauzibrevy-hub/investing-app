//! Pack section, packs list and reload, coverage and health (research plan §5, §7.2
//! golden; prd.md A2.1 to A2.5) over seven synthetic tickers across the five packs plus one
//! with no pack. Golden payloads live in `tests/fixtures/research/packs/<TICKER>.json`;
//! `UPDATE_GOLDEN=1` rewrites them.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

mod common;

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::{TimeZone, Utc};
use common::{json_of, TestApp};
use investing_app::research::kpi::KpiValue;
use investing_app::research::packs::Packs;
use investing_app::research::store::Store;
use investing_app::research::universe::{add, sync, AddRequest, HeldPosition, Role};
use investing_app::research::xbrl;
use serde_json::{json, Value};

const NOW: &str = "2026-10-24T09:00:00+00:00";
const HELD: [&str; 6] = ["BANK", "COAL", "HEVY", "SHIP", "DIST", "NONE"];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/research/packs")
}

fn kpi(ticker: &str, metric: &str, fy: i32, q: u32, value: Option<f64>, unit: &str) -> KpiValue {
    KpiValue {
        ticker: ticker.into(),
        metric: metric.into(),
        fy,
        q,
        value,
        unknown: value.is_none(),
        unit: unit.into(),
        document: format!("Laporan operasional {fy}Q{q} sintetis"),
        document_date: format!("{fy}-{:02}-28", q * 3 + 1),
        entered_at: NOW.into(),
    }
}

/// Filings for all seven tickers ingested into a temp store, universe seeded (six held,
/// TANK as SHIP's peer), KPIs for COAL and SHIP, then the app over that store.
fn seed(dir: &Path) -> TestApp {
    let xbrl_dir = dir.join("XBRL");
    assert_eq!(pack_fixture::write_all(&xbrl_dir), 7 * 18);
    let db = dir.join("research.sqlite");
    {
        let store = Store::open(&db).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 10, 24, 9, 0, 0).unwrap();
        let report = xbrl::ingest_new(&store, &xbrl_dir, now).unwrap();
        assert_eq!(report.ingested.len(), 7 * 18, "{:?}", report.unparsed);
        assert!(report.unparsed.is_empty());
        assert!(
            report.reconciliation_failures.is_empty(),
            "{:?}",
            report.reconciliation_failures
        );
        let held: Vec<HeldPosition> = HELD
            .iter()
            .map(|t| HeldPosition {
                ticker: (*t).into(),
                avg_price: 1_000.0,
            })
            .collect();
        sync(&store, &held, NOW).unwrap();
        let packs = Packs::embedded().set();
        add(
            &store,
            &packs,
            &AddRequest {
                ticker: "TANK".into(),
                role: Some(Role::Peer),
                peer_of: Some("SHIP".into()),
                ..Default::default()
            },
            NOW,
        )
        .unwrap();
        // COAL: eight quarters of volumes and prices, one explicit unknown, one annual.
        for (i, (fy, q)) in [
            (2024, 3),
            (2024, 4),
            (2025, 1),
            (2025, 2),
            (2025, 3),
            (2025, 4),
            (2026, 1),
            (2026, 2),
        ]
        .into_iter()
        .enumerate()
        {
            let k = i as f64;
            store
                .set_kpi(&kpi(
                    "COAL",
                    "production_mt",
                    fy,
                    q,
                    Some(6.0 + 0.25 * k),
                    "Mt",
                ))
                .unwrap();
            store
                .set_kpi(&kpi("COAL", "sales_mt", fy, q, Some(5.8 + 0.25 * k), "Mt"))
                .unwrap();
            store
                .set_kpi(&kpi("COAL", "asp_usd_t", fy, q, Some(70.0 - k), "USD/t"))
                .unwrap();
        }
        store
            .set_kpi(&kpi("COAL", "cash_cost_usd_t", 2026, 2, None, "USD/t"))
            .unwrap();
        store
            .set_kpi(&kpi("COAL", "reserves_mt", 2025, 4, Some(420.0), "Mt"))
            .unwrap();
        // SHIP and TANK: vessel counts so the per-vessel metrics compute.
        for (t, n) in [("SHIP", 12.0), ("TANK", 20.0)] {
            for (fy, q) in [(2025, 3), (2025, 4), (2026, 1), (2026, 2)] {
                store
                    .set_kpi(&kpi(t, "vessel_count", fy, q, Some(n), "kapal"))
                    .unwrap();
            }
        }
    }
    common::test_app_with(|cfg| {
        cfg.idx_xbrl_dir = xbrl_dir.clone();
        cfg.research_db_path = db.clone();
    })
}

/// Golden form: `as_of` is clock-dependent, operand lists are long, and rupiah amounts of
/// a billion or more are scaled down by 1e6 so no fixture holds a ten-digit run.
fn golden_form(v: &Value) -> Value {
    fn walk(v: &Value) -> Value {
        match v {
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, x)| {
                        let y = match k.as_str() {
                            "as_of" => json!("<dynamic>"),
                            "inputs" => json!(x.as_array().map_or(0, Vec::len)),
                            _ => walk(x),
                        };
                        (k.clone(), y)
                    })
                    .collect(),
            ),
            Value::Array(a) => Value::Array(a.iter().map(walk).collect()),
            Value::Number(n) if n.as_f64().is_some_and(|x| x.abs() >= 1e9) => {
                json!((n.as_f64().unwrap() / 1e6 * 1e4).round() / 1e4)
            }
            Value::Number(n) => match n.as_f64() {
                Some(x) if x.fract() != 0.0 => json!((x * 1e6).round() / 1e6),
                _ => v.clone(),
            },
            other => other.clone(),
        }
    }
    walk(v)
}

fn golden(name: &str, actual: &Value) {
    let path = fixtures().join(format!("{name}.json"));
    let actual = golden_form(actual);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(fixtures()).unwrap();
        let mut text = serde_json::to_string_pretty(&actual).unwrap();
        text.push('\n');
        std::fs::write(&path, text).unwrap();
        return;
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing fixture {name}.json (run with UPDATE_GOLDEN=1)"));
    let expected: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        expected, actual,
        "{name}.json differs; re-run with UPDATE_GOLDEN=1 if the change is intended"
    );
}

async fn call(
    t: &TestApp,
    method: Method,
    uri: &str,
    key: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", t.bearer())
        .header("idempotency-key", key);
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    json_of(t.send(req.body(body).unwrap()).await).await
}

fn metric<'a>(pack: &'a Value, id: &str) -> &'a Value {
    pack["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["metrics"].as_array().unwrap())
        .find(|m| m["id"] == id)
        .unwrap_or_else(|| panic!("no metric {id}"))
}

fn metric_ids(pack: &Value) -> Vec<String> {
    pack["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["metrics"].as_array().unwrap())
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn golden_pack_payloads_for_seven_tickers() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let long = regex::Regex::new("[0-9]{10,}").unwrap();
    for ticker in ["BANK", "COAL", "HEVY", "SHIP", "TANK", "DIST", "NONE"] {
        let (s, v) = t.get(&format!("/api/research/{ticker}/pack")).await;
        assert_eq!(s, StatusCode::OK, "{ticker}: {v}");
        assert_eq!(v["ticker"], ticker);
        assert_eq!(v["quarters"].as_array().unwrap().len(), 12);
        assert_eq!(v["latest"]["label"], "2026Q2");
        for m in v["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|d| d["metrics"].as_array().unwrap())
        {
            if m["layer"] != "driver" {
                assert_eq!(
                    m["cells"].as_array().unwrap().len(),
                    12,
                    "{ticker} {}",
                    m["id"]
                );
            }
        }
        golden(ticker, &v);
        let text = golden_form(&v).to_string();
        assert!(
            !long.is_match(&text),
            "{ticker}: ten-digit run in the golden"
        );
    }
}

#[tokio::test]
async fn a2_1_bank_shows_the_sharia_metrics_with_twelve_quarters_and_percentiles() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let (s, v) = t.get("/api/research/BANK/pack").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["pack"]["id"], "bank_sharia");
    assert_eq!(v["pack"]["source"], "filing");
    assert_eq!(v["pack"]["filing_code"], "G111");
    for id in [
        "casa_ratio",
        "fdr",
        "npf_gross",
        "npf_net",
        "coverage",
        "cost_of_funds",
        "financing_yield",
        "net_margin_earning_assets",
        "cost_to_income",
        "provision_cost",
    ] {
        let m = metric(&v, id);
        let cells = m["cells"].as_array().unwrap();
        assert_eq!(cells.len(), 12, "{id}");
        let last = cells.last().unwrap();
        assert!(last["value"].is_number(), "{id} latest: {last}");
        assert!(
            m["percentile"]["value"].is_number(),
            "{id}: {}",
            m["percentile"]
        );
        assert!(m["latest"]["value"].is_number(), "{id}");
        assert_eq!(m["unit"], "%", "{id}");
    }
    // CASA = (40+30+20+50)/(40+30+20+50+80) at k = 0, drifting with the steps.
    let casa = metric(&v, "casa_ratio");
    let k = 17.0;
    let num = (40.0 + k) + (30.0 + k) + (20.0 + 0.5 * k) + (50.0 + k);
    let den = num + (80.0 + 2.0 * k);
    let want = 100.0 * num / den;
    assert!((casa["latest"]["value"].as_f64().unwrap() - want).abs() < 1e-6);
    assert_eq!(casa["yoy"]["kind"], "pp");
    // NPF comes from a ratio tag in percent and sits inside its declared range.
    let npf = metric(&v, "npf_gross");
    assert!((npf["latest"]["value"].as_f64().unwrap() - (2.4 - 0.02 * 17.0)).abs() < 1e-9);
    assert_eq!(npf["cells"][11]["flags"], json!([]));
    // CAR and Tier 1 are KPI rows, quarterly, not entered yet: a gap with the Isi action.
    for id in ["car", "tier1"] {
        let m = metric(&v, id);
        assert_eq!(m["layer"], "kpi");
        assert_eq!(m["cadence"], "quarterly");
        assert_eq!(m["cells"][11]["state"], "not_entered");
        assert_eq!(m["cells"][11]["action"], "Isi");
    }
    // Drivers without a source are listed, never hidden.
    let missing: Vec<&str> = v["missing_drivers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(missing, ["sbn_10y"], "bi_rate has a source since R3.3");
    assert!(v["coverage"]["kpi"]["missing"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m == "car 2026Q2"));
}

#[tokio::test]
async fn a2_2_each_industry_pack_is_populated_at_ninety_percent_or_more() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    for (ticker, pack, must) in [
        (
            "COAL",
            "coal_mining",
            vec![
                "mining_cost_ratio",
                "royalty_ratio",
                "net_debt_to_ebitda",
                "mine_life_years",
                "deferred_stripping_vs_production",
            ],
        ),
        (
            "HEVY",
            "heavy_equipment_contracting",
            vec!["capex_to_depreciation", "net_cash"],
        ),
        (
            "SHIP",
            "shipping",
            vec![
                "revenue_per_vessel",
                "ebitda_per_vessel",
                "vessels_gross",
                "cost_of_revenue_ratio",
            ],
        ),
        (
            "DIST",
            "it_distribution",
            vec![
                "gross_margin",
                "receivable_days",
                "inventory_days",
                "payable_days",
                "cash_conversion_cycle",
                "customers_over_10pct_share",
            ],
        ),
    ] {
        let (s, v) = t.get(&format!("/api/research/{ticker}/pack")).await;
        assert_eq!(s, StatusCode::OK, "{ticker}");
        assert_eq!(v["pack"]["id"], pack, "{ticker}");
        let pct = v["coverage"]["xbrl"]["pct"].as_f64().unwrap();
        assert!(
            pct >= 90.0,
            "{ticker}: xbrl coverage {pct}% {}",
            v["coverage"]
        );
        for id in must {
            let m = metric(&v, id);
            assert!(
                m["latest"]["value"].is_number(),
                "{ticker} {id}: {}",
                m["cells"][11]
            );
        }
    }
    // COAL files in USD: a currency metric stays a currency metric, no IDR mix.
    let (_, coal) = t.get("/api/research/COAL/pack").await;
    let rev = metric(&coal, "revenue");
    assert!((rev["latest"]["value"].as_f64().unwrap() - (120.0 + 2.0 * 17.0) * 1e6).abs() < 1.0);
    // The unknown KPI is a cell of its own, distinct from the ones never entered.
    let cc = metric(&coal, "cash_cost_usd_t");
    assert_eq!(cc["cells"][11]["state"], "unknown");
    assert_eq!(cc["cells"][10]["state"], "not_entered");
    assert!(cc["latest"].is_null());
    // SHIP sees its peer TANK next to each metric.
    let (_, ship) = t.get("/api/research/SHIP/pack").await;
    assert_eq!(ship["peers"], json!(["TANK"]));
    let rpv = metric(&ship, "revenue_per_vessel");
    assert_eq!(rpv["peers"][0]["ticker"], "TANK");
    assert!(rpv["peers"][0]["value"].is_number(), "{}", rpv["peers"]);
    let freight = metric(&ship, "freight_handling_ratio");
    assert!(
        freight["latest"].is_null(),
        "tag nil on disk: a visible gap"
    );
    assert_eq!(freight["cells"][11]["gap"]["kind"], "missing");
    assert!(ship["missing_drivers"].as_array().unwrap().len() == 2);
}

#[tokio::test]
async fn a2_3_kpi_entry_shows_with_provenance_in_the_pack() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let (s, v) = call(
        &t,
        Method::PUT,
        "/api/research/BANK/kpi/car/2026/2",
        "car-1",
        Some(json!({"value": 21.4, "document": "https://example.invalid/laporan-publikasi-jun-2026.pdf", "document_date": "2026-07-29"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = call(
        &t,
        Method::PUT,
        "/api/research/BANK/kpi/tier1/2026/2",
        "tier-1",
        Some(json!({"unknown": true, "document": "Belum dipublikasikan", "document_date": "2026-07-29"})),
    )
    .await;
    assert_eq!(v["state"], "unknown");
    let (_, p) = t.get("/api/research/BANK/pack").await;
    let car = metric(&p, "car")["cells"][11].clone();
    assert_eq!(car["state"], "entered");
    assert_eq!(car["value"], 21.4);
    assert_eq!(car["provenance"]["kind"], "manual");
    assert_eq!(car["provenance"]["document_date"], "2026-07-29");
    assert_eq!(metric(&p, "car")["latest"]["value"], 21.4);
    let tier = metric(&p, "tier1")["cells"][11].clone();
    assert_eq!(tier["state"], "unknown");
    assert!(tier["value"].is_null());
    assert_eq!(tier["provenance"]["document"], "Belum dipublikasikan");
    assert_eq!(metric(&p, "car")["cells"][10]["state"], "not_entered");
    let missing = p["coverage"]["kpi"]["missing"].as_array().unwrap();
    assert!(!missing
        .iter()
        .any(|m| m == "car 2026Q2" || m == "tier1 2026Q2"));
    assert!(missing.iter().any(|m| m == "car 2026Q1"));
    assert_eq!(p["coverage"]["kpi"]["unknown"], 1);
}

#[tokio::test]
async fn a2_4_pack_override_re_renders_without_restart_and_shows_the_filing_default() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let (s, v) = call(
        &t,
        Method::PUT,
        "/api/research/universe/COAL",
        "ov-1",
        Some(json!({"pack_override": "shipping"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, p) = t.get("/api/research/COAL/pack").await;
    assert_eq!(p["pack"]["id"], "shipping");
    assert_eq!(p["pack"]["source"], "override");
    assert_eq!(p["pack"]["filing_default"], "coal_mining");
    assert_eq!(p["pack"]["filing_code"], "A121");
    assert!(metric_ids(&p).contains(&"vessel_count".to_string()));
    assert!(!metric_ids(&p).contains(&"production_mt".to_string()));
    let (s, _) = call(
        &t,
        Method::PUT,
        "/api/research/universe/COAL",
        "ov-2",
        Some(json!({"pack_override": null})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, p) = t.get("/api/research/COAL/pack").await;
    assert_eq!(p["pack"]["id"], "coal_mining");
    assert_eq!(p["pack"]["source"], "filing");
    // Reload re-reads the directory (empty here, so the embedded packs serve) without a restart.
    let (s, r) = call(&t, Method::POST, "/api/research/packs/reload", "rl-1", None).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["reloaded"], 7);
    assert_eq!(r["status"]["ok"], true);
}

#[tokio::test]
async fn a2_5_unknown_industry_shows_core_and_a_visible_no_pack_state() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let (s, p) = t.get("/api/research/NONE/pack").await;
    assert_eq!(s, StatusCode::OK);
    assert!(p["pack"]["id"].is_null());
    assert_eq!(p["pack"]["source"], "none");
    assert_eq!(p["pack"]["filing_code"], "Z999");
    assert_eq!(p["pack"]["action"], "Tentukan paket");
    assert_eq!(p["coverage"]["has_pack"], false);
    let dims: Vec<&str> = p["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["pack"].as_str().unwrap())
        .collect();
    assert_eq!(dims, ["core", "core", "core"]);
    assert!(metric(&p, "free_cash_flow")["latest"]["value"].is_number());
    assert_eq!(p["coverage"]["xbrl"]["pct"], 100.0);
    assert!(p["missing_drivers"].as_array().unwrap().is_empty());
    // Not in the universe → 404; malformed → 404.
    assert_eq!(
        t.get("/api/research/ZZZZ/pack").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        t.get("/api/research/z1/pack").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn packs_list_and_health_report_definitions_members_and_load_status() {
    let dir = tempfile::tempdir().unwrap();
    let t = seed(dir.path());
    let (s, v) = t.get("/api/research/packs").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["status"]["ok"], true);
    assert_eq!(v["status"]["source"], "embedded");
    let packs = v["packs"].as_array().unwrap();
    assert_eq!(packs.len(), 7);
    let tanker = packs.iter().find(|p| p["id"] == "shipping_tanker").unwrap();
    assert_eq!(tanker["industry_codes"], json!([]));
    let shipping = packs.iter().find(|p| p["id"] == "shipping").unwrap();
    assert_eq!(shipping["members"], json!(["SHIP", "TANK"]));
    assert_eq!(shipping["industry_codes"], json!(["A112"]));
    let core = packs.iter().find(|p| p["id"] == "core").unwrap();
    assert_eq!(core["members"].as_array().unwrap().len(), 7);
    let m = shipping["dimensions"][0]["metrics"][0].clone();
    for k in ["id", "label", "layer", "unit", "direction", "why"] {
        assert!(m.get(k).is_some(), "{k}");
    }
    let vessels = shipping["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["metrics"].as_array().unwrap())
        .find(|m| m["id"] == "vessels_gross")
        .unwrap();
    assert!(vessels["bindings"]["VesselsGross"][0]
        .as_str()
        .unwrap()
        .contains("VesselsDirectlyOwnedMember"));
    assert_eq!(shipping["drivers"][0]["status"], "TBD");

    let (_, h) = t.get("/api/health?deep=1").await;
    assert_eq!(h["sources"]["research_packs"]["ok"], true);
    assert!(h["sources"]["research_packs"]["detail"]
        .as_str()
        .unwrap()
        .contains("7 packs"));
    assert_eq!(h["sources"]["research_kpi"]["ok"], true);
    assert!(h["sources"]["research_kpi"]["detail"]
        .as_str()
        .unwrap()
        .contains("values"));
}

#[tokio::test]
async fn an_invalid_pack_file_is_reported_on_health_and_the_embedded_copy_serves() {
    let dir = tempfile::tempdir().unwrap();
    let packs_dir = dir.path().join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::write(packs_dir.join("coal_mining.toml"), "id = \"coal_mining\"\n").unwrap();
    let t = common::test_app_with(|cfg| cfg.research_packs_dir = packs_dir.clone());
    let (_, h) = t.get("/api/health?deep=1").await;
    assert_eq!(h["sources"]["research_packs"]["ok"], false);
    assert!(h["sources"]["research_packs"]["detail"]
        .as_str()
        .unwrap()
        .contains("coal_mining.toml"));
    let (_, v) = t.get("/api/research/packs").await;
    assert_eq!(v["status"]["invalid"][0]["file"], "coal_mining.toml");
    assert!(v["status"]["embedded_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i == "coal_mining"));
    assert_eq!(v["packs"].as_array().unwrap().len(), 7);
}
