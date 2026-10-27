//! Area 2: `GET /api/finance/stock-portfolio`, parser for `PORTOFOLIO!A1:AE35`
//! ported from MC `GoogleSheetsService.get_stock_portfolio`.
//!
//! Rupiah amounts (cost, valuation, P/L, cash) are integers; prices, lots,
//! units and percentages stay floats.
//!
//! The only Sheets route this app serves: it reads the portfolio spreadsheet and
//! nothing else (no finance spreadsheet, no ledger, no idempotency journal).

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::error::{ApiError, ApiResult};
use crate::py::py_round;
use crate::sheets::{Rows, SheetsClient, SheetsError, TtlCache};

pub const PORTFOLIO_RANGE: &str = "PORTOFOLIO!A1:AE35";

const READ_TTL: Duration = Duration::from_secs(45);

pub struct PortfolioState {
    pub sheets: SheetsClient,
    pub portfolio_spreadsheet_id: String,
    reads: TtlCache<Arc<Rows>>,
}

impl PortfolioState {
    pub fn new(cfg: &Config, sheets: SheetsClient) -> Self {
        Self {
            sheets,
            portfolio_spreadsheet_id: cfg.portfolio_spreadsheet_id.clone(),
            reads: TtlCache::new(READ_TTL),
        }
    }

    pub async fn rows_in(
        &self,
        spreadsheet_id: &str,
        range: &str,
    ) -> Result<Arc<Rows>, SheetsError> {
        let key = format!("{spreadsheet_id}|{range}");
        self.reads
            .get_or_try(&key, || async {
                self.sheets
                    .values_get(spreadsheet_id, range)
                    .await
                    .map(Arc::new)
            })
            .await
    }
}

pub fn router(cfg: Arc<Config>) -> Router {
    let sheets = SheetsClient::new(cfg.google_credentials_path.clone());
    router_with(Arc::new(PortfolioState::new(&cfg, sheets)))
}

pub fn router_with(state: Arc<PortfolioState>) -> Router {
    Router::new()
        .route("/api/finance/stock-portfolio", get(stock_portfolio))
        .with_state(state)
}

/// MC `_clean_n`: strips `Rp`, `,` and `%`; `(x)` is negative; junk is 0.
fn clean_n(v: &str) -> f64 {
    clean(v, true)
}

/// MC `_clean_p`: like `_clean_n` but keeps `Rp` (percent cells).
fn clean_p(v: &str) -> f64 {
    clean(v, false)
}

fn clean(v: &str, strip_rp: bool) -> f64 {
    let mut s = v.trim();
    if s.is_empty() || s == "-" {
        return 0.0;
    }
    let neg = s.starts_with('(') && s.ends_with(')') && s.len() >= 2;
    if neg {
        s = s[1..s.len() - 1].trim();
    }
    let mut s = s.to_string();
    if strip_rp {
        s = s.replace("Rp", "");
    }
    let s = s.replace([',', '%'], "");
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    if lower.contains("inf") || lower.contains("nan") {
        return 0.0;
    }
    match s.parse::<f64>() {
        Ok(n) if neg => -n,
        Ok(n) => n,
        Err(_) => 0.0,
    }
}

fn cell(r: &[String], i: usize) -> &str {
    r.get(i).map(String::as_str).unwrap_or("")
}

/// Money is kept as integer rupiah (finance-app `src/finance/parse.rs` `to_rupiah`).
fn to_rupiah(n: f64) -> i64 {
    n.round() as i64
}

/// Python `datetime.now().isoformat()` (local, naive, microseconds when non-zero).
pub fn py_isoformat_now() -> String {
    let now = chrono::Local::now().naive_local();
    if now.and_utc().timestamp_subsec_micros() == 0 {
        now.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        now.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
    }
}

struct Position {
    no: i64,
    category: String,
    ticker: String,
    full_code: String,
    company_name: String,
    shares: f64,
    avg_price: f64,
    current_price: f64,
    cost: i64,
    valuation: i64,
    pl: i64,
    pl_pct: f64,
    fund_alloc_pct: f64,
    val_alloc_pct: f64,
    target_alloc_pct: f64,
}

impl Position {
    fn to_json(&self) -> Value {
        json!({
            "no": self.no,
            "category": self.category,
            "ticker": self.ticker,
            "full_code": self.full_code,
            "company_name": self.company_name,
            "shares": self.shares,
            "lots": py_round(self.shares / 100.0, 2),
            "avg_price": self.avg_price,
            "current_price": self.current_price,
            "cost": self.cost,
            "valuation": self.valuation,
            "pl": self.pl,
            "pl_pct": self.pl_pct,
            "fund_alloc_pct": self.fund_alloc_pct,
            "val_alloc_pct": self.val_alloc_pct,
            "target_alloc_pct": self.target_alloc_pct,
            "drift_pct": py_round(self.val_alloc_pct - self.target_alloc_pct, 2),
        })
    }
}

