//! Source adapters (plan §6.3). Each one records a `source_runs` row per attempt and never
//! deletes stored data on failure.

pub mod emitens;
pub mod portfolio;

use super::store::{SourceRun, Store};

/// Why an adapter run failed; the text lands in `source_runs.error`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct SourceError(pub String);

impl From<super::store::StoreError> for SourceError {
    fn from(e: super::store::StoreError) -> Self {
        Self(e.to_string())
    }
}

/// Writes the outcome of one run. `now` is an RFC 3339 timestamp.
pub fn record_run(
    store: &Store,
    source: &str,
    ticker: Option<&str>,
    started_at: &str,
    now: &str,
    outcome: &Result<usize, SourceError>,
) {
    let run = SourceRun {
        source: source.into(),
        ticker: ticker.map(str::to_string),
        started_at: started_at.into(),
        finished_at: Some(now.into()),
        ok: outcome.is_ok(),
        rows: outcome.as_ref().ok().map(|n| *n as i64),
        error: outcome.as_ref().err().map(|e| e.0.clone()),
    };
    if let Err(e) = store.record_source_run(&run) {
        tracing::warn!("research: cannot record {source} run: {e}");
    }
}
