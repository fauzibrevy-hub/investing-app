//! External driver series (research plan R3.3, `docs/research/spike-r3-drivers.md`): one
//! catalogue entry per driver the packs name, plus CPI for the cycle adjustment. Three
//! adapter families cover every free source the spike found: Yahoo chart v8 (reusing the
//! price adapter), SDMX CSV (BIS, OECD) and FRED's `fredgraph.csv`. Drivers without a
//! free source stay in the catalogue with `family = None` so the Packs screen lists them as
//! `TBD` instead of dropping them (A3.5); a labelled proxy series may stand in on the chart.
//!
//! Each adapter stores `(series, date, value, source, fetched_at)` and never overwrites a
//! stored value with a gap. Monthly observations are dated on the first of their month.
//! Values are stored as the source reports them (no splicing of methods, one method per
//! series).

use chrono::{DateTime, Months, NaiveDate, Utc};
use futures::future::BoxFuture;

use super::yahoo::YahooSource;
use super::PriceSource;
use crate::http::Http;
use crate::research::sources::{record_run, SourceError};
use crate::research::store::Store;

/// `source_runs.source` for every driver fetch; `ticker` carries the driver id.
pub const SOURCE: &str = "drivers";
/// History fetched when a series has no rows yet.
pub const BACKFILL_YEARS: u32 = 10;
/// Days a driver may lag before it counts as stale (daily series; monthly ones are
/// refreshed daily too, so the run age is what health reads).
pub const STALE_DAYS: i64 = 7;

pub const BIS_BASE: &str = "https://stats.bis.org/api/v1/data";
pub const OECD_BASE: &str = "https://sdmx.oecd.org/public/rest/data";
pub const FRED_BASE: &str = "https://fred.stlouisfed.org/graph/fredgraph.csv";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cadence {
    Daily,
    Monthly,
}

impl Cadence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "harian",
            Self::Monthly => "bulanan",
        }
    }
}

/// Which adapter fetches the series; `None` means no free source (listed as `TBD`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Yahoo chart v8 symbol, through `prices/yahoo.rs`.
    Yahoo {
        symbol: &'static str,
    },
    /// BIS SDMX data API, CSV `detail=dataonly`; `path` is `FLOW/KEY/all`.
    Bis {
        path: &'static str,
    },
    /// OECD SDMX data API, labelled CSV; `path` is `AGENCY,FLOW,VERSION/KEY`.
    Oecd {
        path: &'static str,
    },
    /// FRED `fredgraph.csv?id=`.
    Fred {
        id: &'static str,
    },
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriverSpec {
    pub id: &'static str,
    /// Indonesian UI label.
    pub label: &'static str,
    pub unit: &'static str,
    pub cadence: Cadence,
    /// Human-readable source, shown on the Packs screen; `None` is `TBD`.
    pub source: Option<&'static str>,
    pub family: Family,
    /// The driver this series stands in for (the proxy is never named as that driver).
    pub proxy_for: Option<&'static str>,
    /// Caveat shown with the series (instrument change, method, what a proxy misses).
    pub note: &'static str,
}

impl DriverSpec {
    pub fn has_source(&self) -> bool {
        self.family != Family::None
    }

    pub fn is_proxy(&self) -> bool {
        self.proxy_for.is_some()
    }
}

