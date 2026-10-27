//! The store-facing half of the inbox: one pass that re-evaluates claims and runs every
//! producer over what the store holds (after an ingest, a price pull, a KPI write or a
//! manual refresh), and the Desk payload (`GET /desk`: inbox, coverage, calendar, ages).

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::{json, Value};

use super::producers::{self, FilingBrief, FlagInput, KpiDueInput, StaleInput};
use super::{item_json, DataState, Kind, FILTERS};
use crate::research::calendar;
use crate::research::claims::source as claim_source;
use crate::research::derive::Flag;
use crate::research::journal;
use crate::research::packs::payload as pack_payload;
use crate::research::packs::{Cadence, PackSet, Quarter};
use crate::research::prices::payload::level_marks;
use crate::research::store::{EventKind, Store, StoreError};
use crate::research::universe::UniverseEntry;
use crate::research::valuation::PojkOffsets;

/// Elements the hard flags read (core pack: `total_equity`, `net_income`).
const EQUITY_ELEMENT: &str = "idx-cor:Equity";
const NET_INCOME_ELEMENT: &str = "idx-cor:ProfitLoss";
/// Closes the level-crossing producer looks back over.
const CROSSING_WINDOW: usize = 10;
/// Inbox rows the Desk returns.
const INBOX_LIMIT: usize = 200;

/// Which tickers a pass covers: the whole universe (scheduler, refresh) or one ticker (a
/// KPI write).
#[derive(Clone, Copy, Debug)]
pub enum Scope<'a> {
    All,
    Ticker(&'a str),
}

