//! Held positions from the portfolio sheet (`parse_portfolio`). An empty or unreadable sheet
//! is an error, never "nothing held", so a Sheets hiccup cannot wipe the universe.

use std::future::Future;

use super::SourceError;
use crate::finance::portfolio::{parse_portfolio, PORTFOLIO_RANGE};
use crate::research::universe::{held_from_portfolio, HeldPosition};
use crate::sheets::SheetsClient;

pub const SOURCE: &str = "portfolio";

pub trait HeldProvider: Send + Sync {
    fn held(&self) -> impl Future<Output = Result<Vec<HeldPosition>, SourceError>> + Send;
}

pub struct SheetsPortfolio {
    pub sheets: SheetsClient,
    pub spreadsheet_id: String,
}

impl HeldProvider for SheetsPortfolio {
    async fn held(&self) -> Result<Vec<HeldPosition>, SourceError> {
        let rows = self
            .sheets
            .values_get(&self.spreadsheet_id, PORTFOLIO_RANGE)
            .await
            .map_err(|e| SourceError(format!("portfolio sheet: {e}")))?;
        let parsed = parse_portfolio(&rows, &self.spreadsheet_id, "");
        if parsed["available"] == false {
            return Err(SourceError("portfolio sheet is empty".into()));
        }
        Ok(held_from_portfolio(&parsed))
    }
}
