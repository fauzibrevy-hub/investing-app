//! Valuation (research plan §4.6, D10, D3; design.md §4.2 Valuasi): reverse DCF, TTM and
//! cycle-adjusted multiples with own-history percentiles, earnings yield against the
//! 10-year government bond, and the owner's scenarios. No system fair value, score or
//! label: the only value per share produced here is the owner's scenario table output.
//!
//! The engine is pure over `Inputs` (prices, point-in-time shares, per-quarter
//! fundamentals with the date they became available, dividends, driver series, scenarios);
//! `Inputs::load` fills it from the store and `build` renders the API payload. Rules come
//! from `research/rules.toml` (embedded default) and carry a version (prd.md A3.2).

use std::path::{Path, PathBuf};

use chrono::{Datelike, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::packs::def::Period;
use super::packs::eval::{evaluate, percentile_rank, Ctx, Quarter};
use super::packs::payload::{quarter_end_date, StoreSource};
use super::packs::PackSet;
use super::shares::{Resolved, ShareSeries};
use super::store::{EventKind, ScenarioRow, Store, StoreError};

pub const RULES_FILE: &str = "rules.toml";
pub const EMBEDDED_RULES: &str = include_str!("../../research/rules.toml");
/// Trading days per year, for history-length checks.
const TRADING_DAYS: usize = 250;

// ---------------------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PojkOffsets {
    pub q1: u32,
    pub q2: u32,
    pub q3: u32,
    pub fy: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangelogEntry {
    pub version: String,
    pub date: String,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub version: String,
    /// Percent values, shown side by side (design.md §4.2 `TINGKAT DISKONTO`).
    pub discount_rates: Vec<f64>,
    pub terminal_growth: f64,
    pub horizon_years: u32,
    pub cycle_windows: Vec<u32>,
    pub min_cycle_years: u32,
    pub percentile_window_years: u32,
    pub percentile_min_years: u32,
    pub pojk_offsets: PojkOffsets,
    #[serde(default)]
    pub changelog: Vec<ChangelogEntry>,
}

impl Rules {
    pub fn parse(text: &str) -> Result<Self, String> {
        let r: Rules = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut errs = Vec::new();
        if r.version.trim().is_empty() {
            errs.push("version is empty".to_string());
        }
        if r.discount_rates.is_empty() || r.discount_rates.len() > 5 {
            errs.push("discount_rates must hold 1 to 5 rates".to_string());
        }
        if !(0.0..10.0).contains(&r.terminal_growth) {
            errs.push("terminal_growth must be in [0, 10) percent".to_string());
        }
        for d in &r.discount_rates {
            if !(d.is_finite() && *d > r.terminal_growth && *d < 50.0) {
                errs.push(format!(
                    "discount rate {d} must exceed terminal growth and stay below 50"
                ));
            }
        }
        if !(1..=30).contains(&r.horizon_years) {
            errs.push("horizon_years must be 1 to 30".to_string());
        }
        if r.cycle_windows.is_empty() || r.cycle_windows.iter().any(|w| *w < 2 || *w > 20) {
            errs.push("cycle_windows must be 2 to 20 years each".to_string());
        }
        if r.min_cycle_years < 2 {
            errs.push("min_cycle_years must be at least 2".to_string());
        }
        if r.percentile_window_years < 1 || r.percentile_min_years < 1 {
            errs.push("percentile windows must be at least 1 year".to_string());
        }
        if errs.is_empty() {
            Ok(r)
        } else {
            Err(errs.join("; "))
        }
    }

    pub fn embedded() -> Self {
        Self::parse(EMBEDDED_RULES).expect("embedded rules.toml is valid")
    }

    /// The middle discount rate, used by the scenario table.
    pub fn base_discount_rate(&self) -> f64 {
        let mut v = self.discount_rates.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    }
}

/// Rules as loaded: from `<dir>/rules.toml` when present and valid, else the embedded
/// file, with the reason reported on the Packs screen.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LoadedRules {
    pub rules: Rules,
    /// `embedded` or the file path.
    pub source: String,
    pub error: Option<String>,
    pub loaded_at: String,
}

impl LoadedRules {
    pub fn load(dir: &Path, now: &str) -> Self {
        let path: PathBuf = dir.join(RULES_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => match Rules::parse(&text) {
                Ok(rules) => Self {
                    rules,
                    source: path.display().to_string(),
                    error: None,
                    loaded_at: now.into(),
                },
                Err(e) => Self {
                    rules: Rules::embedded(),
                    source: "embedded".into(),
                    error: Some(format!("{}: {e}", path.display())),
                    loaded_at: now.into(),
                },
            },
            Err(_) => Self {
                rules: Rules::embedded(),
                source: "embedded".into(),
                error: None,
                loaded_at: now.into(),
            },
        }
    }

    pub fn embedded(now: &str) -> Self {
        Self {
            rules: Rules::embedded(),
            source: "embedded".into(),
            error: None,
            loaded_at: now.into(),
        }
    }

    /// `GET /api/research/rules` (design.md §4.4 Rules card).
    pub fn json(&self, derive_version: &str) -> Value {
        let r = &self.rules;
        json!({
            "rules_version": r.version,
            "derive_rules_version": derive_version,
            "discount_rates": r.discount_rates,
            "terminal_growth": r.terminal_growth,
            "horizon_years": r.horizon_years,
            "cycle_windows": r.cycle_windows,
            "min_cycle_years": r.min_cycle_years,
            "percentile_window_years": r.percentile_window_years,
            "percentile_min_years": r.percentile_min_years,
            "pojk_offsets": r.pojk_offsets,
            "changelog": r.changelog,
            "source": self.source,
            "file": RULES_FILE,
            "error": self.error,
            "loaded_at": self.loaded_at,
        })
    }
}

// ---------------------------------------------------------------------------------------
// Reverse DCF
// ---------------------------------------------------------------------------------------

