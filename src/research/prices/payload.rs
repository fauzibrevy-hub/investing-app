//! `GET /api/research/{ticker}/prices?range=1y|3y|5y|10y` (research plan §5, R3.5;
//! design.md §4.2 Harga): price and MA200 over the range, the pack's driver series on the
//! secondary axis (TBD drivers listed with their labelled proxy), the index relative,
//! journal levels, dividends and splits, and the six
//! `StatRow`s: close, 52-week high and drawdown, 1-year volatility, 6-month return against
//! the index, dividend yield TTM, average cost if held. Pure over `PriceInputs`.

use chrono::{Months, NaiveDate};
use serde_json::{json, Value};

use super::drivers::{self, DriverSpec};
use crate::research::journal::LevelMark;
use crate::research::packs::payload::{driver_index, driver_status, DriverIndex};
use crate::research::packs::{Driver, PackSet};
use crate::research::store::{EventKind, Store, StoreError};
use crate::research::universe::{self, UniverseEntry};

pub type Series = Vec<(NaiveDate, f64)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    Y1,
    Y3,
    Y5,
    Y10,
}

impl Range {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "1y" => Some(Self::Y1),
            "3y" => Some(Self::Y3),
            "5y" => Some(Self::Y5),
            "10y" => Some(Self::Y10),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Y1 => "1y",
            Self::Y3 => "3y",
            Self::Y5 => "5y",
            Self::Y10 => "10y",
        }
    }

    pub fn months(self) -> u32 {
        match self {
            Self::Y1 => 12,
            Self::Y3 => 36,
            Self::Y5 => 60,
            Self::Y10 => 120,
        }
    }
}

/// One pack driver with the stored series behind it (own or proxy).
#[derive(Clone, Debug)]
pub struct DriverSeries {
    pub driver: Driver,
    pub spec: Option<&'static DriverSpec>,
    pub series: Series,
    pub proxy: Option<(&'static DriverSpec, Series)>,
}

#[derive(Clone, Debug, Default)]
pub struct PriceInputs {
    pub ticker: String,
    pub role: String,
    /// Full daily history, ascending.
    pub prices: Series,
    pub index: Series,
    pub dividends: Series,
    /// `(date, numerator, denominator)`.
    pub splits: Vec<(NaiveDate, f64, f64)>,
    pub drivers: Vec<DriverSeries>,
    pub driver_index: DriverIndex,
    pub avg_cost: Option<f64>,
    pub pack_id: Option<String>,
    /// The owner's journal levels, oldest first (R4.2).
    pub levels: Vec<LevelMark>,
}

fn parse_series(rows: Vec<(String, f64)>) -> Series {
    rows.into_iter()
        .filter_map(|(d, v)| {
            NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .ok()
                .map(|d| (d, v))
        })
        .collect()
}

impl PriceInputs {
    pub fn load(
        store: &Store,
        packs: &PackSet,
        entry: &UniverseEntry,
        today: NaiveDate,
    ) -> Result<Self, StoreError> {
        let ticker = entry.ticker.as_str();
        let bars = |t: &str| -> Result<Series, StoreError> {
            Ok(parse_series(
                store
                    .prices(t)?
                    .into_iter()
                    .map(|b| (b.date, b.close))
                    .collect(),
            )
            .into_iter()
            .filter(|(d, _)| *d <= today)
            .collect())
        };
        let prices = bars(ticker)?;
        let index = bars(super::yahoo::INDEX_SYMBOL)?;
        let (mut dividends, mut splits) = (Vec::new(), Vec::new());
        for e in store.events(ticker)? {
            let Ok(d) = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d") else {
                continue;
            };
            match e.kind {
                EventKind::Dividend => {
                    if let Some(a) = e.amount {
                        dividends.push((d, a));
                    }
                }
                EventKind::Split => {
                    if let (Some(n), Some(den)) = (e.ratio_num, e.ratio_den) {
                        splits.push((d, n, den));
                    }
                }
            }
        }
        let codes = store.latest_industry_codes()?;
        let choice = universe::pack_choice(packs, entry, codes.get(ticker).map(String::as_str));
        let industry = choice.id.as_deref().and_then(|id| packs.get(id));
        let from = today
            .checked_sub_months(Months::new(Range::Y10.months()))
            .unwrap_or(today)
            .format("%Y-%m-%d")
            .to_string();
        let mut out = Vec::new();
        for d in industry.map(|p| p.drivers.as_slice()).unwrap_or_default() {
            let series = parse_series(store.driver_series(&d.id, Some(&from))?);
            let proxy = match d.proxy.as_deref().and_then(drivers::spec) {
                Some(p) => Some((p, parse_series(store.driver_series(p.id, Some(&from))?))),
                None => None,
            };
            out.push(DriverSeries {
                driver: d.clone(),
                spec: drivers::spec(&d.id),
                series,
                proxy,
            });
        }
        Ok(Self {
            ticker: ticker.into(),
            role: entry.role.as_str().into(),
            prices,
            index,
            dividends,
            splits,
            drivers: out,
            driver_index: driver_index(store)?,
            avg_cost: store.avg_price(ticker)?,
            pack_id: choice.id,
            levels: level_marks(store, ticker)?,
        })
    }
}

