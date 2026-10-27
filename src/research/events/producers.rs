//! Pure event producers (prd.md A4.3): inputs in, `NewEvent`s out. Every id is derived from
//! the fact that caused the event, so a producer run twice (every scheduler tick, every KPI
//! write) stores each event once. Producers only look back a bounded time, so a first run
//! over a store full of old data does not flood the inbox.

use chrono::{Duration, NaiveDate};
use serde_json::json;

use super::{Kind, NewEvent};
use crate::research::calendar;
use crate::research::claims::source::StatusChange;
use crate::research::claims::Status;
use crate::research::journal::{Crossing, LevelMark};
use crate::research::packs::Quarter;
use crate::research::store::RestatementRow;
use crate::research::valuation::PojkOffsets;

/// A filing is news for this long after it arrived in the store.
pub const FILING_HORIZON_DAYS: i64 = 45;
/// A restatement is news for this long after it was detected.
pub const RESTATEMENT_HORIZON_DAYS: i64 = 45;
/// KPI cells are due this many days after the filing of their quarter became available
/// (the owner's own target, plan §7.3).
pub const KPI_GRACE_DAYS: i64 = 14;
/// Prices older than this many days are stale.
pub const PRICE_STALE_DAYS: i64 = 5;
/// A filing is overdue this many days after its POJK deadline.
pub const FILING_GRACE_DAYS: i64 = 14;

fn day_of(ts: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(ts.get(..10)?, "%Y-%m-%d").ok()
}

// ---- new filing ----

/// What the filing producer needs of one stored filing.
#[derive(Clone, Debug, PartialEq)]
pub struct FilingBrief {
    pub id: String,
    pub ticker: String,
    pub quarter: Quarter,
    pub audited: bool,
    pub available_from: NaiveDate,
    pub file_hash: String,
    /// RFC 3339: when the file reached the store.
    pub ingested_at: String,
}

/// One event for the newest filing of each ticker that reached the store within the
/// horizon. Older filings that arrived together with it (a first ingest of the whole
/// history) are not news; a re-ingested file with a new hash is.
pub fn filing_events(filings: &[FilingBrief], today: NaiveDate) -> Vec<NewEvent> {
    let mut newest: std::collections::BTreeMap<&str, &FilingBrief> = Default::default();
    for f in filings {
        let best = newest.get(f.ticker.as_str());
        if best.is_none_or(|b| f.quarter > b.quarter) {
            newest.insert(&f.ticker, f);
        }
    }
    newest
        .into_values()
        .filter(|f| {
            day_of(&f.ingested_at).is_some_and(|d| (today - d).num_days() <= FILING_HORIZON_DAYS)
        })
        .map(|f| {
            NewEvent::new(
                Kind::FilingNew,
                &format!("{}:{}", f.id, f.file_hash),
                &f.ingested_at,
                Some(&f.ticker),
                json!({
                    "filing_id": f.id, "label": f.quarter.label(), "fy": f.quarter.fy,
                    "q": f.quarter.q, "audited": f.audited,
                    "available_from": f.available_from.to_string(),
                }),
            )
        })
        .collect()
}

// ---- claim status ----

pub fn claim_events(changes: &[StatusChange], at: &str) -> Vec<NewEvent> {
    let label = |s: &str| Status::parse(s).map_or("?", Status::label);
    changes
        .iter()
        .map(|c| {
            NewEvent::new(
                Kind::ClaimStatus,
                &format!("{}:{}:{}:{at}", c.claim_id, c.from, c.to),
                at,
                Some(&c.ticker),
                json!({
                    "claim_id": c.claim_id, "statement": c.statement,
                    "from": c.from, "to": c.to,
                    "from_label": label(&c.from), "to_label": label(&c.to),
                    "value": c.value,
                }),
            )
        })
        .collect()
}

// ---- KPI due ----

