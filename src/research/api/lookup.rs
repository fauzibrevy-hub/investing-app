//! `GET /api/research/lookup?q=&limit=` (design.md §4.5, prd.md A6.2): the add wizard's ticker
//! check and peer search. The exchange list is `emitens.csv` plus every ticker that has a
//! filing in the store. A ticker's industry code is the one on its latest filing and the
//! proposed pack is the pack that claims it (A2.5); neither is guessed when no filing exists.
//! When the exchange list cannot be read, `list_available` is false and `listed` is null:
//! the caller sees that the ticker could not be checked, never a made-up answer.

use std::collections::{BTreeMap, BTreeSet};

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{store_error, ApiError, ApiState};
use crate::research::packs::PackSet;
use crate::research::universe;

const DEFAULT_LIMIT: usize = 8;
const MAX_LIMIT: usize = 25;

#[derive(Deserialize)]
pub struct LookupQuery {
    q: Option<String>,
    limit: Option<usize>,
}

/// Every ticker the app may research: the exchange list, then filings on disk.
pub fn listed_tickers(st: &ApiState) -> Result<(BTreeMap<String, String>, bool), ApiError> {
    let engine = st.engine()?;
    let names = engine.names();
    let list_available = !names.is_empty();
    let mut all: BTreeMap<String, String> =
        names.iter().map(|(t, n)| (t.clone(), n.clone())).collect();
    for f in engine.store.filings(None).map_err(store_error)? {
        all.entry(f.filing.ticker).or_default();
    }
    Ok((all, list_available))
}

fn item(
    ticker: &str,
    name: &str,
    listed: Option<bool>,
    codes: &std::collections::HashMap<String, String>,
    packs: &PackSet,
    universe: &[universe::UniverseEntry],
) -> Value {
    let code = codes.get(ticker);
    let pack = code.and_then(|c| packs.for_industry_code(c));
    let member = universe.iter().find(|e| e.ticker == ticker);
    json!({
        "ticker": ticker,
        "name": (!name.is_empty()).then_some(name),
        "listed": listed,
        "industry_code": code,
        "pack": pack.map(|p| json!({"id": p.id, "label": p.label})),
        "in_universe": member.is_some(),
        "role": member.map(|e| e.role.as_str()),
    })
}

pub async fn lookup(
    State(st): State<ApiState>,
    Query(q): Query<LookupQuery>,
) -> Result<Json<Value>, ApiError> {
    let engine = st.engine()?;
    let needle = q.q.as_deref().unwrap_or_default().trim().to_uppercase();
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let (all, list_available) = listed_tickers(&st)?;
    let codes = engine.store.latest_industry_codes().map_err(store_error)?;
    let members = engine.store.universe().map_err(store_error)?;
    let packs = st.pack_set();
    let one = |t: &str, listed: Option<bool>| {
        let name = all.get(t).map_or("", String::as_str);
        item(t, name, listed, &codes, &packs, &members)
    };

    let exact = if universe::is_valid_ticker(&needle) {
        match (all.contains_key(&needle), list_available) {
            (true, _) => Some(one(&needle, Some(true))),
            (false, false) => Some(one(&needle, None)),
            (false, true) => None,
        }
    } else {
        None
    };

    let mut ranked: BTreeSet<(u8, String)> = BTreeSet::new();
    if !needle.is_empty() {
        for (t, n) in &all {
            let rank = if *t == needle {
                0
            } else if t.starts_with(&needle) {
                1
            } else if n.to_uppercase().contains(&needle) {
                2
            } else {
                continue;
            };
            ranked.insert((rank, t.clone()));
        }
    }
    let items: Vec<Value> = ranked
        .iter()
        .take(limit)
        .map(|(_, t)| one(t, Some(true)))
        .collect();
    Ok(Json(json!({
        "as_of": engine.now().to_rfc3339(),
        "query": needle,
        "list_available": list_available,
        "exact": exact,
        "items": items,
    })))
}