/// Present value of `fcf` growing at `g` for `years`, then a terminal value growing at
/// `tg`, discounted at `r`. Rates are fractions. Requires `r > tg`.
pub fn present_value(fcf: f64, g: f64, r: f64, years: u32, tg: f64) -> f64 {
    let mut pv = 0.0;
    let mut cf = fcf;
    let mut disc = 1.0;
    for _ in 0..years {
        cf *= 1.0 + g;
        disc *= 1.0 + r;
        pv += cf / disc;
    }
    let terminal = cf * (1.0 + tg) / (r - tg);
    pv + terminal / disc
}

/// Growth bounds the bisection searches: below −95 % a year or above +300 % a year the
/// answer is not a number worth showing.
pub const GROWTH_LO: f64 = -0.95;
pub const GROWTH_HI: f64 = 3.0;

/// Solves `present_value(fcf, g, r, years, tg) = market_cap` for `g`. `None` when FCF is
/// not positive (not applicable), the rate does not exceed terminal growth, or the solution
/// lies outside `[GROWTH_LO, GROWTH_HI]`. Monotone in `market_cap` (tested by property).
pub fn implied_growth(market_cap: f64, fcf: f64, r: f64, years: u32, tg: f64) -> Option<f64> {
    if !(fcf > 0.0 && fcf.is_finite() && market_cap > 0.0 && market_cap.is_finite()) {
        return None;
    }
    if !(r > tg && years > 0) {
        return None;
    }
    let (mut lo, mut hi) = (GROWTH_LO, GROWTH_HI);
    if present_value(fcf, lo, r, years, tg) > market_cap
        || present_value(fcf, hi, r, years, tg) < market_cap
    {
        return None;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if present_value(fcf, mid, r, years, tg) < market_cap {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-10 {
            break;
        }
    }
    Some(0.5 * (lo + hi))
}

// ---------------------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------------------

/// One quarter's fundamentals and the day they became usable (filing `available_from`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct QuarterInputs {
    pub fy: i32,
    pub q: u32,
    pub period_end: NaiveDate,
    pub available_from: NaiveDate,
    pub revenue_ttm: Option<f64>,
    pub net_income_ttm: Option<f64>,
    pub ebitda_ttm: Option<f64>,
    pub fcf_ttm: Option<f64>,
    pub equity: Option<f64>,
    pub net_debt: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub growth: Option<f64>,
    pub margin: Option<f64>,
    pub exit_multiple: Option<f64>,
    pub years: Option<u32>,
    pub updated_at: String,
}

pub type Series = Vec<(NaiveDate, f64)>;

/// Everything the engine reads; pure data.
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    pub ticker: String,
    /// Reporting currency of the filings (`IDR` or `USD`); prices are always IDR.
    pub currency: String,
    /// Daily closes, ascending.
    pub prices: Series,
    pub shares: ShareSeries,
    /// Ascending by `available_from`.
    pub quarters: Vec<QuarterInputs>,
    /// Dividend per share by ex-date.
    pub dividends: Series,
    /// Splits as `(date, numerator / denominator)`. Yahoo closes are split-adjusted, so a
    /// share count dated before a split is moved to the current basis with these ratios.
    pub splits: Vec<(NaiveDate, f64)>,
    pub cpi: Series,
    pub bond_10y: Series,
    pub usd_idr: Series,
    pub scenarios: Vec<Scenario>,
}

/// Latest value on or before `on` of an ascending series.
pub fn value_on(series: &[(NaiveDate, f64)], on: NaiveDate) -> Option<f64> {
    let i = series.partition_point(|(d, _)| *d <= on);
    (i > 0).then(|| series[i - 1].1)
}

impl Inputs {
    /// The latest quarter whose filing was available on `on`.
    pub fn known(&self, on: NaiveDate) -> Option<&QuarterInputs> {
        // The newest quarter among those already available: an amended older filing with a
        // later availability date never hides a newer quarter (review R3).
        self.quarters
            .iter()
            .filter(|q| q.available_from <= on)
            .max_by_key(|q| (q.fy, q.q, q.available_from))
    }

    /// Fiscal years (Q4 rows) available on `on`, newest first, at most `n`.
    pub fn fiscal_years(&self, on: NaiveDate, n: usize) -> Vec<&QuarterInputs> {
        let mut fys: Vec<&QuarterInputs> = self
            .quarters
            .iter()
            .filter(|q| q.q == 4 && q.available_from <= on)
            .collect();
        fys.sort_by_key(|q| std::cmp::Reverse(q.fy));
        fys.dedup_by_key(|q| q.fy);
        fys.truncate(n);
        fys
    }

    /// Multiplier from the reporting currency to IDR on `on`: 1 for IDR filers, the
    /// USD/IDR driver for USD filers, `None` when that driver has no value yet.
    pub fn to_idr(&self, on: NaiveDate) -> Option<f64> {
        if self.currency.eq_ignore_ascii_case("USD") {
            value_on(&self.usd_idr, on)
        } else {
            Some(1.0)
        }
    }

    /// Dividends per share with ex-dates in the twelve months ending on `on`.
    pub fn dps_ttm(&self, on: NaiveDate) -> f64 {
        let from = on.checked_sub_months(Months::new(12)).unwrap_or(on);
        self.dividends
            .iter()
            .filter(|(d, _)| *d > from && *d <= on)
            .map(|(_, v)| v)
            .sum()
    }

    /// Annual dividends per share for the calendar year `year`.
    fn dps_year(&self, year: i32) -> f64 {
        self.dividends
            .iter()
            .filter(|(d, _)| d.year() == year)
            .map(|(_, v)| v)
            .sum()
    }