/// KPI cells of a ticker's latest filed quarter that nobody has answered.
#[derive(Clone, Debug, PartialEq)]
pub struct KpiDueInput {
    pub ticker: String,
    pub quarter: Quarter,
    pub available_from: NaiveDate,
    /// Labels of the metrics with neither a value nor an explicit "unknown".
    pub missing: Vec<String>,
}

/// One event per ticker and quarter once the grace period after the filing has run out and
/// something is still missing.
pub fn kpi_due_events(inputs: &[KpiDueInput], today: NaiveDate, now: &str) -> Vec<NewEvent> {
    inputs
        .iter()
        .filter(|i| !i.missing.is_empty())
        .filter(|i| today >= i.available_from + Duration::days(KPI_GRACE_DAYS))
        .map(|i| {
            NewEvent::new(
                Kind::KpiDue,
                &format!("{}:{}", i.ticker, i.quarter),
                now,
                Some(&i.ticker),
                json!({
                    "label": i.quarter.label(), "fy": i.quarter.fy, "q": i.quarter.q,
                    "due": (i.available_from + Duration::days(KPI_GRACE_DAYS)).to_string(),
                    "missing": i.missing,
                }),
            )
        })
        .collect()
}

// ---- journal level crossed ----

/// Crossings of the owner's levels between the latest closes. `levels` carry the text of
/// the journal entry the line belongs to.
pub fn level_events(
    ticker: &str,
    levels: &[LevelMark],
    crossings: &[Crossing],
    now: &str,
) -> Vec<NewEvent> {
    crossings
        .iter()
        .map(|c| {
            let text = levels
                .iter()
                .find(|l| l.id == c.level_id)
                .map(|l| l.text.as_str())
                .unwrap_or_default();
            NewEvent::new(
                Kind::LevelCrossed,
                &format!("{}:{}:{}", c.level_id, c.date, c.direction.as_str()),
                now,
                Some(ticker),
                json!({
                    "level_id": c.level_id, "level": c.level, "text": text,
                    "direction": c.direction.as_str(), "from": c.from, "to": c.to,
                    "date": c.date.to_string(),
                }),
            )
        })
        .collect()
}

// ---- restatement ----

/// Restatements detected within the horizon.
pub fn restatement_events(rows: &[RestatementRow], today: NaiveDate) -> Vec<NewEvent> {
    rows.iter()
        .filter(|r| {
            day_of(&r.detected_at)
                .is_some_and(|d| (today - d).num_days() <= RESTATEMENT_HORIZON_DAYS)
        })
        .map(|r| {
            let delta =
                (r.original != 0.0).then(|| (r.restated - r.original) / r.original.abs() * 100.0);
            NewEvent::new(
                Kind::Restatement,
                &format!(
                    "{}:{}:{}:{}:{}:{}",
                    r.ticker,
                    r.element,
                    r.start.as_deref().unwrap_or(""),
                    r.end,
                    r.dims,
                    r.restated_filing
                ),
                &r.detected_at,
                Some(&r.ticker),
                json!({
                    "element": r.element.rsplit(':').next().unwrap_or(&r.element),
                    "element_full": r.element, "period_end": r.end, "dims": r.dims,
                    "original": r.original, "original_filing": r.original_filing,
                    "restated": r.restated, "restated_filing": r.restated_filing,
                    "delta_pct": delta,
                }),
            )
        })
        .collect()
}

// ---- stale data ----

#[derive(Clone, Debug, PartialEq)]
pub struct StaleInput {
    pub ticker: String,
    /// Date of the newest stored close; `None` when nothing is stored (no event: the
    /// coverage board says so).
    pub last_close: Option<NaiveDate>,
    /// Quarters this ticker has a filing for.
    pub filed: Vec<Quarter>,
}