/// `(date, ratio)` of every split of the ticker, for moving a price or a level written
/// before a split onto today's split-adjusted basis (`valuation::split_factor`).
pub fn split_ratios(store: &Store, ticker: &str) -> Result<Vec<(NaiveDate, f64)>, StoreError> {
    Ok(store
        .events(ticker)?
        .into_iter()
        .filter(|e| e.kind == EventKind::Split)
        .filter_map(|e| {
            let date = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok()?;
            Some((date, e.ratio_num? / e.ratio_den.filter(|d| *d > 0.0)?))
        })
        .collect())
}

/// The ticker's journal levels, oldest first, on the split-adjusted basis of the stored
/// closes (each level divided by the splits since the day it was written). A level whose
/// `at` does not parse as a timestamp is skipped rather than dated wrongly.
pub fn level_marks(store: &Store, ticker: &str) -> Result<Vec<LevelMark>, StoreError> {
    let splits = split_ratios(store, ticker)?;
    let mut marks: Vec<LevelMark> = store
        .journal_levels(ticker)?
        .into_iter()
        .filter_map(|j| {
            let at = crate::research::journal::wib_day(&j.at)?;
            let entered = j.level?;
            let basis = crate::research::valuation::split_factor(&splits, at);
            Some(LevelMark {
                at,
                level: entered / basis,
                entered,
                id: j.id,
                text: j.text,
            })
        })
        .collect();
    marks.sort_by(|a, b| (a.at, &a.id).cmp(&(b.at, &b.id)));
    Ok(marks)
}

/// Simple moving average of `n` points, aligned with `series`; `None` until `n` points.
pub fn moving_average(series: &[(NaiveDate, f64)], n: usize) -> Vec<(NaiveDate, Option<f64>)> {
    let mut out = Vec::with_capacity(series.len());
    let mut sum = 0.0;
    for (i, (d, c)) in series.iter().enumerate() {
        sum += c;
        if i >= n {
            sum -= series[i - n].1;
        }
        out.push((*d, (i + 1 >= n).then(|| sum / n as f64)));
    }
    out
}

/// Latest value on or before `on`.
pub fn value_on(series: &[(NaiveDate, f64)], on: NaiveDate) -> Option<f64> {
    let i = series.partition_point(|(d, _)| *d <= on);
    (i > 0).then(|| series[i - 1].1)
}

/// `price / index`, rebased to 100 at the first day of the range where both exist.
pub fn index_relative(prices: &[(NaiveDate, f64)], index: &[(NaiveDate, f64)]) -> Series {
    let mut base = None;
    let mut out = Vec::new();
    for (d, p) in prices {
        let Some(i) = value_on(index, *d).filter(|i| *i > 0.0) else {
            continue;
        };
        let ratio = p / i;
        let b = *base.get_or_insert(ratio);
        out.push((*d, 100.0 * ratio / b));
    }
    out
}