pub fn parse_portfolio(rows: &Rows, spreadsheet_id: &str, updated_at: &str) -> Value {
    if rows.is_empty() {
        return json!({"summary": {}, "positions": [], "categories": {}, "available": false, "updated_at": updated_at});
    }

    let (mut current_valuation, mut total_cost, mut total_pl) = (0i64, 0i64, 0i64);
    let (mut total_pl_pct, mut cash_balance, mut cash_alloc_pct) = (0.0f64, 0i64, 0.0f64);
    let (mut unit_count, mut nav_per_unit) = (0.0f64, 0.0f64);

    for r in rows.iter().take(7) {
        if r.len() > 4 && cell(r, 1).contains("VALUASI SAAT INI") {
            current_valuation = to_rupiah(clean_n(cell(r, 4)));
        }
        if r.len() > 4 && cell(r, 1).contains("JUMLAH UNIT PENYERTAAN") {
            unit_count = clean_n(cell(r, 4));
        }
        if r.len() > 9 && cell(r, 8).contains("Rp") {
            cash_balance = to_rupiah(clean_n(cell(r, 8)));
            cash_alloc_pct = clean_p(cell(r, 9));
        }
        if r.len() > 4 && cell(r, 1).contains("HARGA UNIT") {
            nav_per_unit = clean_n(cell(r, 4));
        }
    }

    let mut positions = Vec::new();
    for r in rows.iter().skip(7) {
        if r.len() < 4 {
            continue;
        }
        let no_col = cell(r, 1).trim();
        let kode = cell(r, 3).trim();
        let n = |i: usize| clean_n(cell(r, i));
        let p = |i: usize| clean_p(cell(r, i));

        // Totals row: no number, positive cost and valuation.
        if no_col.is_empty() && r.len() >= 12 && n(8) > 0.0 && n(9) > 0.0 {
            total_cost = to_rupiah(n(8));
            if current_valuation == 0 {
                current_valuation = to_rupiah(n(9));
            }
            total_pl = to_rupiah(n(10));
            total_pl_pct = p(11);
            continue;
        }
        if no_col.is_empty() || !no_col.bytes().all(|b| b.is_ascii_digit()) || kode.is_empty() {
            continue;
        }
        positions.push(Position {
            no: no_col.parse().unwrap_or(0),
            category: cell(r, 2).trim().to_string(),
            ticker: kode.replace("IDX:", "").trim().to_string(),
            full_code: kode.to_string(),
            company_name: cell(r, 4).trim().to_string(),
            shares: n(5),
            avg_price: n(6),
            current_price: n(7),
            cost: to_rupiah(n(8)),
            valuation: to_rupiah(n(9)),
            pl: to_rupiah(n(10)),
            pl_pct: p(11),
            fund_alloc_pct: p(12),
            val_alloc_pct: p(13),
            target_alloc_pct: p(14),
        });
    }

    if total_cost == 0 && !positions.is_empty() {
        total_cost = positions.iter().map(|p| p.cost).sum();
        if current_valuation == 0 {
            current_valuation = positions.iter().map(|p| p.valuation).sum::<i64>() + cash_balance;
        }
        total_pl = positions.iter().map(|p| p.pl).sum();
        if total_cost > 0 {
            total_pl_pct = py_round(total_pl as f64 / total_cost as f64 * 100.0, 2);
        }
    }

    // category → (valuation, cost, pl, val_alloc_pct, count), first-seen order.
    let mut cats: Vec<(String, i64, i64, i64, f64, i64)> = Vec::new();
    for p in &positions {
        let c = if p.category.is_empty() {
            "OTHER".to_string()
        } else {
            p.category.clone()
        };
        let idx = match cats.iter().position(|e| e.0 == c) {
            Some(i) => i,
            None => {
                cats.push((c, 0, 0, 0, 0.0, 0));
                cats.len() - 1
            }
        };
        let e = &mut cats[idx];
        e.1 += p.valuation;
        e.2 += p.cost;
        e.3 += p.pl;
        e.4 += p.val_alloc_pct;
        e.5 += 1;
    }
    let categories: Map<String, Value> = cats
        .into_iter()
        .map(|(c, valuation, cost, pl, val_alloc, count)| {
            let pl_pct = if cost > 0 {
                py_round(pl as f64 / cost as f64 * 100.0, 2)
            } else {
                0.0
            };
            let v = json!({
                "valuation": valuation, "cost": cost, "pl": pl,
                "val_alloc_pct": py_round(val_alloc, 2), "count": count, "pl_pct": pl_pct,
            });
            (c, v)
        })
        .collect();

    json!({
        "summary": {
            "current_valuation": current_valuation,
            "total_cost": total_cost,
            "total_pl": total_pl,
            "total_pl_pct": total_pl_pct,
            "cash_balance": cash_balance,
            "cash_alloc_pct": cash_alloc_pct,
            "unit_count": unit_count,
            "nav_per_unit": nav_per_unit,
            "updated_at": updated_at,
        },
        "positions": positions.iter().map(Position::to_json).collect::<Vec<_>>(),
        "categories": categories,
        "available": true,
        "spreadsheet_id": spreadsheet_id,
    })
}

