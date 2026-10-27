//! Claims (research plan §3.1 `claims.rs`, D8; prd.md §6.1 item 7, A4.1; design.md §3.4):
//! a statement bound to a metric and a condition, evaluated by a pure function over the
//! observations that existed on the evaluation date. Status words are claim vocabulary
//! only (D3): holding (`Berlaku`), broken (`Patah`), not yet testable (`Belum teruji`),
//! expired without data (`Kedaluwarsa`). The system never judges the metric itself.
//!
//! `evaluate` is deterministic: the same spec, observations (in any order) and date give
//! the same evaluation. The store-facing half (metric lookup, re-evaluation) is `source`.

pub mod source;

use std::collections::BTreeMap;
use std::fmt;

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{json, Value};

use super::packs::Quarter;

pub const MAX_STATEMENT_CHARS: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Comparator {
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
    #[serde(rename = "between")]
    Between,
}

impl Comparator {
    pub const ALL: [Comparator; 5] = [Self::Lt, Self::Le, Self::Gt, Self::Ge, Self::Between];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Between => "between",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }

    /// `between` is inclusive at both ends.
    pub fn holds(self, value: f64, threshold: f64, hi: Option<f64>) -> bool {
        // Six decimals: a ratio computed as 100 × a ÷ b that lands a hair under an exact
        // 10 must not break a claim of "at least 10".
        let round = |x: f64| (x * 1e6).round() / 1e6;
        let (value, threshold, hi) = (round(value), round(threshold), hi.map(round));
        match self {
            Self::Lt => value < threshold,
            Self::Le => value <= threshold,
            Self::Gt => value > threshold,
            Self::Ge => value >= threshold,
            Self::Between => hi.is_some_and(|hi| value >= threshold && value <= hi),
        }
    }
}

/// Which observation the claim looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// The newest observation known on the evaluation date.
    Latest,
    /// The newest observation dated on or before the due date, tested once that date came.
    AtDue,
    /// Every quarter whose period ends on or before the due date: one breach breaks it.
    EveryQuarter,
    /// One named quarter.
    Quarter(Quarter),
}

impl Window {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "latest" => Some(Self::Latest),
            "at_due" => Some(Self::AtDue),
            "every_quarter" => Some(Self::EveryQuarter),
            _ => {
                let rest = s.strip_prefix("quarter:")?;
                let (fy, q) = rest.split_once('Q')?;
                let (fy, q) = (fy.parse::<i32>().ok()?, q.parse::<u32>().ok()?);
                (1..=4)
                    .contains(&q)
                    .then(|| Self::Quarter(Quarter::new(fy, q)))
            }
        }
    }

    /// Windows that read quarterly observations; a price metric cannot use them.
    pub fn needs_quarters(self) -> bool {
        matches!(self, Self::EveryQuarter | Self::Quarter(_))
    }
}

impl fmt::Display for Window {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Latest => f.write_str("latest"),
            Self::AtDue => f.write_str("at_due"),
            Self::EveryQuarter => f.write_str("every_quarter"),
            Self::Quarter(q) => write!(f, "quarter:{q}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Holding,
    Broken,
    NotTestable,
    Expired,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Holding => "holding",
            Self::Broken => "broken",
            Self::NotTestable => "not_testable",
            Self::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Holding,
            Self::Broken,
            Self::NotTestable,
            Self::Expired,
        ]
        .into_iter()
        .find(|x| x.as_str() == s)
    }

    /// The Indonesian UI word (design.md §3.4).
    pub fn label(self) -> &'static str {
        match self {
            Self::Holding => "Berlaku",
            Self::Broken => "Patah",
            Self::NotTestable => "Belum teruji",
            Self::Expired => "Kedaluwarsa",
        }
    }

    /// Sort rank in the claim list: Patah, Belum teruji, Berlaku, Kedaluwarsa.
    pub fn rank(self) -> u8 {
        match self {
            Self::Broken => 0,
            Self::NotTestable => 1,
            Self::Holding => 2,
            Self::Expired => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    pub comparator: Comparator,
    pub threshold: f64,
    pub threshold_hi: Option<f64>,
    pub window: Window,
    /// The day the claim was written: `every_quarter` looks at quarters whose report became
    /// available on or after it, not at the history that already existed.
    pub since: NaiveDate,
    pub due: NaiveDate,
}

/// Why a spec is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecError {
    pub field: &'static str,
    pub message: &'static str,
}

