//! Calendar (research plan §3.1 `calendar.rs`, R4.4; design.md §4.1 block 4): POJK reporting
//! windows, dividend ex-dates from `corp_events`, and entries the owner writes by hand.
//!
//! Reporting windows follow the rules file's `pojk_offsets` (days after the period end by
//! which a filing is due: Q1 30, Q2 60, Q3 30, FY 90). A window the ticker has already
//! filed is not listed. Dividend dates are what the price source reported; a date one year
//! after a past ex-date is shown only as a labelled projection (`projected`), never as a
//! fact. Pure: dates and rows in, items out.

use std::collections::HashSet;

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;

use super::valuation::PojkOffsets;

/// Days around today an unfiled window or a dividend date stays listed after it passed.
pub const LOOKBACK_DAYS: i64 = 30;
/// Default horizon of the Desk calendar.
pub const HORIZON_DAYS: i64 = 90;
/// A projected dividend date is dropped when a reported one lies this close to it.
const PROJECTION_TOLERANCE_DAYS: i64 = 45;

/// Kinds of owner-entered dates and their Indonesian labels.
pub const MANUAL_KINDS: [(&str, &str); 5] = [
    ("rups", "RUPS"),
    ("dividen", "Dividen"),
    ("laporan", "Laporan"),
    ("paparan", "Paparan"),
    ("lainnya", "Lainnya"),
];

