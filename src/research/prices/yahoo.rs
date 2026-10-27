//! Yahoo chart v8 adapter (research plan D12, R3.1; unofficial endpoint, personal
//! low-frequency use): daily bars plus dividend and split events for `{T}.JK`, the `^JKSE`
//! index, and the driver symbols the spike found (`IDR=X`, `GC=F`, `BZ=F`, `BWET`).
//!
//! Ten-year backfill when a ticker has no bars, else a short top-up. One request per second
//! at most, exponential backoff with three retries on 429 and 5xx, no redirects (the shared
//! client refuses them). A failed or malformed fetch leaves stored bars untouched and is
//! recorded in `source_runs`. `query2` is used because `query1` throttled first in the spike.

use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as ChronoDuration, NaiveDate};
use serde_json::Value;
use tokio::sync::Mutex;

use super::{ChartData, PriceSource};
use crate::http::{Http, PyErr};
use crate::research::sources::{record_run, SourceError};
use crate::research::store::{PriceBar, Store};

pub const SOURCE: &str = "yahoo";
pub const BASE: &str = "https://query2.finance.yahoo.com/v8/finance/chart";
pub const INDEX_SYMBOL: &str = "^JKSE";
const RETRIES: u32 = 3;
const BACKFILL_RANGE: &str = "10y";
const INCREMENTAL_RANGE: &str = "3mo";

pub struct YahooSource {
    http: Http,
    base: String,
    gate: Mutex<Option<Instant>>,
    min_gap: Duration,
    backoff: Duration,
}

impl YahooSource {
    pub fn new(base: Option<&str>) -> Self {
        Self {
            http: Http::new(Some("Mozilla/5.0")),
            base: base.unwrap_or(BASE).trim_end_matches('/').to_string(),
            gate: Mutex::new(None),
            min_gap: Duration::from_secs(1),
            backoff: Duration::from_secs(2),
        }
    }

    /// Tests shrink the pacing; production keeps 1 req/s and 2 s base backoff.
    pub fn with_timing(mut self, min_gap: Duration, backoff: Duration) -> Self {
        self.min_gap = min_gap;
        self.backoff = backoff;
        self
    }

    async fn pace(&self) {
        let mut last = self.gate.lock().await;
        if let Some(t) = *last {
            let wait = self.min_gap.saturating_sub(t.elapsed());
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
        }
        *last = Some(Instant::now());
    }
}

/// `BBCA` → `BBCA.JK`; an index (`^JKSE`) or a driver symbol (`IDR=X`, `GC=F`) is passed
/// through as is.
pub fn symbol_for(ticker: &str) -> String {
    if ticker.starts_with('^') || ticker.contains('=') {
        ticker.to_string()
    } else {
        format!("{ticker}.JK")
    }
}

fn retryable(e: &PyErr) -> bool {
    if e.kind != "HTTPStatusError" {
        return true;
    }
    // 429 and 5xx are worth another try; other 4xx and redirects are not.
    e.msg.starts_with("Server error") || e.msg.contains("'429 ")
}