    /// Loads one ticker from the store: prices over the percentile window, point-in-time
    /// shares, core metrics per filed quarter through the pack evaluator, dividends,
    /// drivers and the owner's scenarios.
    pub fn load(
        store: &Store,
        packs: &PackSet,
        ticker: &str,
        today: NaiveDate,
        rules: &Rules,
    ) -> Result<Self, StoreError> {
        let from = today
            .checked_sub_months(Months::new(12 * rules.percentile_window_years))
            .unwrap_or(today);
        let from_s = from.format("%Y-%m-%d").to_string();
        let prices: Series = store
            .prices(ticker)?
            .into_iter()
            .filter_map(|b| {
                NaiveDate::parse_from_str(&b.date, "%Y-%m-%d")
                    .ok()
                    .map(|d| (d, b.close))
            })
            .filter(|(d, _)| *d >= from && *d <= today)
            .collect();
        let core = packs.core();
        let src = StoreSource::load(store, ticker, &[core])?;
        let ctx = Ctx {
            source: &src,
            packs,
        };
        let metric = |id: &str, period: Period, q: Quarter| -> Option<f64> {
            let m = core.metric(id)?;
            let f = m.formula.as_ref()?;
            evaluate(&ctx, core, f, period, q).value
        };
        let filings = store.filings(Some(ticker))?;
        let currency = filings
            .first()
            .map(|f| f.filing.currency.clone())
            .unwrap_or_else(|| "IDR".into());
        let mut quarters = Vec::new();
        for f in &filings {
            let q = Quarter::new(f.filing.fy, f.filing.period.quarter());
            let Some(period_end) = quarter_end_date(q) else {
                continue;
            };
            quarters.push(QuarterInputs {
                fy: q.fy,
                q: q.q,
                period_end,
                available_from: f.filing.available_from,
                revenue_ttm: metric("revenue", Period::Ttm, q),
                net_income_ttm: metric("net_income", Period::Ttm, q),
                ebitda_ttm: metric("ebitda", Period::Ttm, q),
                fcf_ttm: metric("free_cash_flow", Period::Ttm, q),
                equity: metric("total_equity", Period::Instant, q),
                net_debt: metric("net_debt", Period::Instant, q),
            });
        }
        quarters.sort_by_key(|q| (q.available_from, q.fy, q.q));
        let events = store.events(ticker)?;
        let dividends: Series = events
            .iter()
            .filter(|e| e.kind == EventKind::Dividend)
            .filter_map(|e| {
                let d = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok()?;
                Some((d, e.amount?))
            })
            .collect();
        let splits: Vec<(NaiveDate, f64)> = events
            .iter()
            .filter(|e| e.kind == EventKind::Split)
            .filter_map(|e| {
                let d = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok()?;
                let (n, den) = (e.ratio_num?, e.ratio_den?);
                (n > 0.0 && den > 0.0).then_some((d, n / den))
            })
            .collect();
        // CPI has no lower bound: the cycle windows of the oldest history days reach years
        // before the price window (review R3).
        let driver = |id: &str, bounded: bool| -> Result<Series, StoreError> {
            Ok(store
                .driver_series(id, bounded.then_some(from_s.as_str()))?
                .into_iter()
                .filter_map(|(d, v)| {
                    NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                        .ok()
                        .map(|d| (d, v))
                })
                .collect())
        };
        let scenarios = store
            .scenarios(ticker)?
            .into_iter()
            .map(|r| Scenario {
                name: r.name,
                growth: r.growth,
                margin: r.margin,
                exit_multiple: r.exit_multiple,
                years: r.years,
                updated_at: r.updated_at,
            })
            .collect();
        Ok(Self {
            ticker: ticker.into(),
            currency,
            prices,
            shares: store.share_series(ticker)?,
            quarters,
            dividends,
            splits,
            cpi: driver("cpi", false)?,
            bond_10y: driver("sbn_10y", true)?,
            usd_idr: driver("usd_idr", true)?,
            scenarios,
        })
    }
}

// ---------------------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FcfBase {
    Ttm,
    Avg5y,
}

impl FcfBase {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ttm" => Some(Self::Ttm),
            "avg5y" | "avg_5y" | "5y" => Some(Self::Avg5y),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ttm => "ttm",
            Self::Avg5y => "avg5y",
        }
    }
}

/// Which multiple, and which earnings base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Multiple {
    Pe,
    Pb,
    EvEbitda,
    DividendYield,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    Ttm,
    Cycle(u32),
}

/// A per-day state: price, shares, fundamentals known that day, converted to IDR.
#[derive(Clone, Debug, PartialEq)]
pub struct DayState {
    pub date: NaiveDate,
    pub price: f64,
    pub shares: Option<f64>,
    pub fx: Option<f64>,
}

impl DayState {
    pub fn market_cap(&self) -> Option<f64> {
        self.shares.map(|s| s * self.price)
    }
}

fn pos(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite() && *x > 0.0)
}

/// Average of a cycle window of fiscal-year values known on `on`, CPI-adjusted to `on`
/// when the CPI series covers every year end, else nominal. Returns `(value, adjusted)`.
fn cycle_average(
    inputs: &Inputs,
    on: NaiveDate,
    window: u32,
    min_years: u32,
    use_cpi: bool,
    pick: impl Fn(&QuarterInputs) -> Option<f64>,
) -> Option<(f64, bool)> {
    let fys = inputs.fiscal_years(on, window as usize);
    if (fys.len() as u32) < min_years {
        return None;
    }
    let nominal: Vec<f64> = fys.iter().map(|fy| pick(fy)).collect::<Option<_>>()?;
    let n = nominal.len() as f64;
    if use_cpi {
        if let Some(now) = value_on(&inputs.cpi, on) {
            let factors: Option<Vec<f64>> = fys
                .iter()
                .map(|fy| {
                    value_on(&inputs.cpi, fy.period_end)
                        .filter(|t| *t > 0.0)
                        .map(|t| now / t)
                })
                .collect();
            if let Some(f) = factors {
                let adjusted = nominal.iter().zip(&f).map(|(v, f)| v * f).sum::<f64>() / n;
                return Some((adjusted, true));
            }
        }
    }
    // Nominal for every year, so the label is honest.
    Some((nominal.iter().sum::<f64>() / n, false))
}

