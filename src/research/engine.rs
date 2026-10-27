//! The context the API and the scheduler share: the store, the clock (injected so tests
//! pin the day) and the company names read from `emitens.csv`. The rules engine that
//! evaluated a ticker into eligibility, zone and snapshots went away in R5 (plan §1).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use chrono::{DateTime, FixedOffset, NaiveDate, Utc};

use super::scheduler::Clock;
use super::sources::emitens;
use super::store::Store;

pub type NameMap = Arc<HashMap<String, String>>;

pub struct Engine {
    pub store: Arc<Store>,
    pub clock: Clock,
    emitens_csv: PathBuf,
    names: Mutex<(Option<SystemTime>, NameMap)>,
}

impl Engine {
    pub fn new(store: Arc<Store>, emitens_csv: PathBuf, clock: Clock) -> Self {
        Self {
            store,
            clock,
            emitens_csv,
            names: Mutex::new((None, Arc::new(HashMap::new()))),
        }
    }

    pub fn now(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    /// Today's calendar date in WIB, the exchange's time zone.
    pub fn today(&self) -> NaiveDate {
        let wib = FixedOffset::east_opt(7 * 3600).expect("valid offset");
        self.now().with_timezone(&wib).date_naive()
    }

    /// The company name from `emitens.csv`, `None` when the file or the row is missing or empty.
    /// The file is re-read when its modification time changes.
    pub fn entity_name(&self, ticker: &str) -> Option<String> {
        self.names().get(ticker).cloned()
    }

    /// Every ticker and name in `emitens.csv` (the exchange list the add wizard validates
    /// against); empty when the file is missing or unreadable.
    pub fn names(&self) -> NameMap {
        let mtime = std::fs::metadata(&self.emitens_csv)
            .and_then(|m| m.modified())
            .ok();
        let mut guard = self.names.lock().unwrap_or_else(|p| p.into_inner());
        if guard.0 != mtime || mtime.is_none() {
            match emitens::load(&self.emitens_csv) {
                Ok(map) => {
                    if map.is_empty() {
                        tracing::warn!(
                            "research: {} holds no company names",
                            self.emitens_csv.display()
                        );
                    }
                    *guard = (mtime, Arc::new(map));
                }
                // A failed read is not cached: the next call tries again.
                Err(e) => {
                    if mtime.is_some() {
                        tracing::warn!("research: cannot read {}: {e}", self.emitens_csv.display());
                    }
                    *guard = (None, Arc::new(HashMap::new()));
                }
            }
        }
        guard.1.clone()
    }
}
