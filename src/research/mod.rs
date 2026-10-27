//! Riset emiten (research canon v1, `docs/research/plan.md` §3.1): a personal research
//! universe with its own SQLite store, raw XBRL facts, derived quarters, price sources and
//! `/api/research/*`.
//!
//! This module is self-contained: it reads the portfolio and the scraper's output, and
//! nothing from the other feature modules. `tests/research_independence.rs` enforces that.

pub mod api;
pub mod calendar;
pub mod calibration;
pub mod claims;
pub mod derive;
pub mod documents;
pub mod engine;
pub mod events;
pub mod extract;
pub mod health;
pub mod journal;
pub mod kpi;
pub mod packs;
pub mod prices;
pub mod scheduler;
pub mod shares;
pub mod sources;
pub mod store;
pub mod structure;
pub mod universe;
pub mod valuation;
pub mod xbrl;

use std::sync::Arc;

use axum::Router;
use chrono::Utc;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::sheets::SheetsClient;
use api::{ApiState, Options};
use engine::Engine;
use packs::Packs;
use prices::drivers::HttpDrivers;
use prices::yahoo::YahooSource;
use scheduler::Scheduler;
use sources::portfolio::SheetsPortfolio;
use store::Store;

/// Shared by the research routes; `None` when the store could not be opened.
pub type Shared = Option<Arc<Store>>;

/// Routes over `store` with default options (journal in the temp dir, no macro, no refresh).
pub fn router_with(store: Shared) -> Router {
    let idem = std::env::temp_dir().join(format!("research-idem-{}.jsonl", uuid::Uuid::new_v4()));
    let engine = store.map(|s| {
        Arc::new(Engine::new(
            s,
            std::path::PathBuf::new(),
            Arc::new(Utc::now),
        ))
    });
    api::router(ApiState::new(
        engine,
        Options {
            idem_path: idem,
            watchlist: None,
            refresh_tx: None,
            packs: Arc::new(Packs::embedded()),
            rules_dir: None,
            documents_dir: std::env::temp_dir()
                .join(format!("research-documents-{}", uuid::Uuid::new_v4())),
        },
    ))
}

/// Opens the store, starts the scheduler unless disabled, and returns the routes.
pub fn router(cfg: Arc<Config>) -> Router {
    let store = match Store::open(&cfg.research_db_path) {
        Ok(s) => Some(Arc::new(s)),
        Err(e) => {
            tracing::error!("research store {}: {e}", cfg.research_db_path.display());
            None
        }
    };
    let engine = store.as_ref().map(|s| {
        Arc::new(Engine::new(
            s.clone(),
            cfg.idx_emitens_csv.clone(),
            Arc::new(Utc::now),
        ))
    });
    let packs = Arc::new(Packs::load(
        &cfg.research_packs_dir,
        &Utc::now().to_rfc3339(),
    ));
    {
        let s = packs.status();
        tracing::info!(
            "research packs: {} loaded from {} ({} invalid)",
            s.set.len(),
            s.source,
            s.invalid.len()
        );
        for (file, err) in &s.invalid {
            tracing::warn!("research pack {file}: {err}");
        }
    }
    let mut refresh_tx = None;
    if let (Some(store), false) = (&store, cfg.research_disable_scheduler) {
        let sched = Arc::new(
            Scheduler::new(
                store.clone(),
                YahooSource::new(None),
                SheetsPortfolio {
                    sheets: SheetsClient::new(cfg.google_credentials_path.clone()),
                    spreadsheet_id: cfg.portfolio_spreadsheet_id.clone(),
                },
                cfg.idx_xbrl_dir.clone(),
                cfg.research_scraper_watchlist.clone(),
                Arc::new(Utc::now),
            )
            .with_pdf_dir(cfg.research_pdf_filings_dir.clone())
            .with_packs(packs.clone())
            .with_rules_dir(cfg.research_packs_dir.clone())
            .with_drivers(Arc::new(HttpDrivers::new())),
        );
        let (tx, mut rx) = mpsc::unbounded_channel::<Option<String>>();
        refresh_tx = Some(tx);
        let worker = sched.clone();
        tokio::spawn(async move {
            while let Some(ticker) = rx.recv().await {
                worker.refresh(ticker).await;
            }
        });
        sched.spawn();
    }
    let dir = cfg
        .research_db_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    api::router(ApiState::new(
        engine,
        Options {
            idem_path: dir.join("research-idempotency.jsonl"),
            watchlist: Some(cfg.research_scraper_watchlist.clone()),
            refresh_tx,
            packs,
            rules_dir: Some(cfg.research_packs_dir.clone()),
            documents_dir: cfg.research_documents_dir.clone(),
        },
    ))
}