/// One multiple on one day. `None` is a gap (no fundamentals, negative earnings, no shares).
/// Yields are percent; multiples are ratios.
pub fn multiple_on(
    inputs: &Inputs,
    rules: &Rules,
    day: &DayState,
    which: Multiple,
    basis: Basis,
    use_cpi: bool,
) -> Option<(f64, bool)> {
    let on = day.date;
    let mcap = day.market_cap()?;
    let fx = day.fx?;
    let known = inputs.known(on)?;
    match (which, basis) {
        (Multiple::Pe, Basis::Ttm) => {
            let e = pos(known.net_income_ttm)? * fx;
            Some((mcap / e, false))
        }
        (Multiple::Pe, Basis::Cycle(w)) => {
            let (e, adj) = cycle_average(inputs, on, w, rules.min_cycle_years, use_cpi, |q| {
                q.net_income_ttm
            })?;
            Some((mcap / (pos(Some(e))? * fx), adj))
        }
        (Multiple::Pb, Basis::Ttm) => {
            let b = pos(known.equity)? * fx;
            Some((mcap / b, false))
        }
        (Multiple::Pb, Basis::Cycle(w)) => {
            let (b, adj) =
                cycle_average(inputs, on, w, rules.min_cycle_years, use_cpi, |q| q.equity)?;
            Some((mcap / (pos(Some(b))? * fx), adj))
        }
        (Multiple::EvEbitda, Basis::Ttm) => {
            let ebitda = pos(known.ebitda_ttm)? * fx;
            let ev = mcap + known.net_debt? * fx;
            Some((ev / ebitda, false))
        }
        (Multiple::EvEbitda, Basis::Cycle(w)) => {
            let (e, adj) = cycle_average(inputs, on, w, rules.min_cycle_years, use_cpi, |q| {
                q.ebitda_ttm
            })?;
            let ev = mcap + known.net_debt? * fx;
            Some((ev / (pos(Some(e))? * fx), adj))
        }
        (Multiple::DividendYield, Basis::Ttm) => {
            // No dividend events at all is a gap, not a zero yield.
            if inputs.dividends.is_empty() {
                return None;
            }
            Some((100.0 * inputs.dps_ttm(on) / day.price, false))
        }
        (Multiple::DividendYield, Basis::Cycle(w)) => {
            // Average annual DPS over the window's calendar years before `on`, counting
            // only years the price history covers (a year without data is not a zero).
            if inputs.dividends.is_empty() {
                return None;
            }
            let first_year = inputs.prices.first().map(|(d, _)| d.year())?;
            let years: Vec<i32> = (1..=w as i32)
                .map(|k| on.year() - k)
                .filter(|y| *y >= first_year)
                .collect();
            if (years.len() as u32) < rules.min_cycle_years {
                return None;
            }
            let avg = years.iter().map(|y| inputs.dps_year(*y)).sum::<f64>() / years.len() as f64;
            Some((100.0 * avg / day.price, false))
        }
    }
}

/// Earnings yield (percent) on one day: TTM net income over market cap.
pub fn earnings_yield_on(inputs: &Inputs, day: &DayState) -> Option<f64> {
    let mcap = day.market_cap()?;
    let fx = day.fx?;
    let e = inputs.known(day.date)?.net_income_ttm? * fx;
    Some(100.0 * e / mcap)
}

/// The per-day states over the price history, shares resolved per breakpoint.
pub fn day_states(inputs: &Inputs) -> Vec<DayState> {
    let dates: Vec<NaiveDate> = inputs.prices.iter().map(|(d, _)| *d).collect();
    let shares = inputs.shares.over(&dates);
    inputs
        .prices
        .iter()
        .zip(shares)
        .map(|((d, p), s)| DayState {
            date: *d,
            price: *p,
            shares: s.map(|r| r.shares * split_factor(&inputs.splits, *d)),
            fx: inputs.to_idr(*d),
        })
        .collect()
}

/// Product of the split ratios dated after `on`: moves a share count of that day onto the
/// basis of today's (split-adjusted) closes.
pub fn split_factor(splits: &[(NaiveDate, f64)], on: NaiveDate) -> f64 {
    splits
        .iter()
        .filter(|(d, _)| *d > on)
        .map(|(_, r)| r)
        .product()
}

/// Summary of a daily history: the current value, its percentile, median, range, count.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Band {
    pub value: Option<f64>,
    pub percentile: Option<f64>,
    pub median: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub n: usize,
    /// `riwayat < 3 th` when the history is too short for a percentile.
    pub caption: Option<String>,
    /// Cycle variants: whether the earnings were CPI-adjusted (else nominal, labelled).
    pub cpi_adjusted: Option<bool>,
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    })
}

/// Builds a band from a daily history (ascending) and the current value.
pub fn band(history: &[f64], current: Option<(f64, bool)>, rules: &Rules, cycle: bool) -> Band {
    let min_points = rules.percentile_min_years as usize * TRADING_DAYS;
    let mut sorted = history.to_vec();
    let enough = history.len() >= min_points;
    let percentile = match current {
        Some((c, _)) if enough => percentile_rank(history, c),
        _ => None,
    };
    Band {
        value: current.map(|c| c.0),
        percentile,
        median: if enough { median(&mut sorted) } else { None },
        min: if enough {
            sorted.first().copied()
        } else {
            None
        },
        max: if enough { sorted.last().copied() } else { None },
        n: history.len(),
        caption: (!enough && current.is_some())
            .then(|| format!("riwayat < {} th", rules.percentile_min_years)),
        cpi_adjusted: if cycle { current.map(|c| c.1) } else { None },
    }
}