/// Every driver the packs name (research/packs/*.toml) plus CPI and the proxies, keyed by
/// the id the pack files use. Order is display order.
pub const CATALOGUE: &[DriverSpec] = &[
    DriverSpec {
        id: "bi_rate",
        label: "BI rate",
        unit: "%",
        cadence: Cadence::Monthly,
        source: Some("BIS SDMX WS_CBPOL M.ID"),
        family: Family::Bis {
            path: "WS_CBPOL/M.ID/all",
        },
        proxy_for: None,
        note: "Sejak 2016-08-19 BI 7-day reverse repo rate; sebelumnya BI rate (ganti instrumen, bukan pemangkasan).",
    },
    DriverSpec {
        id: "sbn_10y",
        label: "Imbal hasil SBN 10 tahun",
        unit: "%",
        cadence: Cadence::Daily,
        source: None,
        family: Family::None,
        proxy_for: None,
        note: "Tidak ada riwayat gratis: FRED dan Yahoo tidak punya, OECD berhenti 2024-12. Snapshot harian PHEI menunggu tinjauan syarat pakai.",
    },
    DriverSpec {
        id: "newcastle",
        label: "Batubara Newcastle",
        unit: "USD/t",
        cadence: Cadence::Monthly,
        source: Some("FRED PCOALAUUSDM (IMF PCPS)"),
        family: Family::Fred { id: "PCOALAUUSDM" },
        proxy_for: None,
        note: "Rata-rata bulanan IMF; berbeda sekitar 5% dari Pink Sheet Bank Dunia (metode lain, tidak dicampur).",
    },
    DriverSpec {
        id: "hba",
        label: "HBA",
        unit: "USD/t",
        cadence: Cadence::Monthly,
        source: None,
        family: Family::None,
        proxy_for: None,
        note: "Hanya PDF Kepmen ESDM dan berita; laman harga acuan Minerba sedang dalam perbaikan. Newcastle tampil sebagai proksi berlabel.",
    },
    DriverSpec {
        id: "usd_idr",
        label: "USD/IDR",
        unit: "IDR",
        cadence: Cadence::Daily,
        source: Some("Yahoo IDR=X"),
        family: Family::Yahoo { symbol: "IDR=X" },
        proxy_for: None,
        note: "Kurs pasar Yahoo, bukan JISDOR.",
    },
    DriverSpec {
        id: "gold",
        label: "Harga emas",
        unit: "USD/oz",
        cadence: Cadence::Daily,
        source: Some("Yahoo GC=F"),
        family: Family::Yahoo { symbol: "GC=F" },
        proxy_for: None,
        note: "Kontrak COMEX bulan terdekat.",
    },
    DriverSpec {
        id: "cpi",
        label: "IHK Indonesia",
        unit: "indeks (2015 = 100)",
        cadence: Cadence::Monthly,
        source: Some("OECD SDMX DF_PRICES_ALL IDN"),
        family: Family::Oecd {
            path: "OECD.SDD.TPS,DSD_PRICES@DF_PRICES_ALL,1.0/IDN.M.N.CPI.IX._T.N._Z",
        },
        proxy_for: None,
        note: "Dipakai untuk penyesuaian siklus kelipatan; tertinggal sekitar satu bulan.",
    },
    DriverSpec {
        id: "brent",
        label: "Minyak Brent (proksi bunker)",
        unit: "USD/bbl",
        cadence: Cadence::Daily,
        source: Some("Yahoo BZ=F"),
        family: Family::Yahoo { symbol: "BZ=F" },
        proxy_for: Some("bunker"),
        note: "Proksi untuk harga bahan bakar kapal (VLSFO Singapura tidak punya sumber gratis).",
    },
    DriverSpec {
        id: "bwet",
        label: "BWET (proksi sewa tanker)",
        unit: "USD",
        cadence: Cadence::Daily,
        source: Some("Yahoo BWET"),
        family: Family::Yahoo { symbol: "BWET" },
        proxy_for: Some("charter_index"),
        note: "ETF futures sewa tanker global, sejak 2023-05; bukan indeks regional dan bukan OSV.",
    },
];

pub fn spec(id: &str) -> Option<&'static DriverSpec> {
    CATALOGUE.iter().find(|d| d.id == id)
}

/// Catalogue entries an adapter can fetch.
pub fn sourced() -> impl Iterator<Item = &'static DriverSpec> {
    CATALOGUE.iter().filter(|d| d.has_source())
}

/// The proxy series for a driver without a source, if the catalogue declares one.
pub fn proxy_of(driver_id: &str) -> Option<&'static DriverSpec> {
    CATALOGUE.iter().find(|d| d.proxy_for == Some(driver_id))
}

pub type Obs = (NaiveDate, f64);

/// Fetches one driver series; `since` asks for an incremental window (the adapter may
/// return more). Implemented over HTTP in production and by a mock in tests.
pub trait DriverFetcher: Send + Sync {
    fn fetch<'a>(
        &'a self,
        spec: &'a DriverSpec,
        since: Option<NaiveDate>,
    ) -> BoxFuture<'a, Result<Vec<Obs>, SourceError>>;
}