/// Highest close in the twelve months ending on the last day, with its date and the
/// drawdown of the last close from it (percent, ≤ 0).
pub fn high_52w(prices: &[(NaiveDate, f64)]) -> Option<(NaiveDate, f64, f64)> {
    let (last_d, last_c) = *prices.last()?;
    let from = last_d.checked_sub_months(Months::new(12)).unwrap_or(last_d);
    let (d, h) = prices.iter().filter(|(d, _)| *d > from).fold(
        None,
        |best: Option<(NaiveDate, f64)>, (d, c)| match best {
            Some((_, h)) if h >= *c => best,
            _ => Some((*d, *c)),
        },
    )?;
    Some((d, h, 100.0 * (last_c / h - 1.0)))
}

/// Annualised standard deviation of daily log returns over the last twelve months, percent.
pub fn volatility_1y(prices: &[(NaiveDate, f64)]) -> Option<f64> {
    let (last_d, _) = *prices.last()?;
    let from = last_d.checked_sub_months(Months::new(12)).unwrap_or(last_d);
    let window: Vec<f64> = prices
        .iter()
        .filter(|(d, c)| *d > from && *c > 0.0)
        .map(|(_, c)| *c)
        .collect();
    if window.len() < 20 {
        return None;
    }
    let rets: Vec<f64> = window.windows(2).map(|w| (w[1] / w[0]).ln()).collect();
    let mean = rets.iter().sum::<f64>() / rets.len() as f64;
    let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (rets.len() - 1) as f64;
    Some(100.0 * var.sqrt() * 250f64.sqrt())
}

/// Return over the last `months` months, percent, from the latest close on or before the
/// start of the window.
pub fn return_over(series: &[(NaiveDate, f64)], months: u32) -> Option<f64> {
    let (last_d, last_c) = *series.last()?;
    let start = last_d.checked_sub_months(Months::new(months))?;
    let base = value_on(series, start).filter(|b| *b > 0.0)?;
    Some(100.0 * (last_c / base - 1.0))
}

/// Dividends per share with ex-dates in the twelve months ending on `on`.
pub fn dps_ttm(dividends: &[(NaiveDate, f64)], on: NaiveDate) -> f64 {
    let from = on.checked_sub_months(Months::new(12)).unwrap_or(on);
    dividends
        .iter()
        .filter(|(d, _)| *d > from && *d <= on)
        .map(|(_, v)| v)
        .sum()
}

fn points(series: &[(NaiveDate, f64)], from: NaiveDate) -> Vec<Value> {
    series
        .iter()
        .filter(|(d, _)| *d > from)
        .map(|(d, v)| json!([d.to_string(), v]))
        .collect()
}

fn spec_json(spec: Option<&DriverSpec>, index: &DriverIndex, id: &str, has_source: bool) -> Value {
    let stored = index.get(id);
    json!({
        "unit": spec.map(|s| s.unit),
        "cadence": spec.map(|s| s.cadence.as_str()),
        "note": spec.map(|s| s.note),
        "status": driver_status(has_source, stored),
        "last": stored.map(|s| s.last.clone()),
    })
}