impl Scope<'_> {
    fn includes(&self, ticker: &str) -> bool {
        match self {
            Self::All => true,
            Self::Ticker(t) => *t == ticker,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Events stored by this pass (a repeat of one already stored counts for nothing).
    pub added: usize,
    pub claim_changes: usize,
}

fn quarter_of(f: &crate::research::store::FilingRow) -> Quarter {
    Quarter::new(f.filing.fy, f.filing.period.quarter())
}

fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

/// One pass at a time: the scheduler tick and a KPI write would otherwise both read the same
/// claim status and store two evaluations and two events for one change.
static PASS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Re-evaluates the open claims and runs every producer for `scope`, storing the events that
/// are new. Safe to run as often as wanted: ids come from the facts, so a repeat stores
/// nothing, and a dismissed event stays dismissed.
pub fn react(
    store: &Store,
    packs: &PackSet,
    offsets: &PojkOffsets,
    now: DateTime<Utc>,
    today: NaiveDate,
    scope: Scope<'_>,
) -> Result<Report, StoreError> {
    let _one_at_a_time = PASS.lock().unwrap_or_else(|p| p.into_inner());
    let now_s = now.to_rfc3339();
    let universe: Vec<UniverseEntry> = store
        .universe()?
        .into_iter()
        .filter(|e| scope.includes(&e.ticker))
        .collect();
    let mut events = Vec::new();

    // Claims first: a status change is an event.
    let only: Option<Vec<String>> = match scope {
        Scope::All => None,
        Scope::Ticker(t) => Some(vec![t.to_string()]),
    };
    let changes = claim_source::reevaluate(store, packs, now, today, only.as_deref())?;
    // The new status is already stored, so its event is stored before anything else can
    // fail: a later error must not leave a change nobody was told about.
    let mut added = 0;
    for e in producers::claim_events(&changes, &now_s) {
        if store.add_event(&e.row())? {
            added += 1;
        }
    }
    let claim_changes = changes.len();

    let tickers: HashSet<&str> = universe.iter().map(|e| e.ticker.as_str()).collect();
    let filings: Vec<_> = store
        .filings(None)?
        .into_iter()
        .filter(|f| tickers.contains(f.filing.ticker.as_str()))
        .collect();
    let briefs: Vec<FilingBrief> = filings
        .iter()
        .map(|f| FilingBrief {
            id: f.filing.id.clone(),
            ticker: f.filing.ticker.clone(),
            quarter: quarter_of(f),
            audited: f.filing.audited,
            available_from: f.filing.available_from,
            file_hash: f.filing.file_hash.clone(),
            ingested_at: f.ingested_at.clone(),
        })
        .collect();
    events.extend(producers::filing_events(&briefs, today));

    let since = (today - Duration::days(producers::RESTATEMENT_HORIZON_DAYS)).to_string();
    let restated: Vec<_> = store
        .restatements_since(&since)?
        .into_iter()
        .filter(|r| tickers.contains(r.ticker.as_str()))
        .collect();
    events.extend(producers::restatement_events(&restated, today));

    let mut stale = Vec::new();
    let mut kpi_due = Vec::new();
    let mut flags = Vec::new();
    for entry in &universe {
        let t = entry.ticker.as_str();
        let closes: Vec<(NaiveDate, f64)> = store
            .last_closes(t, CROSSING_WINDOW)?
            .into_iter()
            .filter_map(|(d, c)| parse_day(&d).map(|d| (d, c)))
            .filter(|(d, _)| *d <= today)
            .collect();
        let levels = level_marks(store, t)?;
        let crossed = journal::crossings(&levels, &closes);
        events.extend(producers::level_events(t, &levels, &crossed, &now_s));

        let own: Vec<_> = filings.iter().filter(|f| f.filing.ticker == t).collect();
        stale.push(StaleInput {
            ticker: t.into(),
            last_close: closes.last().map(|(d, _)| *d),
            filed: own.iter().map(|f| quarter_of(f)).collect(),
        });
        if let Some(latest) = own.iter().max_by_key(|f| quarter_of(f)) {
            let applied = claim_source::applied_packs(store, packs, entry)?;
            let q = quarter_of(latest);
            let kpis = store.kpis(t)?;
            let missing: Vec<String> = applied
                .iter()
                .flat_map(|p| p.kpi_metrics())
                .filter(|m| match m.cadence.unwrap_or(Cadence::Quarterly) {
                    Cadence::Quarterly => true,
                    Cadence::Annual => q.q == 4,
                })
                .filter(|m| {
                    !kpis
                        .iter()
                        .any(|k| k.metric == m.id && k.fy == q.fy && k.q == q.q)
                })
                .map(|m| m.label.clone())
                .collect();
            kpi_due.push(KpiDueInput {
                ticker: t.into(),
                quarter: q,
                available_from: latest.filing.available_from,
                missing,
            });
        }
        let series = |element: &str, instant: bool| -> Result<Vec<(Quarter, f64)>, StoreError> {
            let mut rows: Vec<(Quarter, f64)> = store
                .derived(t, Some(element))?
                .into_iter()
                .filter(|r| r.flags.contains(&Flag::Instant) == instant)
                .filter_map(|r| Some((Quarter::new(r.fy, r.q), r.standalone?)))
                .collect();
            rows.sort_by_key(|(q, _)| *q);
            Ok(rows)
        };
        flags.push(FlagInput {
            ticker: t.into(),
            currency: own
                .iter()
                .max_by_key(|f| quarter_of(f))
                .map_or_else(|| "IDR".to_string(), |f| f.filing.currency.clone()),
            equity: series(EQUITY_ELEMENT, true)?,
            net_income: series(NET_INCOME_ELEMENT, false)?,
        });
    }
    let stale_events = producers::stale_events(&stale, offsets, today, &now_s);
    let due_events = producers::kpi_due_events(&kpi_due, today, &now_s);
    // Conditions that clear retire their event; a KPI event whose list shrank is refreshed.
    let flag_events = producers::flag_events(&flags, &now_s);
    retire_and_refresh(
        store,
        &stale_events,
        &due_events,
        &flag_events,
        &tickers,
        &now_s,
    )?;
    events.extend(stale_events);
    events.extend(due_events);
    events.extend(flag_events);

    for e in events {
        if store.add_event(&e.row())? {
            added += 1;
        }
    }
    Ok(Report {
        added,
        claim_changes,
    })
}

/// Open `stale_data` and `kpi_due` events of `tickers` whose condition no longer holds (the
/// price source caught up, the report came, the KPIs were answered) are dismissed by the
/// system; an open KPI event that is still due gets its list of missing cells refreshed, and
/// an open accounting flag gets the payload the producer writes now (it gained the filing
/// currency; the flag itself is never retired here).
fn retire_and_refresh(
    store: &Store,
    stale: &[super::NewEvent],
    due: &[super::NewEvent],
    flags: &[super::NewEvent],
    tickers: &HashSet<&str>,
    now: &str,
) -> Result<(), StoreError> {
    let active: HashSet<&str> = stale.iter().chain(due).map(|e| e.id.as_str()).collect();
    for open in store.events_list(true)? {
        let kind = Kind::parse(&open.kind);
        let in_scope = open.ticker.as_deref().is_some_and(|t| tickers.contains(t));
        if in_scope && kind == Some(Kind::AccountingFlag) {
            if let Some(fresh) = flags.iter().find(|e| e.id == open.id) {
                store.update_event_payload(&open.id, &fresh.payload.to_string())?;
            }
            continue;
        }
        if !in_scope || !matches!(kind, Some(Kind::StaleData | Kind::KpiDue)) {
            continue;
        }
        if !active.contains(open.id.as_str()) {
            store.dismiss_event(&open.id, now)?;
        } else if let Some(fresh) = due.iter().find(|e| e.id == open.id) {
            store.update_event_payload(&open.id, &fresh.payload.to_string())?;
        }
    }
    Ok(())
}

/// Every calendar item of one ticker inside the horizon: unfiled POJK windows, dividend
/// ex-dates, the owner's own dates.
pub fn calendar_for(
    store: &Store,
    offsets: &PojkOffsets,
    ticker: &str,
    today: NaiveDate,
    horizon_days: i64,
) -> Result<Vec<calendar::Item>, StoreError> {
    let filed: HashSet<(i32, u32)> = store
        .filings(Some(ticker))?
        .iter()
        .map(|f| (f.filing.fy, f.filing.period.quarter()))
        .collect();
    let dividends: Vec<(NaiveDate, f64)> = store
        .events(ticker)?
        .into_iter()
        .filter(|e| e.kind == EventKind::Dividend)
        .filter_map(|e| Some((parse_day(&e.date)?, e.amount?)))
        .collect();
    let manual: Vec<calendar::Manual> = store
        .calendar_manual(Some(ticker))?
        .into_iter()
        .filter_map(|c| {
            Some(calendar::Manual {
                date: parse_day(&c.date)?,
                kind: c.kind,
                note: c.note,
            })
        })
        .collect();
    let mut items = calendar::reporting_items(ticker, offsets, &filed, today, horizon_days);
    items.extend(calendar::dividend_items(
        ticker,
        &dividends,
        today,
        horizon_days,
    ));
    items.extend(calendar::manual_items(ticker, &manual, today, horizon_days));
    Ok(calendar::sorted(items))
}

fn role_rank(role: &str) -> u8 {
    match role {
        "held" => 0,
        "manual" => 1,
        _ => 2,
    }
}

/// `GET /desk`: `{inbox, coverage, calendar, ages}` (plan §5; design.md §4.1). Held tickers
/// first, then manual, then peers; a ticker is `Basi` when its newest close or its latest
/// completed report is overdue.
pub fn desk(
    store: &Store,
    packs: &PackSet,
    offsets: &PojkOffsets,
    now: DateTime<Utc>,
    today: NaiveDate,
    only: Option<&str>,
) -> Result<Value, StoreError> {
    let mut universe = store.universe()?;
    if let Some(t) = only {
        universe.retain(|e| e.ticker == t);
    }
    universe.sort_by(|a, b| {
        (role_rank(a.role.as_str()), &a.ticker).cmp(&(role_rank(b.role.as_str()), &b.ticker))
    });
    let seen = store.seen_all()?;
    let expected = pack_payload::last_completed_quarter(today);

    let mut open = store.events_list(true)?;
    if let Some(t) = only {
        open.retain(|e| e.ticker.as_deref() == Some(t));
    }
    let mut counts: BTreeMap<&str, usize> = FILTERS.iter().map(|(g, _)| (*g, 0)).collect();
    for e in &open {
        if let Some(k) = Kind::parse(&e.kind) {
            *counts.entry(k.group()).or_default() += 1;
        }
    }
    counts.insert("semua", open.len());
    let inbox: Vec<Value> = open
        .iter()
        .take(INBOX_LIMIT)
        .map(|e| item_json(e, today))
        .collect();

    let mut coverage = Vec::new();
    let mut calendar_items = Vec::new();
    let mut price_ages = serde_json::Map::new();
    let mut filing_ages = serde_json::Map::new();
    for entry in &universe {
        let t = entry.ticker.as_str();
        let filings = store.filings(Some(t))?;
        let latest = filings.iter().max_by_key(|f| quarter_of(f));
        let last_close = store
            .last_closes(t, 1)?
            .last()
            .and_then(|(d, _)| parse_day(d));
        let price_age = last_close.map(|d| (today - d).num_days());
        let filing_age = latest.map(|f| (today - f.filing.available_from).num_days());
        price_ages.insert(t.into(), json!(price_age));
        filing_ages.insert(t.into(), json!(filing_age));

        let pack = pack_payload::build(store, packs, entry, &[], today, &now.to_rfc3339())?;
        let claims = store.claims(Some(t))?;
        let [holding, broken, not_testable, expired] = claim_source::status_counts(&claims);

        let mut reasons = Vec::new();
        if let Some(age) = price_age.filter(|a| *a > producers::PRICE_STALE_DAYS) {
            reasons.push(format!("harga {age} hari"));
        }
        let filed: Vec<Quarter> = filings.iter().map(quarter_of).collect();
        if !filed.is_empty() && !filed.contains(&expected) {
            if let Some(w) = calendar::pojk_window(offsets, expected.fy, expected.q) {
                let over = (today - w.end).num_days();
                if over > producers::FILING_GRACE_DAYS {
                    reasons.push(format!(
                        "laporan {} belum masuk",
                        super::text_fmt::period(&expected.to_string())
                    ));
                }
            }
        }
        let base = pack["coverage"]["state"].clone();
        let state = if reasons.is_empty() {
            serde_json::from_value::<DataState>(base.clone()).ok()
        } else {
            Some(DataState::Basi)
        };
        let next = latest.and_then(|f| {
            let q = quarter_of(f);
            let (fy, qq, w) = calendar::next_report(offsets, Some((q.fy, q.q)))?;
            Some(json!({
                "label": Quarter::new(fy, qq).label(), "deadline": w.end.to_string(),
                "overdue": w.end < today,
            }))
        });
        coverage.push(json!({
            "ticker": t, "role": entry.role.as_str(), "peer_of": entry.peer_of,
            "business_note": entry.business_note,
            "pack": {
                "id": pack["pack"]["id"], "label": pack["pack"]["label"],
                "source": pack["pack"]["source"], "action": pack["pack"]["action"],
            },
            "last_seen": seen.get(t),
            "claims": {
                "holding": holding, "broken": broken, "not_testable": not_testable,
                "expired": expired,
            },
            "xbrl": pack["coverage"]["xbrl"],
            "kpi": pack["coverage"]["kpi"],
            "next_report": next,
            "data": {
                "state": state, "label": state.map(DataState::label),
                "reasons": reasons, "has_pack": pack["coverage"]["has_pack"],
            },
        }));
        calendar_items.extend(calendar_for(
            store,
            offsets,
            t,
            today,
            calendar::HORIZON_DAYS,
        )?);
    }

    let summaries = store.driver_summaries()?;
    // The footer names drivers by the catalogue's Indonesian label, never by id (`bi_rate`, `usd_idr`).
    let driver_labels: HashMap<String, String> = summaries
        .iter()
        .map(|d| {
            let label = crate::research::prices::drivers::spec(&d.series)
                .map_or_else(|| d.series.clone(), |s| s.label.to_string());
            (d.series.clone(), label)
        })
        .collect();
    let drivers: HashMap<String, i64> = summaries
        .into_iter()
        .filter_map(|d| Some((d.series, (today - parse_day(&d.last)?).num_days())))
        .collect();
    Ok(json!({
        "as_of": now.to_rfc3339(),
        "inbox": inbox,
        "inbox_total": open.len(),
        "filters": FILTERS.iter().map(|(g, l)| json!({"group": g, "label": l, "count": counts.get(g)})).collect::<Vec<_>>(),
        "coverage": coverage,
        "calendar": calendar::sorted(calendar_items),
        "ages": {"prices": price_ages, "filings": filing_ages, "drivers": drivers, "driver_labels": driver_labels},
    }))
}