/// The scenario table (design.md §4.2 Skenario): revenue grown for `years` at `growth`,
/// times `margin`, times `exit_multiple`, discounted at the middle discount rate, per
/// share. The only value per share in the tab (D3).
pub fn scenario_value(
    s: &Scenario,
    revenue_ttm_idr: Option<f64>,
    shares: Option<f64>,
    base_rate_pct: f64,
) -> Option<f64> {
    let (g, m, x, years) = (s.growth?, s.margin?, s.exit_multiple?, s.years?);
    let revenue = pos(revenue_ttm_idr)?;
    let shares = pos(shares)?;
    let earnings = revenue * (1.0 + g / 100.0).powi(years as i32) * (m / 100.0);
    let exit_value = earnings * x;
    let pv = exit_value / (1.0 + base_rate_pct / 100.0).powi(years as i32);
    let v = pv / shares;
    v.is_finite().then_some(v)
}

/// Renders `GET /api/research/{ticker}/valuation`.
pub fn build(inputs: &Inputs, rules: &Rules, fcf_base: FcfBase, now: &str) -> Value {
    let days = day_states(inputs);
    let today = days.last().cloned();
    let price = today.as_ref().map(|d| d.price);
    let date = today.as_ref().map(|d| d.date);
    let shares: Option<Resolved> = date.and_then(|d| inputs.shares.at(d));
    let known = date.and_then(|d| inputs.known(d));
    let fx = today.as_ref().and_then(|d| d.fx);
    let mcap = today.as_ref().and_then(|d| d.market_cap());

    // Reverse DCF in IDR.
    let fcf_ttm = known.and_then(|k| k.fcf_ttm).zip(fx).map(|(v, fx)| v * fx);
    let fcf_avg5y = date
        .and_then(|d| cycle_average(inputs, d, 5, rules.min_cycle_years, false, |q| q.fcf_ttm))
        .zip(fx)
        .map(|((v, _), fx)| v * fx);
    let fcf = match fcf_base {
        FcfBase::Ttm => fcf_ttm,
        FcfBase::Avg5y => fcf_avg5y,
    };
    let tg = rules.terminal_growth / 100.0;
    let dcf_rows: Vec<Value> = rules
        .discount_rates
        .iter()
        .map(|r| {
            let g = mcap
                .zip(fcf)
                .and_then(|(m, f)| implied_growth(m, f, r / 100.0, rules.horizon_years, tg));
            json!({"discount_rate": r, "implied_growth": g.map(|g| g * 100.0)})
        })
        .collect();
    let no_solution = dcf_rows.iter().all(|r| r["implied_growth"].is_null());
    let dcf_reason = match (mcap, fcf) {
        (None, _) => Some("harga atau jumlah saham belum tersedia"),
        (_, None) if fx.is_none() => Some("kurs USD/IDR belum tersedia"),
        (_, None) => Some("arus kas bebas belum tersedia"),
        (_, Some(f)) if f <= 0.0 => Some("FCF ≤ 0: DCF terbalik tidak berlaku"),
        _ if no_solution => Some("pertumbuhan tersirat di luar rentang pencarian"),
        _ => None,
    };

    // Multiples with own-history bands.
    let mut multiples = serde_json::Map::new();
    for (key, which) in [
        ("pe", Multiple::Pe),
        ("pb", Multiple::Pb),
        ("ev_ebitda", Multiple::EvEbitda),
        ("dividend_yield", Multiple::DividendYield),
    ] {
        let mut variants = serde_json::Map::new();
        let mut bases = vec![("ttm".to_string(), Basis::Ttm)];
        for w in &rules.cycle_windows {
            bases.push((format!("cycle{w}y"), Basis::Cycle(*w)));
        }
        for (name, basis) in bases {
            let compute = |use_cpi: bool| {
                let series: Vec<(NaiveDate, f64, bool)> = days
                    .iter()
                    .filter_map(|d| {
                        multiple_on(inputs, rules, d, which, basis, use_cpi)
                            .map(|(v, a)| (d.date, v, a))
                    })
                    .collect();
                let current = today
                    .as_ref()
                    .and_then(|d| multiple_on(inputs, rules, d, which, basis, use_cpi));
                (series, current)
            };
            let (mut series, mut current) = compute(true);
            if matches!(basis, Basis::Cycle(_)) {
                // One basis for the whole band: when any day could not be CPI-adjusted,
                // every point is nominal and the label says so (review R3).
                let any_adjusted = series.iter().any(|s| s.2) || current.is_some_and(|c| c.1);
                let all_adjusted = series.iter().all(|s| s.2) && current.is_none_or(|c| c.1);
                if any_adjusted && !all_adjusted {
                    (series, current) = compute(false);
                }
            }
            let history: Vec<f64> = series.iter().map(|s| s.1).collect();
            let b = band(&history, current, rules, matches!(basis, Basis::Cycle(_)));
            variants.insert(name, json!(b));
        }
        multiples.insert(key.into(), Value::Object(variants));
    }

    // Earnings yield vs the 10-year bond.
    let ey = today.as_ref().and_then(|d| earnings_yield_on(inputs, d));
    let bond = date.and_then(|d| value_on(&inputs.bond_10y, d));
    let spread_hist: Vec<f64> = days
        .iter()
        .filter_map(|d| {
            let e = earnings_yield_on(inputs, d)?;
            let b = value_on(&inputs.bond_10y, d.date)?;
            Some(e - b)
        })
        .collect();
    let spread = ey.zip(bond).map(|(e, b)| e - b);
    let spread_band = band(&spread_hist, spread.map(|s| (s, false)), rules, false);

    // Scenarios.
    let revenue_idr = known
        .and_then(|k| k.revenue_ttm)
        .zip(fx)
        .map(|(v, fx)| v * fx);
    let base_rate = rules.base_discount_rate();
    let scenarios: Vec<Value> = ["bear", "base", "bull"]
        .iter()
        .map(|name| {
            let s = inputs.scenarios.iter().find(|s| s.name == *name);
            let value = s.and_then(|s| {
                scenario_value(s, revenue_idr, shares.as_ref().map(|r| r.shares), base_rate)
            });
            let gap = value.zip(price).map(|(v, p)| 100.0 * (v / p - 1.0));
            json!({
                "name": name,
                "growth": s.and_then(|s| s.growth),
                "margin": s.and_then(|s| s.margin),
                "exit_multiple": s.and_then(|s| s.exit_multiple),
                "years": s.and_then(|s| s.years),
                "value_per_share": value,
                "gap_pct": gap,
                "updated_at": s.map(|s| s.updated_at.clone()),
                "action": if s.is_none() { Some("Isi skenario") } else { None },
            })
        })
        .collect();

    json!({
        "ticker": inputs.ticker,
        "as_of": now,
        "rules_version": rules.version,
        "currency": inputs.currency,
        "fx_usd_idr": if inputs.currency.eq_ignore_ascii_case("USD") { fx } else { None },
        "price": {"close": price, "date": date},
        "shares": shares.as_ref().map(|r| json!({"count": r.shares, "source": r.source, "source_label": r.source.label()})),
        "market_cap": mcap,
        "latest_quarter": known.map(|k| json!({"fy": k.fy, "q": k.q, "label": format!("{}Q{}", k.fy, k.q), "available_from": k.available_from})),
        "history": {"from": days.first().map(|d| d.date), "to": date, "days": days.len()},
        "reverse_dcf": {
            "fcf_base": fcf_base.as_str(),
            "fcf": fcf,
            "fcf_ttm": fcf_ttm,
            "fcf_avg5y": fcf_avg5y,
            "horizon_years": rules.horizon_years,
            "terminal_growth": rules.terminal_growth,
            "rows": dcf_rows,
            "applicable": dcf_reason.is_none(),
            "reason": dcf_reason,
        },
        "multiples": multiples,
        "earnings_yield": {
            "earnings_yield": ey,
            "bond_yield": bond,
            "bond_status": if bond.is_some() { "tersedia" } else { "TBD" },
            "bond_driver": "sbn_10y",
            "spread": spread,
            "spread_band": spread_band,
        },
        "scenarios": {
            "discount_rate": base_rate,
            "rows": scenarios,
        },
    })
}