/// Prices older than `PRICE_STALE_DAYS`, and the latest completed quarter's report once its
/// POJK deadline plus `FILING_GRACE_DAYS` has passed for a ticker that files at all.
pub fn stale_events(
    inputs: &[StaleInput],
    offsets: &PojkOffsets,
    today: NaiveDate,
    now: &str,
) -> Vec<NewEvent> {
    let mut out = Vec::new();
    let expected = crate::research::packs::payload::last_completed_quarter(today);
    for i in inputs {
        if let Some(last) = i.last_close {
            let age = (today - last).num_days();
            if age > PRICE_STALE_DAYS {
                out.push(NewEvent::new(
                    Kind::StaleData,
                    &format!("{}:prices:{last}", i.ticker),
                    now,
                    Some(&i.ticker),
                    json!({"source": "prices", "last": last.to_string(), "age_days": age}),
                ));
            }
        }
        let tracked = !i.filed.is_empty() && !i.filed.contains(&expected);
        if let (true, Some(w)) = (
            tracked,
            calendar::pojk_window(offsets, expected.fy, expected.q),
        ) {
            let age = (today - w.end).num_days();
            if age > FILING_GRACE_DAYS {
                out.push(NewEvent::new(
                    Kind::StaleData,
                    &format!("{}:filing:{expected}", i.ticker),
                    now,
                    Some(&i.ticker),
                    json!({
                        "source": "filing", "quarter": expected.label(),
                        "deadline": w.end.to_string(), "age_days": age,
                    }),
                ));
            }
        }
    }
    out
}

// ---- hard accounting flags ----

/// Equity (instant) and standalone net income per quarter, each ascending.
#[derive(Clone, Debug, PartialEq)]
pub struct FlagInput {
    pub ticker: String,
    /// Reporting currency of the latest filing; the amounts in the event text are in it.
    pub currency: String,
    pub equity: Vec<(Quarter, f64)>,
    pub net_income: Vec<(Quarter, f64)>,
}