/// User agent for BIS and OECD.
pub const HTTP_UA: &str = "Mozilla/5.0";
/// FRED's edge drops `Mozilla/5.0` and honest custom agents (the request hangs to a
/// timeout) and serves curl's, so FRED alone sends this one.
pub const FRED_UA: &str = "curl/8.7.1";

/// The production fetcher: a Yahoo source (its own pacing gate) plus plain HTTP clients
/// with a longer timeout for BIS/OECD and for FRED, which times out intermittently.
pub struct HttpDrivers {
    yahoo: YahooSource,
    http: Http,
    fred_http: Http,
    bis_base: String,
    oecd_base: String,
    fred_base: String,
    retry_pause: std::time::Duration,
}

impl Default for HttpDrivers {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpDrivers {
    pub fn new() -> Self {
        Self::with_bases(YahooSource::new(None), BIS_BASE, OECD_BASE, FRED_BASE)
    }

    /// Tests point every family at a local mock server.
    pub fn with_bases(yahoo: YahooSource, bis: &str, oecd: &str, fred: &str) -> Self {
        Self {
            yahoo,
            http: Http::with_timeout(Some(HTTP_UA), std::time::Duration::from_secs(30)),
            fred_http: Http::with_timeout(Some(FRED_UA), std::time::Duration::from_secs(30)),
            bis_base: bis.trim_end_matches('/').to_string(),
            oecd_base: oecd.trim_end_matches('/').to_string(),
            fred_base: fred.to_string(),
            retry_pause: std::time::Duration::from_millis(500),
        }
    }

    /// Tests shrink the pause between retries.
    pub fn with_retry_pause(mut self, pause: std::time::Duration) -> Self {
        self.retry_pause = pause;
        self
    }

    /// Up to three tries on a timeout, a server error or a 429 (OECD rate limits), with a
    /// growing pause; other failures are final.
    async fn get_text(&self, http: &Http, url: &str) -> Result<String, SourceError> {
        let mut last = None;
        for attempt in 0..3u32 {
            match http.get(url).await {
                Ok(r) => return Ok(r.text),
                Err(e) => {
                    let again = e.kind.ends_with("Timeout")
                        || e.msg.starts_with("Server error")
                        || e.msg.contains("'429 ");
                    last = Some(SourceError(format!(
                        "{}: {}",
                        e.kind,
                        e.msg.lines().next().unwrap_or("")
                    )));
                    if !again || attempt == 2 {
                        break;
                    }
                    tokio::time::sleep(self.retry_pause * (attempt + 1)).await;
                }
            }
        }
        Err(last.unwrap_or_else(|| SourceError("no attempt".into())))
    }
}

fn start_month(since: Option<NaiveDate>, now: NaiveDate) -> String {
    let from = since.unwrap_or_else(|| ten_years_before(now));
    from.format("%Y-%m").to_string()
}

pub fn ten_years_before(d: NaiveDate) -> NaiveDate {
    d.checked_sub_months(Months::new(12 * BACKFILL_YEARS))
        .unwrap_or(d)
}

impl DriverFetcher for HttpDrivers {
    fn fetch<'a>(
        &'a self,
        spec: &'a DriverSpec,
        since: Option<NaiveDate>,
    ) -> BoxFuture<'a, Result<Vec<Obs>, SourceError>> {
        Box::pin(async move {
            let today = Utc::now().date_naive();
            match spec.family {
                Family::Yahoo { symbol } => {
                    let range = super::yahoo::range_for_gap(since.map(|s| (today - s).num_days()));
                    let data = self.yahoo.fetch(symbol, range).await?;
                    Ok(data
                        .bars
                        .iter()
                        .filter_map(|b| {
                            NaiveDate::parse_from_str(&b.date, "%Y-%m-%d")
                                .ok()
                                .map(|d| (d, b.close))
                        })
                        .collect())
                }
                Family::Bis { path } => {
                    let url = format!(
                        "{}/{path}?format=csv&detail=dataonly&startPeriod={}",
                        self.bis_base,
                        start_month(since, today)
                    );
                    parse_sdmx_csv(&self.get_text(&self.http, &url).await?)
                }
                Family::Oecd { path } => {
                    let url = format!(
                        "{}/{path}?startPeriod={}&format=csvfilewithlabels",
                        self.oecd_base,
                        start_month(since, today)
                    );
                    parse_sdmx_csv(&self.get_text(&self.http, &url).await?)
                }
                Family::Fred { id } => {
                    let from = since.unwrap_or_else(|| ten_years_before(today));
                    let url = format!("{}?id={id}&cosd={from}", self.fred_base);
                    parse_fred_csv(&self.get_text(&self.fred_http, &url).await?, id)
                }
                Family::None => Err(SourceError(format!("{}: no source (TBD)", spec.id))),
            }
        })
    }
}