/// Renders the payload. `today` bounds the range; `now` is the `as_of` stamp.
pub fn build(inputs: &PriceInputs, range: Range, today: NaiveDate, now: &str) -> Value {
    let from = today
        .checked_sub_months(Months::new(range.months()))
        .unwrap_or(today);
    let ma200: Vec<Value> = moving_average(&inputs.prices, 200)
        .into_iter()
        .filter(|(d, v)| *d > from && v.is_some())
        .map(|(d, v)| json!([d.to_string(), v]))
        .collect();
    let relative = index_relative(
        &inputs
            .prices
            .iter()
            .filter(|(d, _)| *d > from)
            .copied()
            .collect::<Vec<_>>(),
        &inputs.index,
    );
    let drivers: Vec<Value> = inputs
        .drivers
        .iter()
        .map(|d| {
            let mut v = spec_json(
                d.spec,
                &inputs.driver_index,
                &d.driver.id,
                d.driver.has_source(),
            );
            let proxy = d.proxy.as_ref().map(|(p, s)| {
                let mut pv = spec_json(Some(p), &inputs.driver_index, p.id, true);
                pv["id"] = json!(p.id);
                pv["label"] = json!(p.label);
                pv["source"] = json!(p.source);
                pv["proxy"] = json!(true);
                pv["proxy_for"] = json!(p.proxy_for);
                pv["series"] = json!(points(s, from));
                pv
            });
            v["id"] = json!(d.driver.id);
            v["label"] = json!(d.driver.label);
            v["source"] = json!(d.driver.source);
            v["compare_metric"] = json!(d.driver.compare_metric);
            v["series"] = json!(points(&d.series, from));
            v["proxy"] = proxy.unwrap_or(Value::Null);
            v
        })
        .collect();
    let plotted = drivers
        .iter()
        .filter(|d| {
            !d["series"].as_array().is_some_and(Vec::is_empty)
                || !d["proxy"]["series"].as_array().is_none_or(Vec::is_empty)
        })
        .count();
    let events: Vec<Value> = inputs
        .dividends
        .iter()
        .filter(|(d, _)| *d > from)
        .map(|(d, a)| json!({"date": d.to_string(), "kind": "dividend", "amount": a}))
        .chain(
            inputs
                .splits
                .iter()
                .filter(|(d, _, _)| *d > from)
                .map(|(d, n, den)| json!({"date": d.to_string(), "kind": "split", "ratio_num": n, "ratio_den": den})),
        )
        .collect();

    let last = inputs.prices.last().copied();
    let close = last.map(|(_, c)| c);
    let high = high_52w(&inputs.prices);
    // Both legs end on the price series' last date, so a stale index fetch does not shift
    // the window (review R3).
    let vs_index = last.and_then(|(last_d, _)| {
        let p = return_over(&inputs.prices, 6)?;
        let start = last_d.checked_sub_months(Months::new(6))?;
        let i0 = value_on(&inputs.index, start).filter(|b| *b > 0.0)?;
        let i1 = value_on(&inputs.index, last_d)?;
        Some(p - 100.0 * (i1 / i0 - 1.0))
    });
    let dy = last.map(|(d, c)| 100.0 * dps_ttm(&inputs.dividends, d) / c);
    let avg_cost = inputs.avg_cost.filter(|_| inputs.role == "held");
    json!({
        "ticker": inputs.ticker,
        "as_of": now,
        "range": range.as_str(),
        "from": from.to_string(),
        "to": last.map(|(d, _)| d.to_string()),
        "pack": inputs.pack_id,
        "price": points(&inputs.prices, from),
        "ma200": ma200,
        "drivers": drivers,
        "drivers_plotted": plotted,
        "index_relative": relative.iter().map(|(d, v)| json!([d.to_string(), v])).collect::<Vec<_>>(),
        "index_symbol": super::yahoo::INDEX_SYMBOL,
        // The owner's journal levels (R4.2), drawn as dashed horizontal lines; every level
        // is listed whatever the range, each with the day it was written.
        "levels": inputs.levels.iter().map(|l| json!({
            "id": l.id, "level": l.level, "entered": l.entered, "at": l.at.to_string(),
            "text": l.text,
            "gap_pct": close.filter(|c| *c > 0.0).map(|c| 100.0 * (l.level / c - 1.0)),
        })).collect::<Vec<_>>(),
        "events": events,
        "stats": {
            "close": {"value": close, "date": last.map(|(d, _)| d.to_string())},
            "high_52w": high.map(|(d, h, dd)| json!({"value": h, "date": d.to_string(), "drawdown_pct": dd})),
            "volatility_1y_pct": volatility_1y(&inputs.prices),
            "vs_index_6m_pct": vs_index,
            "dividend_yield_ttm_pct": dy,
            "avg_cost": avg_cost.map(|a| json!({"value": a, "gap_pct": close.map(|c| 100.0 * (c / a - 1.0))})),
        },
        "empty": inputs.prices.is_empty(),
    })
}

#[cfg(test)]
mod tests {
    use chrono::Datelike;

    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn daily(from: &str, to: &str, f: impl Fn(usize) -> f64) -> Series {
        let mut day = d(from);
        let mut out = Vec::new();
        let mut i = 0;
        while day <= d(to) {
            if day.weekday().num_days_from_monday() < 5 {
                out.push((day, f(i)));
                i += 1;
            }
            day = day.succ_opt().unwrap();
        }
        out
    }