pub async fn stock_portfolio(State(st): State<Arc<PortfolioState>>) -> ApiResult {
    let id = &st.portfolio_spreadsheet_id;
    let rows = st.rows_in(id, PORTFOLIO_RANGE).await.map_err(|e| {
        tracing::error!("stock portfolio: {e}");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to fetch stock portfolio: {e}"),
        )
    })?;
    Ok(Json(parse_portfolio(&rows, id, &py_isoformat_now())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(data: &[&[&str]]) -> Rows {
        data.iter()
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect()
    }

    fn sheet() -> Rows {
        let mut r = rows(&[
            &[""],
            &["", "VALUASI SAAT INI", "", "", "Rp 12,500,000"],
            &["", "JUMLAH UNIT PENYERTAAN", "", "", "10,000.5"],
            &[
                "",
                "HARGA UNIT",
                "",
                "",
                "1,249.94",
                "",
                "",
                "",
                "Rp 500,000",
                "4.00%",
            ],
            &[""],
            &[""],
            &[""],
            &["", "No", "Kategori", "Kode", "Emiten"],
        ]);
        r.extend(rows(&[
            &[
                "",
                "1",
                "CORE",
                "IDX:BBCA",
                "Bank Central Asia",
                "1,000",
                "8,750.5",
                "9,100",
                "8,750,500",
                "9,100,000",
                "349,500",
                "3.99%",
                "70.00%",
                "72.80%",
                "70.00%",
            ],
            &[
                "",
                "2",
                "",
                "IDX:ASII",
                "Astra",
                "500",
                "5,000",
                "4,800",
                "2,500,000",
                "2,400,000",
                "(100,000)",
                "(4.00%)",
                "20.00%",
                "19.20%",
                "25.00%",
            ],
            &["", "x", "CORE", "IDX:NOPE", "skip"],
            &[
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "11,250,500",
                "11,500,000",
                "249,500",
                "2.22%",
            ],
        ]));
        r
    }

    #[test]
    fn parses_summary_positions_and_categories() {
        let v = parse_portfolio(&sheet(), "sid", "T");
        let s = &v["summary"];
        assert_eq!(s["current_valuation"], 12_500_000);
        assert_eq!(s["unit_count"], 10000.5);
        assert_eq!(s["nav_per_unit"], 1249.94);
        assert_eq!(s["cash_balance"], 500_000);
        assert_eq!(s["cash_alloc_pct"], 4.0);
        assert_eq!(s["total_cost"], 11_250_500);
        assert_eq!(s["total_pl"], 249_500);
        assert_eq!(s["total_pl_pct"], 2.22);
        assert_eq!(s["updated_at"], "T");

        let p = &v["positions"];
        assert_eq!(p.as_array().unwrap().len(), 2);
        assert_eq!(p[0]["ticker"], "BBCA");
        assert_eq!(p[0]["full_code"], "IDX:BBCA");
        assert_eq!(p[0]["lots"], 10.0);
        assert_eq!(p[0]["avg_price"], 8750.5);
        assert_eq!(p[0]["drift_pct"], 2.8);
        assert_eq!(p[1]["pl"], -100_000);
        assert_eq!(p[1]["pl_pct"], -4.0);
        assert_eq!(p[1]["drift_pct"], -5.8);

        assert_eq!(
            v["categories"]["CORE"],
            json!({"valuation": 9100000, "cost": 8750500, "pl": 349500, "val_alloc_pct": 72.8, "count": 1, "pl_pct": 3.99})
        );
        assert_eq!(v["categories"]["OTHER"]["pl_pct"], -4.0);
        assert_eq!(v["available"], true);
        assert_eq!(v["spreadsheet_id"], "sid");
    }

    #[test]
    fn totals_fallback_without_totals_row() {
        let mut r = sheet();
        r.pop();
        r[1][4] = String::new();
        let v = parse_portfolio(&r, "sid", "T");
        assert_eq!(v["summary"]["total_cost"], 11_250_500);
        assert_eq!(
            v["summary"]["current_valuation"],
            9_100_000 + 2_400_000 + 500_000
        );
        assert_eq!(v["summary"]["total_pl_pct"], 2.22);
    }

    #[test]
    fn empty_sheet() {
        assert_eq!(
            parse_portfolio(&vec![], "sid", "T"),
            json!({"summary": {}, "positions": [], "categories": {}, "available": false, "updated_at": "T"})
        );
    }

    #[test]
    fn clean_number_rules() {
        assert_eq!(clean_n("Rp 1,234"), 1234.0);
        assert_eq!(clean_n("(5%)"), -5.0);
        assert_eq!(clean_n("-"), 0.0);
        assert_eq!(
            clean_p("Rp 5"),
            0.0,
            "percent cleaner keeps Rp → unparseable"
        );
    }
}