/// `YYYY-MM-DD`, `YYYY-MM` (first of the month) or `YYYY` (first of January).
pub fn parse_period(s: &str) -> Option<NaiveDate> {
    let s = s.trim().trim_matches('"');
    match s.len() {
        10 => NaiveDate::parse_from_str(s, "%Y-%m-%d").ok(),
        7 => NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").ok(),
        4 => NaiveDate::parse_from_str(&format!("{s}-01-01"), "%Y-%m-%d").ok(),
        _ => None,
    }
}

fn split_csv_line(line: &str) -> Vec<String> {
    // Enough CSV for SDMX exports: quoted fields may contain commas, no escaped quotes.
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// SDMX CSV (BIS `detail=dataonly`, OECD `csvfilewithlabels`): the header names the
/// columns; `TIME_PERIOD` and `OBS_VALUE` are read wherever they sit. `NaN` and empty
/// values are skipped. An HTML or empty body is an error, never an empty series.
pub fn parse_sdmx_csv(text: &str) -> Result<Vec<Obs>, SourceError> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| SourceError("sdmx: empty body".into()))?;
    if header.trim_start().starts_with('<') {
        return Err(SourceError("sdmx: HTML instead of CSV".into()));
    }
    let cols = split_csv_line(header);
    let find = |name: &str| {
        cols.iter()
            .position(|c| c.trim().eq_ignore_ascii_case(name))
            .ok_or_else(|| SourceError(format!("sdmx: no {name} column")))
    };
    let (ti, vi) = (find("TIME_PERIOD")?, find("OBS_VALUE")?);
    let mut out = Vec::new();
    for line in lines {
        let f = split_csv_line(line);
        let (Some(t), Some(v)) = (f.get(ti), f.get(vi)) else {
            continue;
        };
        let Some(date) = parse_period(t) else {
            continue;
        };
        let Ok(value) = v.trim().parse::<f64>() else {
            continue;
        };
        if value.is_finite() {
            out.push((date, value));
        }
    }
    finish(out, "sdmx")
}

/// FRED `fredgraph.csv`: header `observation_date,<ID>`, `.` for a missing value. A missing
/// series comes back as an HTML 404 page, which is an error here too.
pub fn parse_fred_csv(text: &str, id: &str) -> Result<Vec<Obs>, SourceError> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| SourceError("fred: empty body".into()))?;
    let expected = format!("observation_date,{id}");
    if header.trim() != expected {
        return Err(SourceError(format!(
            "fred: unexpected header {:?}",
            header.chars().take(40).collect::<String>()
        )));
    }
    let mut out = Vec::new();
    for line in lines {
        let mut f = line.split(',');
        let (Some(t), Some(v)) = (f.next(), f.next()) else {
            continue;
        };
        let Some(date) = parse_period(t) else {
            continue;
        };
        if let Ok(value) = v.trim().parse::<f64>() {
            if value.is_finite() {
                out.push((date, value));
            }
        }
    }
    finish(out, "fred")
}

fn finish(mut out: Vec<Obs>, what: &str) -> Result<Vec<Obs>, SourceError> {
    if out.is_empty() {
        return Err(SourceError(format!("{what}: no observations")));
    }
    out.sort_by_key(|o| o.0);
    out.dedup_by_key(|o| o.0);
    Ok(out)
}

