//! Price and driver series (research plan §3.1 `prices/`, D12): the `PriceSource` seam, the
//! Yahoo chart adapter behind it, and the external driver adapters. Every adapter records a
//! `source_runs` row per attempt and never deletes stored data on failure.

pub mod drivers;
pub mod payload;
pub mod yahoo;

use std::future::Future;

use super::sources::SourceError;
use super::store::{CorpEventRow, EventKind, PriceBar};

/// One symbol's daily bars plus its dividend and split events, as a source returns them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChartData {
    pub bars: Vec<PriceBar>,
    /// `(date, amount)`.
    pub dividends: Vec<(String, f64)>,
    /// `(date, numerator, denominator)`: shares are multiplied by `num / den`.
    pub splits: Vec<(String, f64, f64)>,
}

impl ChartData {
    pub fn events(&self, ticker: &str) -> Vec<CorpEventRow> {
        let divs = self.dividends.iter().map(|(date, amount)| CorpEventRow {
            ticker: ticker.into(),
            date: date.clone(),
            kind: EventKind::Dividend,
            amount: Some(*amount),
            ratio_num: None,
            ratio_den: None,
        });
        let splits = self.splits.iter().map(|(date, n, d)| CorpEventRow {
            ticker: ticker.into(),
            date: date.clone(),
            kind: EventKind::Split,
            amount: None,
            ratio_num: Some(*n),
            ratio_den: Some(*d),
        });
        divs.chain(splits).collect()
    }
}

/// Seam for price providers (D12): `symbol` is the provider's symbol (`BBCA.JK`, `^JKSE`,
/// `IDR=X`), `range` a provider range such as `10y` or `3mo`. Yahoo is the only
/// implementation; tests substitute a local mock server behind the same type.
pub trait PriceSource: Send + Sync {
    fn fetch(
        &self,
        symbol: &str,
        range: &str,
    ) -> impl Future<Output = Result<ChartData, SourceError>> + Send;
}
