//! Background sync (research plan §3.1, §4.2): universe every 10 minutes, the raw XBRL
//! directory every 10 minutes and at start (a new file hash means ingest), prices and
//! driver series once per trading day after 16:30 WIB, then claim re-evaluation and the
//! inbox producers.
//! `INVESTING_RESEARCH_DISABLE_SCHEDULER` turns the loop off; `tick` can still be called by hand.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveTime, Utc, Weekday};

use super::events::react as inbox;
use super::packs::Packs;
use super::prices::drivers::{self, DriverFetcher};
use super::prices::yahoo::{self, INDEX_SYMBOL};
use super::prices::PriceSource;
use super::sources::portfolio::HeldProvider;
use super::sources::{record_run, SourceError};
use super::store::Store;
use super::universe::{self, UniverseEntry};
use super::valuation::LoadedRules;
use super::xbrl::{self as xbrl_files, FilingId};

pub const INTERVAL: Duration = Duration::from_secs(600);
/// Exchange close plus margin, in WIB (UTC+7).
const FETCH_AFTER_WIB: (u32, u32) = (16, 30);
const WIB: i64 = 7 * 3600;

pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// Most recent weekday 16:30 WIB at or before `now`.
pub fn last_slot(now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now + ChronoDuration::seconds(WIB);
    let at = NaiveTime::from_hms_opt(FETCH_AFTER_WIB.0, FETCH_AFTER_WIB.1, 0).expect("valid time");
    let mut day = local.date_naive();
    if local.time() < at {
        day = day.pred_opt().expect("date in range");
    }
    while matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
        day = day.pred_opt().expect("date in range");
    }
    day.and_time(at).and_utc() - ChronoDuration::seconds(WIB)
}

/// Prices are due when nothing succeeded since the latest slot.
pub fn due_for_prices(last_ok: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    last_ok.is_none_or(|t| t < last_slot(now))
}

#[derive(Debug, Default, PartialEq)]
pub struct TickReport {
    pub universe: usize,
    /// Filings ingested from the raw XBRL directory this tick.
    pub filings_ingested: Vec<FilingId>,
    pub prices_ok: Vec<String>,
    pub prices_failed: Vec<String>,
    /// Driver ids fetched this tick (R3.3); empty when no fetcher is attached.
    pub drivers_ok: Vec<String>,
    pub drivers_failed: Vec<String>,
    pub errors: Vec<String>,
}

pub struct Scheduler<P: PriceSource, H: HeldProvider> {
    pub store: Arc<Store>,
    pub prices: P,
    pub held: H,
    /// Raw `.xbrl` filings (`INVESTING_IDX_XBRL_DIR`), read-only.
    pub xbrl_dir: PathBuf,
    /// Original-filing JSON files (`INVESTING_PDF_FILINGS_DIR`), read-only; `None` reads none.
    pdf_dir: Option<PathBuf>,
    pub watchlist: PathBuf,
    pub clock: Clock,
    /// Pack definitions for the claim re-evaluation after ingest (R4.1); `None` skips it.
    packs: Option<Arc<Packs>>,
    /// Where `rules.toml` lives (POJK offsets for the inbox); `None` uses the embedded rules.
    rules_dir: Option<PathBuf>,
    /// External driver series (R3.3); `None` skips the driver pass (tests without a mock).
    drivers: Option<Arc<dyn DriverFetcher>>,
    /// Size and mtime per `.xbrl` file at the last pass, so unchanged files are not re-hashed.
    seen_files: Mutex<xbrl_files::FileMeta>,
}

impl<P: PriceSource + 'static, H: HeldProvider + 'static> Scheduler<P, H> {
    pub fn new(
        store: Arc<Store>,
        prices: P,
        held: H,
        xbrl_dir: PathBuf,
        watchlist: PathBuf,
        clock: Clock,
    ) -> Self {
        Self {
            store,
            prices,
            held,
            xbrl_dir,
            pdf_dir: None,
            watchlist,
            clock,
            packs: None,
            rules_dir: None,
            drivers: None,
            seen_files: Mutex::new(xbrl_files::FileMeta::default()),
        }
    }

    /// Ingests every new or changed file under `xbrl_dir` and re-derives the tickers it
    /// touched (research plan §4.2, R1.6 contract). A missing directory is an empty pass
    /// recorded as a failed `source_runs` row, so health can say so.
    pub fn ingest_new(&self) -> Vec<FilingId> {
        self.ingest().0
    }