/// Fetches and stores one driver: a ten-year backfill when the series has no rows, else an
/// incremental window from one month before the last stored date. On failure the stored
/// series is untouched and the failed run is recorded under `drivers` / driver id.
pub async fn ingest<F: DriverFetcher + ?Sized>(
    store: &Store,
    fetcher: &F,
    spec: &DriverSpec,
    now: DateTime<Utc>,
) -> Result<usize, SourceError> {
    let started = now.to_rfc3339();
    let outcome = async {
        let last = store
            .driver_last_date(spec.id)?
            .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
        let since = last.and_then(|d| d.checked_sub_months(Months::new(1)));
        let obs = fetcher.fetch(spec, since).await?;
        let rows: Vec<(String, f64)> = obs
            .iter()
            .map(|(d, v)| (d.format("%Y-%m-%d").to_string(), *v))
            .collect();
        let source = spec.source.unwrap_or("TBD");
        store.upsert_drivers(spec.id, source, &now.to_rfc3339(), &rows)?;
        Ok(rows.len())
    }
    .await;
    record_run(
        store,
        SOURCE,
        Some(spec.id),
        &started,
        &Utc::now().to_rfc3339(),
        &outcome,
    );
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_covers_every_pack_driver_and_names_proxies_for_the_shipping_gaps() {
        for id in [
            "bi_rate",
            "sbn_10y",
            "newcastle",
            "hba",
            "usd_idr",
            "gold",
            "cpi",
        ] {
            assert!(spec(id).is_some(), "{id}");
        }
        assert!(!spec("sbn_10y").unwrap().has_source());
        assert!(!spec("hba").unwrap().has_source());
        assert_eq!(proxy_of("bunker").unwrap().id, "brent");
        assert_eq!(proxy_of("charter_index").unwrap().id, "bwet");
        assert_eq!(sourced().count(), 7);
        let mut ids: Vec<&str> = CATALOGUE.iter().map(|d| d.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOGUE.len(), "ids are unique");
    }

    #[test]
    fn bis_and_oecd_csv_are_read_by_column_name_and_dated_on_the_first() {
        let bis = "FREQ,REF_AREA,TIME_PERIOD,OBS_VALUE\nM,ID,2026-07,5.75\nM,ID,2026-08,NaN\nM,ID,2026-09,5.5\n";
        let obs = parse_sdmx_csv(bis).unwrap();
        assert_eq!(
            obs,
            vec![
                (NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(), 5.75),
                (NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(), 5.5),
            ]
        );
        let oecd =
            "STRUCTURE,STRUCTURE_ID,\"Reference area\",FREQ,TIME_PERIOD,OBS_VALUE,BASE_PER\n\
                    DATAFLOW,X,\"Indonesia, Republic of\",M,2026-08,137.7939,2015\n\
                    DATAFLOW,X,\"Indonesia, Republic of\",M,2026-09,138.2123,2015\n";
        let obs = parse_sdmx_csv(oecd).unwrap();
        assert_eq!(obs.len(), 2);
        assert_eq!(obs[1].1, 138.2123);
        assert!(parse_sdmx_csv("<html>oops</html>").is_err());
        assert!(parse_sdmx_csv("A,B\n1,2\n").is_err());
        assert!(parse_sdmx_csv("TIME_PERIOD,OBS_VALUE\n").is_err());
    }

    #[test]
    fn fred_csv_skips_dots_and_rejects_html() {
        let csv =
            "observation_date,PCOALAUUSDM\n2026-07-01,.\n2026-08-01,140.51\n2026-09-01,153.84\n";
        let obs = parse_fred_csv(csv, "PCOALAUUSDM").unwrap();
        assert_eq!(obs.len(), 2);
        assert_eq!(
            obs[0],
            (NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(), 140.51)
        );
        assert!(parse_fred_csv("<!DOCTYPE html><html>404</html>", "PCOALAUUSDM").is_err());
        assert!(parse_fred_csv(csv, "OTHER").is_err());
    }

    #[test]
    fn periods_and_backfill_window() {
        assert_eq!(parse_period("2026-09"), NaiveDate::from_ymd_opt(2026, 9, 1));
        assert_eq!(
            parse_period("2026-09-15"),
            NaiveDate::from_ymd_opt(2026, 9, 15)
        );
        assert_eq!(parse_period("2026"), NaiveDate::from_ymd_opt(2026, 1, 1));
        assert_eq!(parse_period("2026-M09"), None);
        assert_eq!(
            ten_years_before(NaiveDate::from_ymd_opt(2026, 10, 24).unwrap()),
            NaiveDate::from_ymd_opt(2016, 10, 24).unwrap()
        );
    }
}