pub fn manual_label(kind: &str) -> Option<&'static str> {
    MANUAL_KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, l)| *l)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Window {
    /// First day after the period end.
    pub start: NaiveDate,
    /// The POJK deadline.
    pub end: NaiveDate,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Item {
    pub date: NaiveDate,
    pub ticker: String,
    /// `pojk`, `dividend` or the manual kind.
    pub kind: String,
    pub label: String,
    /// `pojk`, `corp_events` or `manual`.
    pub source: &'static str,
    pub window: Option<Window>,
    /// A date derived from last year's, not reported by anyone.
    pub projected: bool,
    /// A reporting deadline already past without a filing.
    pub overdue: bool,
    pub amount: Option<f64>,
    pub note: Option<String>,
}

pub fn period_end(fy: i32, q: u32) -> Option<NaiveDate> {
    match q {
        1 => NaiveDate::from_ymd_opt(fy, 3, 31),
        2 => NaiveDate::from_ymd_opt(fy, 6, 30),
        3 => NaiveDate::from_ymd_opt(fy, 9, 30),
        4 => NaiveDate::from_ymd_opt(fy, 12, 31),
        _ => None,
    }
}

pub fn offset_days(offsets: &PojkOffsets, q: u32) -> i64 {
    i64::from(match q {
        1 => offsets.q1,
        2 => offsets.q2,
        3 => offsets.q3,
        _ => offsets.fy,
    })
}

/// The reporting window of quarter `q` of `fy` (`q = 4` is the full-year report).
pub fn pojk_window(offsets: &PojkOffsets, fy: i32, q: u32) -> Option<Window> {
    let end = period_end(fy, q)?;
    Some(Window {
        start: end + Duration::days(1),
        end: end + Duration::days(offset_days(offsets, q)),
    })
}

fn quarter_label(fy: i32, q: u32) -> String {
    format!("{fy}Q{q}")
}

/// Unfiled reporting windows whose deadline lies between `LOOKBACK_DAYS` ago and
/// `horizon_days` ahead. `filed` holds the `(fy, q)` the ticker has a filing for.
pub fn reporting_items(
    ticker: &str,
    offsets: &PojkOffsets,
    filed: &HashSet<(i32, u32)>,
    today: NaiveDate,
    horizon_days: i64,
) -> Vec<Item> {
    let mut out = Vec::new();
    for fy in today.year() - 1..=today.year() + 1 {
        for q in 1..=4 {
            let Some(w) = pojk_window(offsets, fy, q) else {
                continue;
            };
            if filed.contains(&(fy, q))
                || w.end < today - Duration::days(LOOKBACK_DAYS)
                || w.end > today + Duration::days(horizon_days)
            {
                continue;
            }
            out.push(Item {
                date: w.end,
                ticker: ticker.into(),
                kind: "pojk".into(),
                label: format!("Batas laporan {}", quarter_label(fy, q)),
                source: "pojk",
                overdue: w.end < today,
                window: Some(w),
                projected: false,
                amount: None,
                note: None,
            });
        }
    }
    out
}

/// The quarter after the latest filed one and its deadline: `LAPORAN BERIKUT` on the
/// coverage board. `None` for a ticker with no filing.
pub fn next_report(
    offsets: &PojkOffsets,
    latest_filed: Option<(i32, u32)>,
) -> Option<(i32, u32, Window)> {
    let (fy, q) = latest_filed?;
    let (nfy, nq) = if q >= 4 { (fy + 1, 1) } else { (fy, q + 1) };
    Some((nfy, nq, pojk_window(offsets, nfy, nq)?))
}

/// Ex-dates from `corp_events` inside the look-back and horizon, plus projections one year
/// after a past ex-date that no reported date already covers.
pub fn dividend_items(
    ticker: &str,
    dividends: &[(NaiveDate, f64)],
    today: NaiveDate,
    horizon_days: i64,
) -> Vec<Item> {
    let (from, to) = (
        today - Duration::days(LOOKBACK_DAYS),
        today + Duration::days(horizon_days),
    );
    let mut out: Vec<Item> = dividends
        .iter()
        .filter(|(d, _)| *d >= from && *d <= to)
        .map(|(d, amount)| Item {
            date: *d,
            ticker: ticker.into(),
            kind: "dividend".into(),
            label: "Tanggal ex dividen".into(),
            source: "corp_events",
            window: None,
            projected: false,
            overdue: false,
            amount: Some(*amount),
            note: None,
        })
        .collect();
    for (d, amount) in dividends {
        let Some(next) = d.checked_add_months(chrono::Months::new(12)) else {
            continue;
        };
        if next <= today || next > to {
            continue;
        }
        let covered = dividends
            .iter()
            .any(|(o, _)| (*o - next).num_days().abs() <= PROJECTION_TOLERANCE_DAYS);
        if covered {
            continue;
        }
        out.push(Item {
            date: next,
            ticker: ticker.into(),
            kind: "dividend".into(),
            label: "Perkiraan ex dividen (tahun lalu)".into(),
            source: "corp_events",
            window: None,
            projected: true,
            overdue: false,
            amount: Some(*amount),
            note: Some(format!("dari tanggal {d}")),
        });
    }
    out
}

/// One owner-entered date.
#[derive(Clone, Debug, PartialEq)]
pub struct Manual {
    pub date: NaiveDate,
    pub kind: String,
    pub note: Option<String>,
}

/// Owner dates from a week ago to the horizon.
pub fn manual_items(
    ticker: &str,
    manual: &[Manual],
    today: NaiveDate,
    horizon_days: i64,
) -> Vec<Item> {
    manual
        .iter()
        .filter(|m| {
            m.date >= today - Duration::days(7) && m.date <= today + Duration::days(horizon_days)
        })
        .map(|m| Item {
            date: m.date,
            ticker: ticker.into(),
            kind: m.kind.clone(),
            label: manual_label(&m.kind).unwrap_or("Lainnya").into(),
            source: "manual",
            window: None,
            projected: false,
            overdue: false,
            amount: None,
            note: m.note.clone(),
        })
        .collect()
}

/// Sorted by date, then ticker, then kind.
pub fn sorted(mut items: Vec<Item>) -> Vec<Item> {
    items.sort_by(|a, b| {
        (a.date, &a.ticker, &a.kind, a.projected).cmp(&(b.date, &b.ticker, &b.kind, b.projected))
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
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
    fn every_pojk_window_ends_its_offset_after_the_period_end() {
        let o = offsets();
        let cases = [
            (1, "2026-04-01", "2026-04-30"),
            (2, "2026-07-01", "2026-08-29"),
            (3, "2026-10-01", "2026-10-30"),
            (4, "2027-01-01", "2027-03-31"),
        ];
        for (q, start, end) in cases {
            // The FY window of 2026 opens on 2027-01-01.
            let w = pojk_window(&o, 2026, q).unwrap();
            assert_eq!((w.start, w.end), (d(start), d(end)), "Q{q}");
        }
        assert_eq!(pojk_window(&o, 2026, 5), None);
        // Offsets come from the rules: a changed rule moves the deadline.
        let longer = PojkOffsets { q3: 45, ..o };
        assert_eq!(pojk_window(&longer, 2026, 3).unwrap().end, d("2026-11-14"));
    }

    #[test]
    fn unfiled_windows_in_range_are_listed_and_filed_ones_are_not() {
        let o = offsets();
        let today = d("2026-10-24");
        let mut filed: HashSet<(i32, u32)> = HashSet::new();
        filed.insert((2026, 2));
        let items = reporting_items("COAL", &o, &filed, today, HORIZON_DAYS);
        let labels: Vec<(String, NaiveDate, bool)> = items
            .iter()
            .map(|i| (i.label.clone(), i.date, i.overdue))
            .collect();
        // Q3 is due on 2026-10-30; the FY window closes 2027-03-31, beyond 90 days.
        assert_eq!(
            labels,
            [("Batas laporan 2026Q3".to_string(), d("2026-10-30"), false)]
        );
        assert_eq!(
            items[0].window,
            Some(Window {
                start: d("2026-10-01"),
                end: d("2026-10-30")
            })
        );
        // Q2 not filed: its deadline (2026-08-29) is 56 days back, outside the look-back.
        assert!(
            reporting_items("COAL", &o, &HashSet::new(), today, HORIZON_DAYS)
                .iter()
                .all(|i| i.label != "Batas laporan 2026Q2")
        );
        // Two weeks after the Q3 deadline, still unfiled: listed and flagged overdue.
        let late = reporting_items("COAL", &o, &HashSet::new(), d("2026-11-13"), HORIZON_DAYS);
        let q3 = late
            .iter()
            .find(|i| i.label == "Batas laporan 2026Q3")
            .unwrap();
        assert!(q3.overdue);
        // 31 days after the deadline it drops off.
        let gone = reporting_items("COAL", &o, &HashSet::new(), d("2026-11-30"), HORIZON_DAYS);
        assert!(gone.iter().all(|i| i.label != "Batas laporan 2026Q3"));
        // The horizon is honoured: a short one hides Q3 from a week before.
        assert!(reporting_items("COAL", &o, &HashSet::new(), d("2026-10-01"), 10).is_empty());
    }

    #[test]
    fn next_report_wraps_the_year() {
        let o = offsets();
        let (fy, q, w) = next_report(&o, Some((2026, 2))).unwrap();
        assert_eq!((fy, q, w.end), (2026, 3, d("2026-10-30")));
        let (fy, q, w) = next_report(&o, Some((2026, 4))).unwrap();
        assert_eq!((fy, q, w.end), (2027, 1, d("2027-04-30")));
        assert!(next_report(&o, None).is_none());
    }

    #[test]
    fn dividends_are_reported_dates_with_labelled_projections() {
        let today = d("2026-10-24");
        let history = [(d("2025-11-20"), 12.0), (d("2026-05-15"), 20.0)];
        let items = dividend_items("COAL", &history, today, HORIZON_DAYS);
        // Nothing reported inside the window; 2025-11-20 + 1 year is a projection.
        assert_eq!(items.len(), 1);
        let p = &items[0];
        assert!(p.projected);
        assert_eq!(p.date, d("2026-11-20"));
        assert_eq!(p.amount, Some(12.0));
        assert_eq!(p.label, "Perkiraan ex dividen (tahun lalu)");
        assert_eq!(p.note.as_deref(), Some("dari tanggal 2025-11-20"));
        // A reported date near the projection replaces it.
        let history = [
            (d("2025-11-20"), 12.0),
            (d("2026-11-10"), 13.0),
            (d("2026-05-15"), 20.0),
        ];
        let items = dividend_items("COAL", &history, today, HORIZON_DAYS);
        assert_eq!(items.len(), 1);
        assert!(!items[0].projected);
        assert_eq!(items[0].date, d("2026-11-10"));
        assert_eq!(items[0].source, "corp_events");
        // Recent past ex-dates stay listed for the look-back, older ones do not.
        let items = dividend_items(
            "COAL",
            &[(d("2026-10-01"), 5.0), (d("2026-08-01"), 5.0)],
            today,
            90,
        );
        assert_eq!(items.iter().filter(|i| !i.projected).count(), 1);
        assert!(dividend_items("COAL", &[], today, 90).is_empty());
    }

    #[test]
    fn manual_dates_are_listed_from_a_week_back_and_labelled() {
        let today = d("2026-10-24");
        let manual = [
            Manual {
                date: d("2026-10-10"),
                kind: "rups".into(),
                note: None,
            },
            Manual {
                date: d("2026-10-20"),
                kind: "paparan".into(),
                note: Some("paparan publik".into()),
            },
            Manual {
                date: d("2026-12-01"),
                kind: "rups".into(),
                note: Some("RUPS tahunan".into()),
            },
            Manual {
                date: d("2027-06-01"),
                kind: "lainnya".into(),
                note: None,
            },
        ];
        let items = manual_items("COAL", &manual, today, HORIZON_DAYS);
        let got: Vec<(&str, &str)> = items
            .iter()
            .map(|i| (i.label.as_str(), i.note.as_deref().unwrap_or("")))
            .collect();
        assert_eq!(
            got,
            [("Paparan", "paparan publik"), ("RUPS", "RUPS tahunan")]
        );
        assert!(items.iter().all(|i| i.source == "manual" && !i.projected));
        assert_eq!(manual_label("rups"), Some("RUPS"));
        assert_eq!(manual_label("beli"), None);
    }

    #[test]
    fn items_sort_by_date_then_ticker_then_kind() {
        let today = d("2026-10-24");
        let mut all = reporting_items("COAL", &offsets(), &HashSet::new(), today, 200);
        all.extend(reporting_items(
            "BANK",
            &offsets(),
            &HashSet::new(),
            today,
            200,
        ));
        let sorted = sorted(all);
        let keys: Vec<(String, String)> = sorted
            .iter()
            .map(|i| (i.date.to_string(), i.ticker.clone()))
            .collect();
        let mut check = keys.clone();
        check.sort();
        assert_eq!(keys, check);
        assert_eq!(sorted[0].ticker, "BANK");
    }
}