/// Negative equity at the latest quarter that has an equity figure; net loss in the two
/// latest consecutive quarters. Each is a fact with its number, never a verdict. A
/// going-concern note is a text block the fact store skips, so it cannot be flagged here.
pub fn flag_events(inputs: &[FlagInput], now: &str) -> Vec<NewEvent> {
    let mut out = Vec::new();
    for i in inputs {
        if let Some((q, v)) = i.equity.last().filter(|(_, v)| *v < 0.0) {
            out.push(NewEvent::new(
                Kind::AccountingFlag,
                &format!("{}:negative_equity:{q}", i.ticker),
                now,
                Some(&i.ticker),
                json!({"flag": "negative_equity", "quarter": q.label(), "value": v, "currency": i.currency}),
            ));
        }
        if let [.., (q0, v0), (q1, v1)] = i.net_income.as_slice() {
            if *v0 < 0.0 && *v1 < 0.0 && q1.back(1) == *q0 {
                out.push(NewEvent::new(
                    Kind::AccountingFlag,
                    &format!("{}:two_loss_quarters:{q1}", i.ticker),
                    now,
                    Some(&i.ticker),
                    json!({
                        "flag": "two_loss_quarters", "quarter": q1.label(), "currency": i.currency,
                        "quarters": [
                            {"quarter": q0.label(), "value": v0},
                            {"quarter": q1.label(), "value": v1},
                        ],
                    }),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::research::events::{text, FILTERS};
    use crate::research::journal::{crossings, Direction};

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn q(fy: i32, q: u32) -> Quarter {
        Quarter::new(fy, q)
    }

    fn brief(id: &str, ticker: &str, quarter: Quarter, ingested: &str) -> FilingBrief {
        FilingBrief {
            id: id.into(),
            ticker: ticker.into(),
            quarter,
            audited: false,
            available_from: d("2026-08-29"),
            file_hash: "abc".into(),
            ingested_at: ingested.into(),
        }
    }

    #[test]
    fn a_new_filing_is_reported_once_for_the_newest_quarter_of_each_ticker() {
        let today = d("2026-10-24");
        let filings = [
            brief(
                "AAAA_2026_Q1",
                "AAAA",
                q(2026, 1),
                "2026-10-20T01:00:00+00:00",
            ),
            brief(
                "AAAA_2026_Q2",
                "AAAA",
                q(2026, 2),
                "2026-10-20T01:00:00+00:00",
            ),
            brief(
                "BBBB_2026_Q2",
                "BBBB",
                q(2026, 2),
                "2026-10-23T01:00:00+00:00",
            ),
            // Ingested long ago: not news.
            brief(
                "CCCC_2026_Q2",
                "CCCC",
                q(2026, 2),
                "2026-07-01T01:00:00+00:00",
            ),
        ];
        let events = filing_events(&filings, today);
        let tickers: Vec<&str> = events
            .iter()
            .map(|e| e.ticker.as_deref().unwrap())
            .collect();
        assert_eq!(tickers, ["AAAA", "BBBB"]);
        assert!(events.iter().all(|e| e.kind == Kind::FilingNew));
        assert_eq!(events[0].payload["label"], "2026Q2");
        assert_eq!(
            text(Kind::FilingNew, &events[0].payload),
            "Laporan Q2 26 masuk (belum diaudit)"
        );
        // Deterministic ids; a changed file hash is a new event.
        assert_eq!(events, filing_events(&filings, today));
        let mut amended = filings.clone();
        amended[1].file_hash = "def".into();
        assert_ne!(filing_events(&amended, today)[0].id, events[0].id);
        assert_eq!(events[0].at, "2026-10-20T01:00:00+00:00");
    }

    #[test]
    fn a_claim_status_change_names_both_statuses_and_the_value() {
        let change = StatusChange {
            claim_id: "c-1".into(),
            ticker: "COAL".into(),
            statement: "Produksi minimal lima".into(),
            from: "not_testable".into(),
            to: "holding".into(),
            value: Some(6.0),
        };
        let events = claim_events(&[change], "2026-10-24T09:00:00+00:00");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, Kind::ClaimStatus);
        assert_eq!(
            text(Kind::ClaimStatus, &events[0].payload),
            "Klaim \"Produksi minimal lima\": Belum teruji → Berlaku (nilai 6)"
        );
        assert_eq!(Kind::ClaimStatus.section(&events[0].payload), "claims");
    }

    #[test]
    fn kpi_due_waits_for_the_grace_period_and_needs_something_missing() {
        let input = KpiDueInput {
            ticker: "COAL".into(),
            quarter: q(2026, 2),
            available_from: d("2026-08-29"),
            missing: vec![
                "Produksi".into(),
                "Penjualan".into(),
                "Stripping ratio".into(),
                "Biaya".into(),
            ],
        };
        let now = "2026-10-24T09:00:00+00:00";
        assert!(kpi_due_events(std::slice::from_ref(&input), d("2026-09-11"), now).is_empty());
        let events = kpi_due_events(std::slice::from_ref(&input), d("2026-09-12"), now);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["due"], "2026-09-12");
        assert_eq!(
            text(Kind::KpiDue, &events[0].payload),
            "KPI Q2 26 belum diisi: Produksi, Penjualan, Stripping ratio dan 1 lagi"
        );
        assert_eq!(Kind::KpiDue.section(&Value::Null), "pack");
        let answered = KpiDueInput {
            missing: Vec::new(),
            ..input
        };
        assert!(kpi_due_events(&[answered], d("2026-10-24"), now).is_empty());
    }

    #[test]
    fn a_crossed_level_becomes_an_event_with_direction_and_text() {
        let levels = [LevelMark {
            id: "j-1".into(),
            level: 100.0,
            entered: 100.0,
            at: d("2026-01-01"),
            text: "Harga incaran".into(),
        }];
        let found = crossings(
            &levels,
            &[(d("2026-02-02"), 99.0), (d("2026-02-03"), 101.5)],
        );
        let events = level_events("COAL", &levels, &found, "2026-02-03T10:00:00+00:00");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["direction"], Direction::Up.as_str());
        assert_eq!(
            text(Kind::LevelCrossed, &events[0].payload),
            "Harga 101,5 naik melewati level 100 (\"Harga incaran\")"
        );
        assert_eq!(Kind::LevelCrossed.section(&Value::Null), "price");
        // The same crossing on a later pass has the same id.
        let again = level_events("COAL", &levels, &found, "2026-02-03T10:10:00+00:00");
        assert_eq!(again[0].id, events[0].id);
    }

    fn restatement(detected: &str) -> RestatementRow {
        RestatementRow {
            ticker: "COAL".into(),
            element: "idx-cor:SalesAndRevenue".into(),
            start: Some("2026-01-01".into()),
            end: "2026-06-30".into(),
            dims: String::new(),
            original: 100.0,
            original_filing: "COAL_2026_Q2".into(),
            restated: 110.0,
            restated_filing: "COAL_2026_FY".into(),
            detected_at: detected.into(),
        }
    }

    #[test]
    fn a_recent_restatement_is_reported_with_both_values() {
        let today = d("2026-10-24");
        let events = restatement_events(
            &[
                restatement("2026-10-20T01:00:00+00:00"),
                restatement("2026-01-20T01:00:00+00:00"),
            ],
            today,
        );
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].payload["delta_pct"].as_f64().unwrap().round(),
            10.0
        );
        assert_eq!(
            text(Kind::Restatement, &events[0].payload),
            "SalesAndRevenue 2026-06-30 disajikan kembali: 100 → 110"
        );
        assert_eq!(Kind::Restatement.group(), "data");
    }

    fn offsets() -> PojkOffsets {
        PojkOffsets {
            q1: 30,
            q2: 60,
            q3: 30,
            fy: 90,
        }
    }

    #[test]
    fn stale_prices_and_an_overdue_report_are_reported_once_per_episode() {
        let now = "2026-11-20T09:00:00+00:00";
        let input = StaleInput {
            ticker: "COAL".into(),
            last_close: Some(d("2026-10-23")),
            filed: vec![q(2026, 1), q(2026, 2)],
        };
        let events = stale_events(
            std::slice::from_ref(&input),
            &offsets(),
            d("2026-11-20"),
            now,
        );
        let texts: Vec<String> = events
            .iter()
            .map(|e| text(Kind::StaleData, &e.payload))
            .collect();
        assert_eq!(
            texts,
            [
                "Harga terakhir 2026-10-23, 28 hari lalu",
                "Laporan Q3 26 belum masuk; batas 2026-10-30, lewat 21 hari"
            ]
        );
        assert_eq!(Kind::StaleData.section(&events[0].payload), "price");
        assert_eq!(Kind::StaleData.section(&events[1].payload), "sources");
        // Inside the thresholds nothing is reported.
        assert!(stale_events(
            std::slice::from_ref(&input),
            &offsets(),
            d("2026-10-28"),
            now
        )
        .is_empty());
        // 14 days of grace after the deadline: due on the 15th day, not before.
        let at_grace = stale_events(
            std::slice::from_ref(&input),
            &offsets(),
            d("2026-11-13"),
            now,
        );
        assert!(at_grace.iter().all(|e| e.payload["source"] != "filing"));
        let past = stale_events(
            std::slice::from_ref(&input),
            &offsets(),
            d("2026-11-14"),
            now,
        );
        assert!(past.iter().any(|e| e.payload["source"] == "filing"));
        // A ticker with no filings at all, or with no prices, is not "stale".
        let bare = StaleInput {
            ticker: "NEWB".into(),
            last_close: None,
            filed: Vec::new(),
        };
        assert!(stale_events(&[bare], &offsets(), d("2026-11-20"), now).is_empty());
        // The id belongs to the episode: the same last close is the same event tomorrow.
        let again = stale_events(
            &[input],
            &offsets(),
            d("2026-11-21"),
            "2026-11-21T09:00:00+00:00",
        );
        assert_eq!(again[0].id, events[0].id);
    }

    #[test]
    fn hard_flags_state_the_number() {
        let now = "2026-10-24T09:00:00+00:00";
        let input = FlagInput {
            ticker: "BUST".into(),
            currency: "IDR".into(),
            equity: vec![(q(2026, 1), 50.0), (q(2026, 2), -120.0)],
            net_income: vec![(q(2025, 4), 10.0), (q(2026, 1), -5.0), (q(2026, 2), -7.5)],
        };
        let events = flag_events(&[input], now);
        let texts: Vec<String> = events
            .iter()
            .map(|e| text(Kind::AccountingFlag, &e.payload))
            .collect();
        assert_eq!(
            texts,
            [
                "Ekuitas negatif pada Q2 26: −120 rupiah",
                "Rugi bersih dua kuartal berturut-turut: Q1 26 −5 rupiah, Q2 26 −7,5 rupiah"
            ]
        );
        assert_eq!(Kind::AccountingFlag.group(), "bendera");
        // One loss, a gap between the two loss quarters, or positive equity: no flag.
        let clean = FlagInput {
            ticker: "OKAY".into(),
            currency: "IDR".into(),
            equity: vec![(q(2026, 2), 10.0)],
            net_income: vec![(q(2026, 1), -5.0), (q(2026, 2), 3.0)],
        };
        let gap = FlagInput {
            ticker: "GAPP".into(),
            currency: "IDR".into(),
            equity: Vec::new(),
            net_income: vec![(q(2025, 4), -5.0), (q(2026, 2), -3.0)],
        };
        assert!(flag_events(&[clean, gap], now).is_empty());
        // Equity is judged at the latest figure only: it recovered, so nothing.
        let recovered = FlagInput {
            ticker: "BACK".into(),
            currency: "IDR".into(),
            equity: vec![(q(2026, 1), -5.0), (q(2026, 2), 5.0)],
            net_income: Vec::new(),
        };
        assert!(flag_events(&[recovered], now).is_empty());
    }

    #[test]
    fn flag_amounts_are_in_the_filing_currency() {
        let now = "2026-10-24T09:00:00+00:00";
        let usd = FlagInput {
            ticker: "LEAD".into(),
            currency: "USD".into(),
            equity: vec![(q(2026, 2), -2_500_000.0)],
            net_income: vec![(q(2026, 1), -1_190_000.0), (q(2026, 2), -1_700_000.0)],
        };
        let texts: Vec<String> = flag_events(&[usd], now)
            .iter()
            .map(|e| text(Kind::AccountingFlag, &e.payload))
            .collect();
        assert_eq!(
            texts,
            [
                "Ekuitas negatif pada Q2 26: \u{2212}US$2,5 juta",
                "Rugi bersih dua kuartal berturut-turut: Q1 26 \u{2212}US$1,19 juta, Q2 26 \u{2212}US$1,7 juta"
            ]
        );
        // A stored payload from before the currency field is read as rupiah, as it was written.
        let old = json!({"flag": "negative_equity", "quarter": "2026Q2", "value": -120.0});
        assert_eq!(
            text(Kind::AccountingFlag, &old),
            "Ekuitas negatif pada Q2 26: \u{2212}120 rupiah"
        );
    }

    #[test]
    fn kinds_have_groups_filters_and_no_verdict_words() {
        for k in Kind::ALL {
            assert_eq!(Kind::parse(k.as_str()), Some(k));
            assert!(FILTERS.iter().any(|(g, _)| *g == k.group()));
        }
        assert_eq!(Kind::parse("nope"), None);
        let payloads = [
            (Kind::FilingNew, json!({"label": "2026Q2", "audited": true})),
            (
                Kind::StaleData,
                json!({"source": "prices", "last": "2026-10-01", "age_days": 23}),
            ),
        ];
        for (k, p) in payloads {
            let t = text(k, &p).to_lowercase();
            for w in [
                "beli", "jual", "murah", "mahal", "layak", "wajar", "keluar", "coret",
            ] {
                assert!(!t.contains(w), "{t}");
            }
        }
    }
}