    /// `ingest_new` plus the error text for `TickReport`. Files whose size and mtime did
    /// not move since the last pass are not re-hashed; the hash check in the store still
    /// decides what is new. Inside a multi-thread runtime the work runs via
    /// `block_in_place` so the hashing and parsing do not stall other tasks.
    fn ingest(&self) -> (Vec<FilingId>, Option<String>) {
        let now = self.now();
        if !self.xbrl_dir.is_dir() {
            let msg = format!("xbrl dir not found: {}", self.xbrl_dir.display());
            tracing::warn!("research xbrl: {msg}");
            let started = now.to_rfc3339();
            record_run(
                &self.store,
                xbrl_files::SOURCE,
                None,
                &started,
                &started,
                &Err(SourceError(msg.clone())),
            );
            return (Vec::new(), Some(msg));
        }
        let run = || {
            let mut cache = self.seen_files.lock().unwrap_or_else(|p| p.into_inner());
            xbrl_files::ingest_new_with_pdf(
                &self.store,
                &self.xbrl_dir,
                self.pdf_dir.as_deref(),
                now,
                Some(&mut cache),
            )
        };
        let multi_thread = tokio::runtime::Handle::try_current()
            .is_ok_and(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread);
        let outcome = if multi_thread {
            tokio::task::block_in_place(run)
        } else {
            run()
        };
        match outcome {
            Ok(report) => {
                if !report.ingested.is_empty() || !report.unparsed.is_empty() {
                    tracing::info!(
                        "research xbrl: {} ingested, {} unchanged, {} unreadable, {} reconciliation failure(s)",
                        report.ingested.len(),
                        report.unchanged,
                        report.unparsed.len(),
                        report.reconciliation_failures.len()
                    );
                }
                for (id, w) in &report.warnings {
                    tracing::warn!("research xbrl {id}: {w}");
                }
                for (ticker, f) in &report.reconciliation_failures {
                    tracing::warn!("research reconciliation {ticker}: {}", f.explain());
                }
                let error = (!report.unparsed.is_empty())
                    .then(|| format!("{} unreadable file(s)", report.unparsed.len()));
                (report.ingested, error)
            }
            Err(e) => {
                tracing::warn!("research xbrl ingest: {e}");
                (Vec::new(), Some(format!("xbrl ingest: {e}")))
            }
        }
    }

    /// Reads the original-filing JSON files of `dir` (beside the XBRL source) in every pass.
    pub fn with_pdf_dir(mut self, dir: PathBuf) -> Self {
        self.pdf_dir = Some(dir);
        self
    }

    /// Attaches the pack set; claims are then re-evaluated after every tick and refresh.
    pub fn with_packs(mut self, packs: Arc<Packs>) -> Self {
        self.packs = Some(packs);
        self
    }

    /// Reads the POJK offsets from `dir/rules.toml` (embedded default when missing).
    pub fn with_rules_dir(mut self, dir: PathBuf) -> Self {
        self.rules_dir = Some(dir);
        self
    }

    /// Attaches the driver fetcher; every sourced catalogue driver is then refreshed once
    /// per trading day after the price pass, in the same run so Yahoo's budget is shared.
    pub fn with_drivers(mut self, drivers: Arc<dyn DriverFetcher>) -> Self {
        self.drivers = Some(drivers);
        self
    }