/// Threshold rules: finite numbers; `between` needs `threshold_hi >= threshold`, the other
/// comparators take no upper bound.
pub fn check_thresholds(
    comparator: Comparator,
    threshold: f64,
    threshold_hi: Option<f64>,
) -> Result<(), SpecError> {
    if !threshold.is_finite() {
        return Err(SpecError {
            field: "threshold",
            message: "Threshold must be a finite number",
        });
    }
    match (comparator, threshold_hi) {
        (Comparator::Between, None) => Err(SpecError {
            field: "threshold_hi",
            message: "between needs threshold_hi",
        }),
        (Comparator::Between, Some(hi)) if !hi.is_finite() || hi < threshold => Err(SpecError {
            field: "threshold_hi",
            message: "threshold_hi must be a number not below threshold",
        }),
        (Comparator::Between, Some(_)) => Ok(()),
        (_, Some(_)) => Err(SpecError {
            field: "threshold_hi",
            message: "threshold_hi only applies to between",
        }),
        (_, None) => Ok(()),
    }
}

/// One value of the bound metric and when it became known.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Observation {
    /// The fiscal quarter, for quarterly metrics.
    pub quarter: Option<Quarter>,
    /// Period end (quarterly) or trading day (price).
    pub date: NaiveDate,
    /// Point in time: the observation exists for an evaluation only from this date on.
    pub available_from: NaiveDate,
    pub value: f64,
    /// Where the value came from (the metric's operands, or the KPI document).
    pub inputs: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    pub status: Status,
    /// The value the status rests on (the first breach when broken, else the newest).
    pub value: Option<f64>,
    /// The observations the status rests on.
    pub used: Vec<Observation>,
    /// A short Indonesian reason when the status is not holding or broken.
    pub reason: Option<&'static str>,
}

impl Evaluation {
    /// The JSON stored in `claim_evals.inputs`.
    pub fn inputs_json(&self, window: Window, as_of: NaiveDate) -> Value {
        json!({
            "window": window.to_string(),
            "as_of": as_of.to_string(),
            "observations": self.used,
            "reason": self.reason,
        })
    }
}

/// Total order that does not depend on input order: date, availability, then value.
fn order(a: &Observation, b: &Observation) -> std::cmp::Ordering {
    (a.date, a.available_from)
        .cmp(&(b.date, b.available_from))
        .then(a.value.total_cmp(&b.value))
}

/// One observation per quarter (the version known latest, so a restated filing replaces the
/// earlier one once it is available); price-like observations are kept apart by date.
fn newest_versions<'a>(known: &[&'a Observation]) -> Vec<&'a Observation> {
    let mut by_key: BTreeMap<(Option<Quarter>, Option<NaiveDate>), &Observation> = BTreeMap::new();
    for o in known {
        let key = (o.quarter, o.quarter.is_none().then_some(o.date));
        let better = by_key.get(&key).is_none_or(|cur| {
            (o.available_from, o.value.to_bits()) > (cur.available_from, cur.value.to_bits())
        });
        if better {
            by_key.insert(key, o);
        }
    }
    let mut out: Vec<&Observation> = by_key.into_values().collect();
    out.sort_by(|a, b| order(a, b));
    out
}

fn decide(spec: &Spec, o: &Observation) -> Status {
    if spec
        .comparator
        .holds(o.value, spec.threshold, spec.threshold_hi)
    {
        Status::Holding
    } else {
        Status::Broken
    }
}

fn single(spec: &Spec, o: Option<&Observation>, none: Evaluation) -> Evaluation {
    match o {
        Some(o) => Evaluation {
            status: decide(spec, o),
            value: Some(o.value),
            used: vec![o.clone()],
            reason: None,
        },
        None => none,
    }
}