impl PriceSource for YahooSource {
    async fn fetch(&self, symbol: &str, range: &str) -> Result<ChartData, SourceError> {
        let url = format!(
            "{}/{}?range={range}&interval=1d&events=div%7Csplit",
            self.base,
            symbol.replace('^', "%5E").replace('=', "%3D")
        );
        let mut last = SourceError("no attempt".into());
        for attempt in 0..=RETRIES {
            if attempt > 0 {
                tokio::time::sleep(self.backoff * 2u32.pow(attempt - 1)).await;
            }
            self.pace().await;
            let res = match self.http.get(&url).await {
                Ok(r) => r.json().map_err(|e| (false, e)),
                Err(e) => Err((retryable(&e), e)),
            };
            match res {
                Ok(j) => return parse_chart(&j),
                Err((again, e)) => {
                    last = SourceError(format!("{symbol}: {}: {}", e.kind, first_line(&e.msg)));
                    if !again {
                        break;
                    }
                }
            }
        }
        Err(last)
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

fn local_date(ts: i64, offset: i64) -> Option<String> {
    DateTime::from_timestamp(ts + offset, 0).map(|d| d.date_naive().format("%Y-%m-%d").to_string())
}

fn series(q: &Value, key: &str) -> Vec<Option<f64>> {
    q.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().map(Value::as_f64).collect())
        .unwrap_or_default()
}

/// Parses a v8 chart response. Bars with a missing or non-positive close are dropped; a
/// response without any usable bar is an error rather than an empty success.
pub fn parse_chart(j: &Value) -> Result<ChartData, SourceError> {
    let chart = j
        .get("chart")
        .ok_or_else(|| SourceError("response has no chart".into()))?;
    if let Some(err) = chart.get("error").filter(|e| !e.is_null()) {
        let text = err
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return Err(SourceError(format!("yahoo error: {text}")));
    }
    let res = chart
        .get("result")
        .and_then(|r| r.get(0))
        .ok_or_else(|| SourceError("response has no result".into()))?;
    let offset = res
        .pointer("/meta/gmtoffset")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let ts: Vec<i64> = res
        .get("timestamp")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let quote = res
        .pointer("/indicators/quote/0")
        .cloned()
        .unwrap_or(Value::Null);
    let (open, high, low, close) = (
        series(&quote, "open"),
        series(&quote, "high"),
        series(&quote, "low"),
        series(&quote, "close"),
    );
    let volume: Vec<Option<i64>> = quote
        .get("volume")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(Value::as_i64).collect())
        .unwrap_or_default();
    let at = |v: &Vec<Option<f64>>, i: usize| v.get(i).copied().flatten();
    let mut out = ChartData::default();
    for (i, t) in ts.iter().enumerate() {
        let (Some(c), Some(date)) = (at(&close, i), local_date(*t, offset)) else {
            continue;
        };
        if !c.is_finite() || c <= 0.0 {
            continue;
        }
        out.bars.push(PriceBar {
            date,
            open: at(&open, i),
            high: at(&high, i),
            low: at(&low, i),
            close: c,
            volume: volume.get(i).copied().flatten(),
        });
    }
    if out.bars.is_empty() {
        return Err(SourceError("response has no price bars".into()));
    }
    let events = res.get("events");
    for v in events
        .and_then(|e| e.get("dividends"))
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|m| m.values())
    {
        if let (Some(a), Some(d)) = (
            v.get("amount").and_then(Value::as_f64),
            v.get("date")
                .and_then(Value::as_i64)
                .and_then(|t| local_date(t, offset)),
        ) {
            out.dividends.push((d, a));
        }
    }
    for v in events
        .and_then(|e| e.get("splits"))
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|m| m.values())
    {
        if let (Some(n), Some(den), Some(d)) = (
            v.get("numerator").and_then(Value::as_f64),
            v.get("denominator").and_then(Value::as_f64),
            v.get("date")
                .and_then(Value::as_i64)
                .and_then(|t| local_date(t, offset)),
        ) {
            if n > 0.0 && den > 0.0 {
                out.splits.push((d, n, den));
            }
        }
    }
    out.dividends.sort_by(|a, b| a.0.cmp(&b.0));
    out.splits.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Range to ask for given the days since the last stored bar: a short top-up most days,
/// a longer window after an outage, the full backfill when nothing is stored (review R3).
pub fn range_for_gap(days_since_last: Option<i64>) -> &'static str {
    match days_since_last {
        None => BACKFILL_RANGE,
        Some(d) if d <= 80 => INCREMENTAL_RANGE,
        Some(d) if d <= 360 => "1y",
        Some(d) if d <= 1800 => "5y",
        Some(_) => BACKFILL_RANGE,
    }
}

/// Fetches and stores one ticker: a 10-year backfill when no bars exist yet, else a window
/// sized by the gap since the last bar. Yahoo's closes are split-adjusted, so a split the
/// store has not seen yet triggers a full refetch and every stored bar is rewritten on the
/// new basis. On failure the store is untouched and the failed run is recorded.
pub async fn ingest<P: PriceSource>(
    store: &Store,
    src: &P,
    ticker: &str,
    now: DateTime<chrono::Utc>,
) -> Result<usize, SourceError> {
    let started = now.to_rfc3339();
    let last_bar = store
        .last_closes(ticker, 1)
        .ok()
        .and_then(|v| v.first().map(|(d, _)| d.clone()))
        .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let range = range_for_gap(last_bar.map(|d| (now.date_naive() - d).num_days()));
    let outcome = async {
        let mut data = src.fetch(&symbol_for(ticker), range).await?;
        if range != BACKFILL_RANGE {
            let known: std::collections::BTreeSet<String> = store
                .events(ticker)?
                .into_iter()
                .filter(|e| e.kind == crate::research::store::EventKind::Split)
                .map(|e| e.date)
                .collect();
            if data.splits.iter().any(|(d, _, _)| !known.contains(d)) {
                tracing::info!("research prices {ticker}: new split, refetching the full history");
                data = src.fetch(&symbol_for(ticker), BACKFILL_RANGE).await?;
            }
        }
        store.upsert_prices(ticker, &data.bars)?;
        store.upsert_events(&data.events(ticker))?;
        Ok(data.bars.len())
    }
    .await;
    record_run(
        store,
        SOURCE,
        Some(ticker),
        &started,
        &chrono::Utc::now().to_rfc3339(),
        &outcome,
    );
    outcome
}

/// Days a price series may lag before it counts as stale.
pub const STALE_DAYS: i64 = 4;

pub fn is_stale(last_bar: &str, today: NaiveDate) -> bool {
    NaiveDate::parse_from_str(last_bar, "%Y-%m-%d")
        .map(|d| today - d > ChronoDuration::days(STALE_DAYS))
        .unwrap_or(true)
}