/// Scenario rows as stored, for the PUT reply.
pub fn scenario_from_row(r: &ScenarioRow) -> Scenario {
    Scenario {
        name: r.name.clone(),
        growth: r.growth,
        margin: r.margin,
        exit_multiple: r.exit_multiple,
        years: r.years,
        updated_at: r.updated_at.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn embedded_rules_parse_and_validate() {
        let r = Rules::embedded();
        assert_eq!(r.discount_rates, vec![9.0, 11.0, 13.0]);
        assert_eq!(r.base_discount_rate(), 11.0);
        assert_eq!(r.version, "2026.10-r3");
        assert_eq!(r.pojk_offsets.fy, 90);
        let bad = EMBEDDED_RULES.replace("terminal_growth = 3.0", "terminal_growth = 12.0");
        assert!(Rules::parse(&bad).is_err());
        let bad =
            EMBEDDED_RULES.replace("discount_rates = [9.0, 11.0, 13.0]", "discount_rates = []");
        assert!(Rules::parse(&bad).is_err());
        let unknown = format!("{EMBEDDED_RULES}\nfair_value = 1\n");
        assert!(Rules::parse(&unknown).is_err(), "unknown keys are refused");
    }

    #[test]
    fn rules_load_from_a_directory_and_fall_back_to_embedded() {
        let dir = tempfile::tempdir().unwrap();
        let l = LoadedRules::load(dir.path(), "now");
        assert_eq!(l.source, "embedded");
        assert!(l.error.is_none());
        std::fs::write(
            dir.path().join(RULES_FILE),
            EMBEDDED_RULES
                .replace("[9.0, 11.0, 13.0]", "[8.0, 10.0, 12.0]")
                .replace("version = \"2026.10-r3\"", "version = \"2026.11-r4\""),
        )
        .unwrap();
        let l = LoadedRules::load(dir.path(), "now");
        assert_eq!(l.rules.discount_rates, vec![8.0, 10.0, 12.0]);
        assert_eq!(l.rules.version, "2026.11-r4");
        assert!(l.source.ends_with(RULES_FILE));
        std::fs::write(dir.path().join(RULES_FILE), "version = 1\n").unwrap();
        let l = LoadedRules::load(dir.path(), "now");
        assert_eq!(l.source, "embedded");
        assert!(l.error.is_some());
    }

    #[test]
    fn implied_growth_solves_and_refuses_non_positive_fcf() {
        let fcf = 100.0;
        let (r, n, tg) = (0.11, 10, 0.03);
        for g in [-0.2, 0.0, 0.05, 0.3] {
            let mcap = present_value(fcf, g, r, n, tg);
            let solved = implied_growth(mcap, fcf, r, n, tg).unwrap();
            assert!((solved - g).abs() < 1e-7, "{g} vs {solved}");
        }
        assert_eq!(implied_growth(1000.0, 0.0, r, n, tg), None);
        assert_eq!(implied_growth(1000.0, -5.0, r, n, tg), None);
        assert_eq!(
            implied_growth(1000.0, 100.0, 0.02, n, tg),
            None,
            "rate <= terminal"
        );
        assert_eq!(
            implied_growth(1e-9, 100.0, r, n, tg),
            None,
            "below the bracket"
        );
        // A higher discount rate needs more growth for the same market cap.
        let mcap = present_value(fcf, 0.05, 0.09, n, tg);
        let g9 = implied_growth(mcap, fcf, 0.09, n, tg).unwrap();
        let g13 = implied_growth(mcap, fcf, 0.13, n, tg).unwrap();
        assert!(g13 > g9);
    }

    #[test]
    fn scenario_value_needs_every_input_and_discounts_the_exit() {
        let s = Scenario {
            name: "base".into(),
            growth: Some(10.0),
            margin: Some(20.0),
            exit_multiple: Some(10.0),
            years: Some(5),
            updated_at: "t".into(),
        };
        let v = scenario_value(&s, Some(1_000.0), Some(100.0), 11.0).unwrap();
        let expect = 1_000.0 * 1.1f64.powi(5) * 0.2 * 10.0 / 1.11f64.powi(5) / 100.0;
        assert!((v - expect).abs() < 1e-9);
        let mut partial = s.clone();
        partial.years = None;
        assert_eq!(
            scenario_value(&partial, Some(1_000.0), Some(100.0), 11.0),
            None
        );
        assert_eq!(scenario_value(&s, None, Some(100.0), 11.0), None);
        assert_eq!(scenario_value(&s, Some(1_000.0), Some(0.0), 11.0), None);
    }

    fn q(fy: i32, qn: u32, avail: &str, ni: f64, eq: f64) -> QuarterInputs {
        QuarterInputs {
            fy,
            q: qn,
            period_end: quarter_end_date(Quarter::new(fy, qn)).unwrap(),
            available_from: d(avail),
            revenue_ttm: Some(ni * 5.0),
            net_income_ttm: Some(ni),
            ebitda_ttm: Some(ni * 1.5),
            fcf_ttm: Some(ni * 0.8),
            equity: Some(eq),
            net_debt: Some(-eq * 0.1),
        }
    }

    fn sample() -> Inputs {
        let mut prices = Vec::new();
        let mut day = d("2021-01-04");
        let mut i = 0;
        while day <= d("2026-10-23") {
            if day.weekday().num_days_from_monday() < 5 {
                prices.push((day, 1_000.0 + (i % 200) as f64));
                i += 1;
            }
            day = day.succ_opt().unwrap();
        }
        Inputs {
            ticker: "AAAA".into(),
            currency: "IDR".into(),
            prices,
            shares: ShareSeries {
                manual: vec![super::super::shares::ManualShares {
                    as_of: d("2020-12-31"),
                    shares: 1_000.0,
                    source_note: "x".into(),
                }],
                ..Default::default()
            },
            quarters: vec![
                q(2021, 4, "2022-03-31", 90_000.0, 500_000.0),
                q(2022, 4, "2023-03-31", 100_000.0, 550_000.0),
                q(2023, 4, "2024-03-31", 110_000.0, 600_000.0),
                q(2024, 4, "2025-03-31", 120_000.0, 650_000.0),
                q(2025, 4, "2026-03-31", 130_000.0, 700_000.0),
                q(2026, 2, "2026-08-29", 135_000.0, 720_000.0),
            ],
            dividends: vec![(d("2026-05-10"), 40.0), (d("2025-05-10"), 35.0)],
            cpi: vec![
                (d("2021-12-01"), 100.0),
                (d("2022-12-01"), 105.0),
                (d("2023-12-01"), 108.0),
                (d("2024-12-01"), 110.0),
                (d("2025-12-01"), 112.0),
                (d("2026-09-01"), 114.0),
            ],
            bond_10y: vec![(d("2026-10-01"), 6.5)],
            usd_idr: vec![],
            splits: vec![],
            scenarios: vec![],
        }
    }

    #[test]
    fn point_in_time_fundamentals_and_cycle_adjustment() {
        let inputs = sample();
        assert_eq!(
            inputs.known(d("2022-03-30")),
            None,
            "a day before the FY filing"
        );
        assert_eq!(inputs.known(d("2022-03-31")).unwrap().fy, 2021);
        assert_eq!(inputs.known(d("2023-04-01")).unwrap().fy, 2022);
        assert_eq!(inputs.known(d("2021-06-01")), None);
        let fys = inputs.fiscal_years(d("2026-10-23"), 5);
        assert_eq!(
            fys.iter().map(|q| q.fy).collect::<Vec<_>>(),
            [2025, 2024, 2023, 2022, 2021]
        );
        let rules = Rules::embedded();
        let (avg, adjusted) =
            cycle_average(&inputs, d("2026-10-23"), 5, 3, true, |q| q.net_income_ttm).unwrap();
        assert!(adjusted);
        let nominal = (90_000.0 + 100_000.0 + 110_000.0 + 120_000.0 + 130_000.0) / 5.0;
        assert!(avg > nominal, "older years are scaled up by CPI");
        let mut no_cpi = inputs.clone();
        no_cpi.cpi.clear();
        let (avg2, adjusted2) =
            cycle_average(&no_cpi, d("2026-10-23"), 5, 3, true, |q| q.net_income_ttm).unwrap();
        assert!(!adjusted2);
        assert!((avg2 - nominal).abs() < 1e-9);
        assert_eq!(
            cycle_average(&inputs, d("2023-06-01"), 10, 3, true, |q| q.net_income_ttm),
            None,
            "two years only"
        );
        let today = day_states(&inputs).last().cloned().unwrap();
        let (pe, _) = multiple_on(&inputs, &rules, &today, Multiple::Pe, Basis::Ttm, true).unwrap();
        assert!((pe - today.price * 1_000.0 / 135_000.0).abs() < 1e-9);
        let (dy, _) = multiple_on(
            &inputs,
            &rules,
            &today,
            Multiple::DividendYield,
            Basis::Ttm,
            true,
        )
        .unwrap();
        assert!((dy - 100.0 * 40.0 / today.price).abs() < 1e-9);
        let (ev, _) = multiple_on(
            &inputs,
            &rules,
            &today,
            Multiple::EvEbitda,
            Basis::Ttm,
            true,
        )
        .unwrap();
        let ev_expect = (today.price * 1_000.0 - 72_000.0) / (135_000.0 * 1.5);
        assert!((ev - ev_expect).abs() < 1e-9);
    }

    #[test]
    fn a_usd_filer_needs_the_fx_driver_and_the_payload_has_no_fair_value() {
        let mut inputs = sample();
        inputs.currency = "USD".into();
        let rules = Rules::embedded();
        let v = build(&inputs, &rules, FcfBase::Ttm, "now");
        assert_eq!(v["reverse_dcf"]["applicable"], false);
        assert_eq!(v["reverse_dcf"]["reason"], "kurs USD/IDR belum tersedia");
        assert!(v["multiples"]["pe"]["ttm"]["value"].is_null());
        inputs.usd_idr = vec![(d("2021-01-01"), 15_000.0)];
        let v = build(&inputs, &rules, FcfBase::Ttm, "now");
        // Converted FCF dwarfs the sample market cap: outside the search bracket, said so.
        assert_eq!(v["reverse_dcf"]["applicable"], false);
        assert_eq!(
            v["reverse_dcf"]["reason"],
            "pertumbuhan tersirat di luar rentang pencarian"
        );
        assert_eq!(v["fx_usd_idr"], 15_000.0);
        let idr = build(&sample(), &rules, FcfBase::Ttm, "now");
        assert_eq!(
            idr["reverse_dcf"]["applicable"], true,
            "{}",
            idr["reverse_dcf"]
        );
        assert!(idr["reverse_dcf"]["rows"][0]["implied_growth"].is_number());
        let text = v.to_string().to_lowercase();
        for word in ["fair", "wajar", "murah", "mahal", "layak", "beli", "jual"] {
            assert!(!text.contains(word), "{word} in payload");
        }
        assert_eq!(v["rules_version"], "2026.10-r3");
        assert_eq!(v["earnings_yield"]["bond_status"], "tersedia");
        assert_eq!(v["scenarios"]["rows"][0]["action"], "Isi skenario");
        assert_eq!(v["reverse_dcf"]["rows"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn a_split_moves_earlier_share_counts_onto_the_adjusted_price_basis() {
        let mut inputs = sample();
        // 2:1 split on 2025-01-10: closes before it are already halved by Yahoo, so the
        // 1 000 shares resolved for those days count as 2 000 on today's basis.
        inputs.splits = vec![(d("2025-01-10"), 2.0)];
        let days = day_states(&inputs);
        let before = days.iter().find(|x| x.date == d("2024-12-30")).unwrap();
        let after = days.iter().find(|x| x.date == d("2025-01-13")).unwrap();
        assert_eq!(before.shares, Some(2_000.0));
        assert_eq!(after.shares, Some(1_000.0));
        assert_eq!(
            split_factor(&inputs.splits, d("2025-01-10")),
            1.0,
            "the split day is on the new basis"
        );
        // The newest quarter wins over an amended older filing that became available later.
        inputs.quarters.push(QuarterInputs {
            available_from: d("2026-09-15"),
            ..q(2025, 4, "2026-09-15", 131_000.0, 701_000.0)
        });
        assert_eq!(
            inputs.known(d("2026-10-01")).map(|k| (k.fy, k.q)),
            Some((2026, 2))
        );
    }

    #[test]
    fn empty_inputs_render_gaps_not_errors() {
        let inputs = Inputs {
            ticker: "ZZZZ".into(),
            currency: "IDR".into(),
            ..Default::default()
        };
        let v = build(&inputs, &Rules::embedded(), FcfBase::Avg5y, "now");
        assert!(v["price"]["close"].is_null());
        assert_eq!(v["reverse_dcf"]["applicable"], false);
        assert_eq!(v["reverse_dcf"]["fcf_base"], "avg5y");
        assert_eq!(v["multiples"]["pe"]["ttm"]["n"], 0);
        assert_eq!(v["earnings_yield"]["bond_status"], "TBD");
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// Implied growth is monotone in price (market cap) for a fixed FCF and rate.
        #[test]
        fn implied_growth_is_monotone_in_price(
            fcf in 1.0f64..1e9,
            a in 0.1f64..1e4,
            b in 0.1f64..1e4,
            rate in 0.05f64..0.3,
        ) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let (m_lo, m_hi) = (fcf * lo, fcf * hi);
            let g_lo = implied_growth(m_lo, fcf, rate, 10, 0.03);
            let g_hi = implied_growth(m_hi, fcf, rate, 10, 0.03);
            if let (Some(x), Some(y)) = (g_lo, g_hi) {
                prop_assert!(x <= y + 1e-9, "{x} > {y}");
                if hi > lo * 1.01 {
                    prop_assert!(x < y, "strictly increasing: {x} {y}");
                }
            }
        }

        /// Present value grows with growth and shrinks with the discount rate.
        #[test]
        fn present_value_is_monotone(fcf in 1.0f64..1e6, g in -0.5f64..1.0, r in 0.05f64..0.4) {
            let pv = present_value(fcf, g, r, 10, 0.03);
            prop_assert!(pv > 0.0 && pv.is_finite());
            prop_assert!(present_value(fcf, g + 0.01, r, 10, 0.03) > pv);
            prop_assert!(present_value(fcf, g, r + 0.01, 10, 0.03) < pv);
        }

        /// Percentiles stay within [0, 100] and the band is consistent with its history.
        #[test]
        fn percentiles_are_within_0_and_100(
            history in prop::collection::vec(0.1f64..100.0, 750..900),
            current in 0.1f64..100.0,
        ) {
            let rules = Rules::embedded();
            let b = band(&history, Some((current, false)), &rules, false);
            let p = b.percentile.unwrap();
            prop_assert!((0.0..=100.0).contains(&p));
            prop_assert!(b.min.unwrap() <= b.median.unwrap() && b.median.unwrap() <= b.max.unwrap());
            prop_assert!(b.caption.is_none());
            let short = band(&history[..100], Some((current, false)), &rules, false);
            prop_assert!(short.percentile.is_none() && short.caption.is_some());
        }
    }
}