/// Evaluates `spec` over `observations` as of `as_of`. Only observations whose
/// `available_from <= as_of` take part (point in time, plan §4.4). Without one the claim is
/// not yet testable, or expired once `as_of` is past the due date.
pub fn evaluate(spec: &Spec, observations: &[Observation], as_of: NaiveDate) -> Evaluation {
    let known: Vec<&Observation> = observations
        .iter()
        .filter(|o| o.available_from <= as_of)
        .collect();
    let versions = newest_versions(&known);
    let no_data = if as_of > spec.due {
        Evaluation {
            status: Status::Expired,
            value: None,
            used: Vec::new(),
            reason: Some("lewat jatuh tempo tanpa data"),
        }
    } else {
        Evaluation {
            status: Status::NotTestable,
            value: None,
            used: Vec::new(),
            reason: Some("belum ada data"),
        }
    };
    match spec.window {
        Window::Latest => single(spec, versions.last().copied(), no_data),
        Window::Quarter(q) => single(
            spec,
            versions
                .iter()
                .rev()
                .find(|o| o.quarter == Some(q))
                .copied(),
            no_data,
        ),
        Window::AtDue => {
            if as_of < spec.due {
                return Evaluation {
                    status: Status::NotTestable,
                    value: None,
                    used: Vec::new(),
                    reason: Some("jatuh tempo belum tiba"),
                };
            }
            single(
                spec,
                versions.iter().rev().find(|o| o.date <= spec.due).copied(),
                no_data,
            )
        }
        Window::EveryQuarter => {
            let inside: Vec<&Observation> = versions
                .into_iter()
                .filter(|o| o.date <= spec.due && o.available_from >= spec.since)
                .collect();
            if inside.is_empty() {
                return no_data;
            }
            let breach = inside
                .iter()
                .find(|o| decide(spec, o) == Status::Broken)
                .copied();
            let used = inside.iter().map(|o| (*o).clone()).collect();
            match breach {
                Some(o) => Evaluation {
                    status: Status::Broken,
                    value: Some(o.value),
                    used,
                    reason: None,
                },
                None => Evaluation {
                    status: Status::Holding,
                    value: inside.last().map(|o| o.value),
                    used,
                    reason: None,
                },
            }
        }
    }
}

/// Numbers without trailing zeros, dot decimal (the API speaks JSON; the UI formats).
pub fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn condition(spec: &Spec, unit: &str) -> String {
    let u = if unit.is_empty() {
        String::new()
    } else {
        format!(" {unit}")
    };
    match spec.comparator {
        Comparator::Between => format!(
            "antara {} dan {}{u}",
            fmt_num(spec.threshold),
            fmt_num(spec.threshold_hi.unwrap_or(spec.threshold))
        ),
        c => format!("{} {}{u}", c.as_str(), fmt_num(spec.threshold)),
    }
}