    #[test]
    fn stats_over_a_synthetic_year() {
        let prices = daily("2025-01-01", "2026-10-23", |i| 1_000.0 + (i % 100) as f64);
        let (date, high, dd) = high_52w(&prices).unwrap();
        assert_eq!(high, 1_099.0);
        assert!(date > d("2025-10-23"));
        let last = prices.last().unwrap().1;
        assert!((dd - 100.0 * (last / 1_099.0 - 1.0)).abs() < 1e-9);
        let vol = volatility_1y(&prices).unwrap();
        assert!(vol > 0.0 && vol < 200.0, "{vol}");
        let flat = daily("2025-01-01", "2026-10-23", |_| 500.0);
        assert_eq!(volatility_1y(&flat), Some(0.0));
        assert!(volatility_1y(&flat[..10]).is_none());
        let up = daily("2025-01-01", "2026-10-23", |i| 100.0 + i as f64);
        assert!(return_over(&up, 6).unwrap() > 0.0);
        assert_eq!(return_over(&up[..1], 6), None);
        let divs = vec![
            (d("2026-05-10"), 20.0),
            (d("2025-05-10"), 15.0),
            (d("2026-10-23"), 5.0),
        ];
        assert_eq!(dps_ttm(&divs, d("2026-10-23")), 25.0);
        assert_eq!(dps_ttm(&divs, d("2026-10-22")), 20.0);
    }

    #[test]
    fn moving_average_and_index_relative() {
        let prices = daily("2026-01-01", "2026-03-31", |i| 10.0 + i as f64);
        let ma = moving_average(&prices, 5);
        assert!(ma[3].1.is_none());
        assert_eq!(ma[4].1, Some(12.0));
        let index = daily("2026-01-01", "2026-03-31", |_| 2.0);
        let rel = index_relative(&prices, &index);
        assert_eq!(rel[0].1, 100.0);
        assert!((rel[1].1 - 110.0).abs() < 1e-9);
        // A price day before the index starts is skipped, not zeroed.
        let late_index = daily("2026-02-01", "2026-03-31", |_| 2.0);
        let rel = index_relative(&prices, &late_index);
        assert!(rel[0].0 >= d("2026-02-02"));
        assert_eq!(rel[0].1, 100.0);
    }

    #[test]
    fn payload_shape_with_no_prices_and_with_a_tbd_driver_proxy() {
        let inputs = PriceInputs {
            ticker: "ZZZZ".into(),
            role: "manual".into(),
            ..Default::default()
        };
        let v = build(&inputs, Range::Y1, d("2026-10-23"), "now");
        assert_eq!(v["empty"], true);
        assert!(v["stats"]["close"]["value"].is_null());
        assert_eq!(v["levels"], json!([]));
        assert_eq!(v["range"], "1y");
        assert_eq!(Range::parse("10y"), Some(Range::Y10));
        assert_eq!(Range::parse("2y"), None);
        let bwet = drivers::spec("bwet").unwrap();
        let inputs = PriceInputs {
            ticker: "SHIP".into(),
            role: "held".into(),
            prices: daily("2026-01-01", "2026-10-23", |i| 100.0 + i as f64),
            avg_cost: Some(150.0),
            drivers: vec![DriverSeries {
                driver: Driver {
                    id: "charter_index".into(),
                    label: "Indeks sewa".into(),
                    source: "TBD".into(),
                    proxy: Some("bwet".into()),
                    compare_metric: Some("day_rate_usd".into()),
                },
                spec: None,
                series: vec![],
                proxy: Some((
                    bwet,
                    daily("2026-06-01", "2026-10-23", |i| 800.0 + i as f64),
                )),
            }],
            ..Default::default()
        };
        let v = build(&inputs, Range::Y1, d("2026-10-23"), "now");
        assert_eq!(v["drivers"][0]["status"], "TBD");
        assert_eq!(v["drivers"][0]["series"], json!([]));
        assert_eq!(v["drivers"][0]["proxy"]["id"], "bwet");
        assert_eq!(v["drivers"][0]["proxy"]["proxy"], true);
        assert!(!v["drivers"][0]["proxy"]["series"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(v["drivers_plotted"], 1);
        assert_eq!(v["stats"]["avg_cost"]["value"], 150.0);
        assert!(v["stats"]["avg_cost"]["gap_pct"].as_f64().unwrap() > 0.0);
        assert!(v["stats"]["vs_index_6m_pct"].is_null(), "no index series");
    }
}