    /// Fetches every due driver series; one failure does not stop the others and a
    /// failure leaves the stored series as it was.
    pub async fn refresh_drivers(&self, force: bool) -> (Vec<String>, Vec<String>) {
        let (mut ok, mut failed) = (Vec::new(), Vec::new());
        let Some(fetcher) = &self.drivers else {
            return (ok, failed);
        };
        let now = self.now();
        for spec in drivers::sourced() {
            let last = self
                .store
                .last_ok_run(drivers::SOURCE, Some(spec.id))
                .ok()
                .flatten()
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc));
            if !force && !due_for_prices(last, now) {
                continue;
            }
            match drivers::ingest(&self.store, fetcher.as_ref(), spec, now).await {
                Ok(_) => ok.push(spec.id.to_string()),
                Err(e) => {
                    tracing::warn!("research driver {}: {e}", spec.id);
                    failed.push(spec.id.to_string());
                }
            }
        }
        (ok, failed)
    }

    /// Today's date in WIB, the exchange's time zone (same rule as `Engine::today`).
    fn today(&self) -> chrono::NaiveDate {
        (self.now() + ChronoDuration::seconds(WIB)).date_naive()
    }

    /// Re-evaluates the open claims and runs the inbox producers over what ingest just
    /// stored (A4.1, A4.3). A tick runs every ten minutes, so a claim is re-evaluated well
    /// inside the hour after its data arrives.
    pub fn react(&self) {
        let Some(packs) = &self.packs else { return };
        let now = self.now();
        let rules = match &self.rules_dir {
            Some(dir) => LoadedRules::load(dir, &now.to_rfc3339()),
            None => LoadedRules::embedded(&now.to_rfc3339()),
        };
        if let Err(e) = inbox::react(
            &self.store,
            &packs.set(),
            &rules.rules.pojk_offsets,
            now,
            self.today(),
            inbox::Scope::All,
        ) {
            tracing::warn!("research inbox: {e}");
        }
    }

    /// Manual refresh (`POST /api/research/refresh`): one ticker or the whole universe.
    pub async fn refresh(&self, ticker: Option<String>) {
        let tickers: Vec<String> = match &ticker {
            Some(t) => vec![t.clone()],
            None => self
                .store
                .universe()
                .unwrap_or_default()
                .into_iter()
                .map(|e| e.ticker)
                .collect(),
        };
        let whole_universe = ticker.is_none();
        self.refresh_prices(&tickers, true).await;
        if whole_universe {
            // Drivers are not per ticker; a single-ticker refresh leaves them alone.
            self.refresh_drivers(true).await;
        }
        self.react();
    }

    fn now(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    /// Held from the portfolio plus manual adds, stored, and the scraper watchlist rewritten.
    /// When the portfolio cannot be read the stored universe stays as it was.
    pub async fn sync_universe(&self) -> Result<Vec<UniverseEntry>, SourceError> {
        let now = self.now().to_rfc3339();
        let held = self.held.held().await;
        let outcome = (|| {
            let held = held.clone()?;
            let entries = universe::sync(&self.store, &held, &now)?;
            universe::write_watchlist(&self.watchlist, &entries)
                .map_err(|e| SourceError(format!("watchlist: {e}")))?;
            Ok(entries)
        })();
        let counted = outcome.as_ref().map(Vec::len).map_err(Clone::clone);
        record_run(
            &self.store,
            "portfolio",
            None,
            &now,
            &self.now().to_rfc3339(),
            &counted,
        );
        match outcome {
            Ok(e) => Ok(e),
            Err(e) => {
                // Keep serving the stored universe.
                self.store.universe().map_err(SourceError::from)?;
                Err(e)
            }
        }
    }

    /// Fetches every due ticker plus the index; one failure does not stop the others.
    pub async fn refresh_prices(
        &self,
        tickers: &[String],
        force: bool,
    ) -> (Vec<String>, Vec<String>) {
        let now = self.now();
        let (mut ok, mut failed) = (Vec::new(), Vec::new());
        let symbols = tickers.iter().map(String::as_str).chain([INDEX_SYMBOL]);
        for t in symbols {
            let last = self
                .store
                .last_ok_run(yahoo::SOURCE, Some(t))
                .ok()
                .flatten()
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc));
            if !force && !due_for_prices(last, now) {
                continue;
            }
            match yahoo::ingest(&self.store, &self.prices, t, now).await {
                Ok(_) => ok.push(t.to_string()),
                Err(e) => {
                    tracing::warn!("research prices {t}: {e}");
                    failed.push(t.to_string());
                }
            }
        }
        (ok, failed)
    }

    pub async fn tick(&self) -> TickReport {
        let mut report = TickReport::default();
        if let Err(e) = self.sync_universe().await {
            tracing::warn!("research universe sync: {e}");
            report.errors.push(e.to_string());
        }
        let tickers: Vec<String> = self
            .store
            .universe()
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.ticker)
            .collect();
        report.universe = tickers.len();
        let (ingested, ingest_error) = self.ingest();
        report.filings_ingested = ingested;
        if let Some(e) = ingest_error {
            report.errors.push(e);
        }
        (report.prices_ok, report.prices_failed) = self.refresh_prices(&tickers, false).await;
        (report.drivers_ok, report.drivers_failed) = self.refresh_drivers(false).await;
        self.react();
        report
    }

    /// Once at startup: re-derives the tickers whose cells were computed under an older
    /// rules version, which the hash-gated ingest never revisits (research plan §4.4).
    /// Rewrites `derived` and `restatements` only; a second start finds nothing to do.
    pub async fn rederive_stale(&self) -> xbrl_files::RederiveReport {
        let store = self.store.clone();
        let now = self.now().to_rfc3339();
        let outcome =
            tokio::task::spawn_blocking(move || xbrl_files::rederive_stale(&store, &now)).await;
        match outcome {
            Ok(Ok(r)) => {
                tracing::info!(
                    "research rederive: {} ticker(s) re-derived under the current rules, {} failed, {} reconciliation failure(s)",
                    r.tickers.len(),
                    r.failed.len(),
                    r.reconciliation_failures.len()
                );
                r
            }
            Ok(Err(e)) => {
                tracing::warn!("research rederive: {e}");
                xbrl_files::RederiveReport::default()
            }
            Err(e) => {
                tracing::warn!("research rederive task: {e}");
                xbrl_files::RederiveReport::default()
            }
        }
    }

    /// Runs `tick` now and every ten minutes.
    pub fn spawn(self: Arc<Self>) {
        tokio::spawn(async move {
            tracing::info!("research scheduler started");
            let _ = self.rederive_stale().await;
            loop {
                self.tick().await;
                tokio::time::sleep(INTERVAL).await;
            }
        });
    }
}