/// The sentence the claim editor shows before saving (design.md §4.5 `ClaimEditor`).
pub fn preview(metric_label: &str, unit: &str, spec: &Spec) -> String {
    let cond = condition(spec, unit);
    match spec.window {
        Window::Latest => {
            format!("Akan dievaluasi setiap ada data baru: {metric_label} terbaru {cond}.")
        }
        Window::AtDue => format!("Akan dievaluasi pada {}: {metric_label} {cond}.", spec.due),
        Window::EveryQuarter => format!(
            "Akan dievaluasi saat tiap laporan kuartal yang terbit sejak {} sampai {}: {metric_label} {cond}.",
            spec.since, spec.due
        ),
        Window::Quarter(q) => {
            format!("Akan dievaluasi saat laporan {q} tersedia: {metric_label} {cond}.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn obs(q: Option<(i32, u32)>, date: &str, avail: &str, value: f64) -> Observation {
        Observation {
            quarter: q.map(|(fy, q)| Quarter::new(fy, q)),
            date: d(date),
            available_from: d(avail),
            value,
            inputs: Value::Null,
        }
    }

    fn spec(comparator: Comparator, threshold: f64, hi: Option<f64>, window: Window) -> Spec {
        Spec {
            comparator,
            threshold,
            threshold_hi: hi,
            window,
            since: d("2026-01-01"),
            due: d("2026-12-31"),
        }
    }

    fn q3(value: f64) -> Vec<Observation> {
        vec![obs(Some((2026, 3)), "2026-09-30", "2026-10-30", value)]
    }

    #[test]
    fn every_comparator_at_below_on_and_above_the_threshold() {
        // (comparator, holds below 10, holds on 10, holds above 10)
        let table = [
            (Comparator::Lt, true, false, false),
            (Comparator::Le, true, true, false),
            (Comparator::Gt, false, false, true),
            (Comparator::Ge, false, true, true),
        ];
        for (c, below, on, above) in table {
            for (value, want) in [(9.99, below), (10.0, on), (10.01, above)] {
                let e = evaluate(
                    &spec(c, 10.0, None, Window::Latest),
                    &q3(value),
                    d("2026-11-01"),
                );
                let status = if want {
                    Status::Holding
                } else {
                    Status::Broken
                };
                assert_eq!(e.status, status, "{} with {value}", c.as_str());
                assert_eq!(e.value, Some(value));
            }
        }
    }

    #[test]
    fn between_is_inclusive_and_needs_an_upper_bound() {
        let s = spec(Comparator::Between, 10.0, Some(20.0), Window::Latest);
        for (value, want) in [
            (9.99, Status::Broken),
            (10.0, Status::Holding),
            (15.0, Status::Holding),
            (20.0, Status::Holding),
            (20.01, Status::Broken),
        ] {
            assert_eq!(evaluate(&s, &q3(value), d("2026-11-01")).status, want);
        }
        assert!(check_thresholds(Comparator::Between, 10.0, None).is_err());
        assert!(check_thresholds(Comparator::Between, 10.0, Some(9.0)).is_err());
        assert!(check_thresholds(Comparator::Between, 10.0, Some(10.0)).is_ok());
        assert!(check_thresholds(Comparator::Lt, 10.0, Some(12.0)).is_err());
        assert!(check_thresholds(Comparator::Lt, f64::NAN, None).is_err());
        assert!(check_thresholds(Comparator::Ge, 10.0, None).is_ok());
    }

    #[test]
    fn comparator_and_window_text_round_trip() {
        for c in Comparator::ALL {
            assert_eq!(Comparator::parse(c.as_str()), Some(c));
        }
        assert_eq!(Comparator::parse("=="), None);
        for w in ["latest", "at_due", "every_quarter", "quarter:2026Q3"] {
            assert_eq!(Window::parse(w).unwrap().to_string(), w);
        }
        for bad in [
            "",
            "quarter:",
            "quarter:2026Q5",
            "quarter:2026",
            "daily",
            "quarter:xQ1",
        ] {
            assert_eq!(Window::parse(bad), None, "{bad}");
        }
        for s in [
            Status::Holding,
            Status::Broken,
            Status::NotTestable,
            Status::Expired,
        ] {
            assert_eq!(Status::parse(s.as_str()), Some(s));
        }
        assert_eq!(Status::Holding.label(), "Berlaku");
        assert_eq!(Status::Broken.label(), "Patah");
        assert_eq!(Status::NotTestable.label(), "Belum teruji");
        assert_eq!(Status::Expired.label(), "Kedaluwarsa");
    }

    #[test]
    fn latest_window_takes_the_newest_known_observation() {
        let all = vec![
            obs(Some((2026, 2)), "2026-06-30", "2026-07-30", 5.0),
            obs(Some((2026, 3)), "2026-09-30", "2026-10-30", 15.0),
        ];
        let s = spec(Comparator::Gt, 10.0, None, Window::Latest);
        assert_eq!(evaluate(&s, &all, d("2026-11-01")).status, Status::Holding);
        // Before the Q3 filing is available the newest known value is Q2's.
        let e = evaluate(&s, &all, d("2026-10-01"));
        assert_eq!((e.status, e.value), (Status::Broken, Some(5.0)));
    }

    #[test]
    fn quarter_window_waits_for_its_quarter() {
        let s = spec(
            Comparator::Lt,
            20.0,
            None,
            Window::Quarter(Quarter::new(2026, 3)),
        );
        let e = evaluate(&s, &q3(15.0), d("2026-10-29"));
        assert_eq!(e.status, Status::NotTestable, "filing not available yet");
        assert_eq!(e.reason, Some("belum ada data"));
        assert_eq!(
            evaluate(&s, &q3(15.0), d("2026-10-30")).status,
            Status::Holding
        );
        // A different quarter's data does not answer this claim.
        let other = vec![obs(Some((2026, 2)), "2026-06-30", "2026-07-30", 15.0)];
        assert_eq!(
            evaluate(&s, &other, d("2026-11-01")).status,
            Status::NotTestable
        );
    }

    #[test]
    fn every_quarter_window_breaks_on_the_first_breach_and_ignores_later_quarters() {
        let all = vec![
            obs(Some((2026, 1)), "2026-03-31", "2026-04-30", 12.0),
            obs(Some((2026, 2)), "2026-06-30", "2026-07-30", 8.0),
            obs(Some((2026, 3)), "2026-09-30", "2026-10-30", 9.0),
            obs(Some((2027, 1)), "2027-03-31", "2027-04-30", 1.0),
        ];
        let s = spec(Comparator::Ge, 10.0, None, Window::EveryQuarter);
        let e = evaluate(&s, &all, d("2026-11-01"));
        assert_eq!((e.status, e.value), (Status::Broken, Some(8.0)));
        assert_eq!(e.used.len(), 3);
        // Only Q1 known so far: holding.
        let e = evaluate(&s, &all, d("2026-05-01"));
        assert_eq!((e.status, e.value), (Status::Holding, Some(12.0)));
        // 2027Q1 ends after the due date and is outside the window.
        let s = spec(Comparator::Ge, 8.0, None, Window::EveryQuarter);
        let e = evaluate(&s, &all, d("2027-05-01"));
        assert_eq!((e.status, e.value), (Status::Holding, Some(9.0)));
    }

    #[test]
    fn every_quarter_ignores_history_that_existed_before_the_claim() {
        let all = vec![
            obs(Some((2025, 4)), "2025-12-31", "2026-03-31", 3.0),
            obs(Some((2026, 1)), "2026-03-31", "2026-04-30", 12.0),
            obs(Some((2026, 2)), "2026-06-30", "2026-07-30", 11.0),
        ];
        let mut s = spec(Comparator::Ge, 10.0, None, Window::EveryQuarter);
        // Written in May: the 2025 quarter (filed in March) is history, not part of the claim.
        s.since = d("2026-05-01");
        let e = evaluate(&s, &all, d("2026-08-01"));
        assert_eq!((e.status, e.value), (Status::Holding, Some(11.0)));
        assert_eq!(e.used.len(), 1);
        // Written before it was filed, the same quarter counts and breaks the claim.
        s.since = d("2026-01-01");
        let e = evaluate(&s, &all, d("2026-08-01"));
        assert_eq!((e.status, e.value), (Status::Broken, Some(3.0)));
        // Nothing reported since the claim was written: not testable yet.
        s.since = d("2026-08-02");
        assert_eq!(
            evaluate(&s, &all, d("2026-08-03")).status,
            Status::NotTestable
        );
    }

    #[test]
    fn a_ratio_a_hair_under_the_threshold_still_holds_at_the_displayed_value() {
        let almost_ten = 10.0 - 1e-15;
        let just_over = 10.0 + 1e-15;
        assert!(almost_ten < 10.0 && just_over > 10.0);
        assert!(Comparator::Ge.holds(almost_ten, 10.0, None));
        assert!(Comparator::Le.holds(just_over, 10.0, None));
        assert!(!Comparator::Gt.holds(just_over, 10.0, None));
        assert!(!Comparator::Ge.holds(9.99, 10.0, None));
    }

    #[test]
    fn at_due_window_is_tested_only_once_the_due_date_came() {
        let closes = vec![
            obs(None, "2026-12-29", "2026-12-29", 1_100.0),
            obs(None, "2026-12-30", "2026-12-30", 1_300.0),
            obs(None, "2027-01-04", "2027-01-04", 900.0),
        ];
        let s = spec(Comparator::Ge, 1_200.0, None, Window::AtDue);
        let e = evaluate(&s, &closes, d("2026-12-30"));
        assert_eq!(e.status, Status::NotTestable);
        assert_eq!(e.reason, Some("jatuh tempo belum tiba"));
        // On the due date: the newest close on or before it.
        let e = evaluate(&s, &closes, d("2026-12-31"));
        assert_eq!((e.status, e.value), (Status::Holding, Some(1_300.0)));
        // A later close does not move a due-date claim.
        let e = evaluate(&s, &closes, d("2027-01-10"));
        assert_eq!((e.status, e.value), (Status::Holding, Some(1_300.0)));
    }

    #[test]
    fn no_data_is_not_testable_until_the_due_date_then_expired() {
        let s = spec(Comparator::Gt, 1.0, None, Window::Latest);
        let e = evaluate(&s, &[], d("2026-12-31"));
        assert_eq!((e.status, e.value), (Status::NotTestable, None));
        let e = evaluate(&s, &[], d("2027-01-01"));
        assert_eq!(e.status, Status::Expired);
        assert_eq!(e.reason, Some("lewat jatuh tempo tanpa data"));
        // Data that arrives after the due date still answers the claim.
        let late = vec![obs(Some((2026, 4)), "2026-12-31", "2027-02-15", 5.0)];
        assert_eq!(evaluate(&s, &late, d("2027-03-01")).status, Status::Holding);
    }

    #[test]
    fn point_in_time_ignores_observations_not_yet_available() {
        // Q3 ended before the as-of date but was filed after it.
        let s = spec(Comparator::Lt, 10.0, None, Window::Latest);
        let all = vec![
            obs(Some((2026, 2)), "2026-06-30", "2026-07-30", 5.0),
            obs(Some((2026, 3)), "2026-09-30", "2026-11-20", 50.0),
        ];
        let e = evaluate(&s, &all, d("2026-11-19"));
        assert_eq!((e.status, e.value), (Status::Holding, Some(5.0)));
        let e = evaluate(&s, &all, d("2026-11-20"));
        assert_eq!((e.status, e.value), (Status::Broken, Some(50.0)));
    }

    #[test]
    fn a_restated_quarter_is_replaced_once_the_later_filing_is_available() {
        let s = spec(
            Comparator::Lt,
            10.0,
            None,
            Window::Quarter(Quarter::new(2026, 2)),
        );
        let all = vec![
            obs(Some((2026, 2)), "2026-06-30", "2026-07-30", 8.0),
            obs(Some((2026, 2)), "2026-06-30", "2026-10-30", 12.0),
        ];
        assert_eq!(evaluate(&s, &all, d("2026-09-01")).status, Status::Holding);
        assert_eq!(evaluate(&s, &all, d("2026-11-01")).status, Status::Broken);
    }

    #[test]
    fn preview_sentences_name_the_window_and_condition() {
        let mut s = spec(Comparator::Ge, 12.5, None, Window::Latest);
        assert_eq!(
            preview("Rasio CASA", "%", &s),
            "Akan dievaluasi setiap ada data baru: Rasio CASA terbaru >= 12.5 %."
        );
        s.window = Window::Quarter(Quarter::new(2026, 3));
        assert!(preview("Rasio CASA", "%", &s).contains("saat laporan 2026Q3 tersedia"));
        s.window = Window::EveryQuarter;
        assert!(preview("Rasio CASA", "%", &s)
            .contains("yang terbit sejak 2026-01-01 sampai 2026-12-31"));
        s.window = Window::AtDue;
        s.comparator = Comparator::Between;
        s.threshold_hi = Some(20.0);
        assert_eq!(
            preview("Harga penutupan", "", &s),
            "Akan dievaluasi pada 2026-12-31: Harga penutupan antara 12.5 dan 20."
        );
    }

    fn arb_obs() -> impl Strategy<Value = Observation> {
        (
            proptest::option::of((2024i32..2028, 1u32..5)),
            0i64..900,
            0i64..120,
            -1_000.0f64..1_000.0,
        )
            .prop_map(|(q, day, lag, value)| {
                let date = d("2024-01-01") + chrono::Duration::days(day);
                Observation {
                    quarter: q.map(|(fy, q)| Quarter::new(fy, q)),
                    date,
                    available_from: date + chrono::Duration::days(lag),
                    value,
                    inputs: Value::Null,
                }
            })
    }

    fn arb_spec() -> impl Strategy<Value = Spec> {
        (
            0usize..5,
            -500.0f64..500.0,
            0.0f64..500.0,
            0usize..4,
            0i64..1_000,
        )
            .prop_map(|(c, threshold, span, w, due)| {
                let comparator = Comparator::ALL[c];
                Spec {
                    comparator,
                    threshold,
                    threshold_hi: (comparator == Comparator::Between).then_some(threshold + span),
                    window: match w {
                        0 => Window::Latest,
                        1 => Window::AtDue,
                        2 => Window::EveryQuarter,
                        _ => Window::Quarter(Quarter::new(2025, 2)),
                    },
                    since: d("2024-01-01"),
                    due: d("2024-01-01") + chrono::Duration::days(due),
                }
            })
    }

    proptest! {
        #[test]
        fn evaluation_is_deterministic_and_independent_of_input_order(
            spec in arb_spec(),
            mut all in proptest::collection::vec(arb_obs(), 0..12),
            as_of in 0i64..1_200,
            seed in any::<u64>(),
        ) {
            let as_of = d("2024-01-01") + chrono::Duration::days(as_of);
            let first = evaluate(&spec, &all, as_of);
            prop_assert_eq!(&first, &evaluate(&spec, &all, as_of));
            // A deterministic shuffle of the same observations.
            let n = all.len();
            if n > 1 {
                let mut x = seed | 1;
                for i in (1..n).rev() {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    all.swap(i, (x % (i as u64 + 1)) as usize);
                }
            }
            let shuffled = evaluate(&spec, &all, as_of);
            prop_assert_eq!(first.status, shuffled.status);
            prop_assert_eq!(first.value, shuffled.value);
            // Nothing that was not yet available can be among the observations used.
            prop_assert!(first.used.iter().all(|o| o.available_from <= as_of));
        }
    }
}
