//! Point-in-time share count resolution (research plan D13, R1.4 and R3.2).
//!
//! Chain, first hit wins, the source is always returned with the number:
//! 1. a share-count fact for the period itself (instant at, or duration ending on, the date);
//! 2. the most recent of the latest share-count instant and the owner's latest manual
//!    count dated on or before the date (on the same date the filing wins);
//! 3. the EPS-implied count of the latest filing period ending on or before the date
//!    (net income ÷ basic EPS, a derivation labelled as such; never overrides 2);
//! 4. the earliest manual count entered for a later date (shown with its own `as_of`, so
//!    the reader sees the distance).
//!
//! What counts as a share-count fact: any fact with unit `shares`, or one of the elements
//! in `SHARE_COUNT_ELEMENTS`. R1.4 checked every filing on disk (139 files, three
//! taxonomies): none carries either, so in practice a resolution comes from the owner's
//! entries (including the v4 scraper snapshot carried over in R3.2) or from the EPS-implied
//! derivation. Pure: callers load the facts, manual rows and EPS pairs.

use chrono::NaiveDate;
use serde::Serialize;

/// Elements that carry a share count when a filing has one, in priority order.
pub const SHARE_COUNT_ELEMENTS: &[&str] = &[
    "idx-cor:NumberOfSharesOutstanding",
    "idx-cor:NumberOfSharesIssued",
    "idx-cor:NumberOfOrdinarySharesOutstanding",
    "idx-cor:IssuedShares",
    "idx-dei:NumberOfSharesOutstanding",
    "ifrs-full:NumberOfSharesOutstanding",
    "ifrs-full:NumberOfSharesIssued",
    "ifrs-full:WeightedAverageNumberOfOrdinarySharesOutstanding",
];

/// Numerators for the EPS-implied count, in priority order: basic EPS is defined on the
/// profit attributable to the parent's owners, so that element is preferred when tagged.
pub const EPS_NUMERATOR_ELEMENTS: &[&str] = &[
    "idx-cor:ProfitLossAttributableToParentEntity",
    "idx-cor:ProfitLoss",
];

/// Basic EPS elements, in priority order.
pub const EPS_ELEMENTS: &[&str] = &[
    "idx-cor:BasicEarningsLossPerShare",
    "idx-cor:BasicEarningsLossPerShareFromContinuingOperations",
    "ifrs-full:BasicEarningsLossPerShare",
];

/// Named elements always count; any other `shares`-unit fact counts unless its name says
/// it is not the outstanding count (treasury or authorised shares).
pub fn is_share_count(element: &str, unit: &str) -> bool {
    SHARE_COUNT_ELEMENTS.contains(&element)
        || (unit == "shares"
            && !["Treasury", "Authorised", "Authorized"]
                .iter()
                .any(|w| element.contains(w)))
}

fn element_rank(element: &str) -> usize {
    SHARE_COUNT_ELEMENTS
        .iter()
        .position(|e| *e == element)
        .unwrap_or(SHARE_COUNT_ELEMENTS.len())
}

fn rank_in(list: &[&str], element: &str) -> usize {
    list.iter()
        .position(|e| *e == element)
        .unwrap_or(list.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Instant,
    Duration,
}

/// A share-count fact as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct ShareFact {
    pub filing_id: String,
    pub element: String,
    pub kind: Kind,
    /// Instant date, or the duration's end.
    pub end: NaiveDate,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManualShares {
    pub as_of: NaiveDate,
    pub shares: f64,
    pub source_note: String,
}

/// Net income and basic EPS of one filing period, dimension-free, same context.
#[derive(Clone, Debug, PartialEq)]
pub struct EpsPair {
    pub filing_id: String,
    pub period_end: NaiveDate,
    pub numerator_element: String,
    pub net_income: f64,
    pub eps_element: String,
    pub eps: f64,
}

/// An EPS-implied count: `net_income / eps` for one filing period (D13 option c).
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedShares {
    pub filing_id: String,
    pub period_end: NaiveDate,
    pub numerator_element: String,
    pub eps_element: String,
    pub value: f64,
}

/// Turns the EPS pairs into derived counts: one per `(filing, period end)`, keeping the
/// highest-priority element pair; non-positive or non-finite ratios are dropped.
pub fn implied_from_eps(pairs: &[EpsPair]) -> Vec<DerivedShares> {
    let mut out: Vec<DerivedShares> = Vec::new();
    let mut ranked: Vec<&EpsPair> = pairs.iter().collect();
    ranked.sort_by_key(|p| {
        (
            p.period_end,
            p.filing_id.clone(),
            rank_in(EPS_NUMERATOR_ELEMENTS, &p.numerator_element),
            rank_in(EPS_ELEMENTS, &p.eps_element),
        )
    });
    for p in ranked {
        if p.eps == 0.0 || !p.eps.is_finite() || !p.net_income.is_finite() {
            continue;
        }
        let value = p.net_income / p.eps;
        if !(value.is_finite() && value > 0.0) {
            continue;
        }
        if out
            .last()
            .is_some_and(|d| d.filing_id == p.filing_id && d.period_end == p.period_end)
        {
            continue;
        }
        out.push(DerivedShares {
            filing_id: p.filing_id.clone(),
            period_end: p.period_end,
            numerator_element: p.numerator_element.clone(),
            eps_element: p.eps_element.clone(),
            value,
        });
    }
    out
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// The filing's own figure for the period ending on the date.
    Filing {
        filing_id: String,
        element: String,
        period_end: NaiveDate,
    },
    /// The latest instant on or before the date, from an earlier period.
    LatestInstant {
        filing_id: String,
        element: String,
        as_of: NaiveDate,
    },
    /// The owner's entry; `after_date` says it was entered for a later date.
    Manual {
        as_of: NaiveDate,
        source_note: String,
        after_date: bool,
    },
    /// Net income ÷ basic EPS of the latest filing period ending on or before the date: a
    /// derivation (the weighted average count behind the EPS), never a reported count.
    Derived {
        filing_id: String,
        period_end: NaiveDate,
        numerator_element: String,
        eps_element: String,
    },
}

impl Source {
    /// Short Indonesian label for the UI badge (design.md §3.6 vocabulary).
    pub fn label(&self) -> &'static str {
        match self {
            Self::Filing { .. } => "XBRL",
            Self::LatestInstant { .. } => "XBRL (instan terakhir)",
            Self::Manual { .. } => "manual",
            Self::Derived { .. } => "turunan (laba ÷ LPS)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resolved {
    pub shares: f64,
    pub source: Source,
}

fn best<'a>(candidates: impl Iterator<Item = &'a ShareFact>) -> Option<&'a ShareFact> {
    // Latest date first, then instants before durations, then element priority.
    candidates.min_by_key(|f| {
        (
            std::cmp::Reverse(f.end),
            f.kind == Kind::Duration,
            element_rank(&f.element),
            f.filing_id.clone(),
        )
    })
}

/// Resolves the share count in force on `as_of`.
pub fn resolve(
    as_of: NaiveDate,
    facts: &[ShareFact],
    manual: &[ManualShares],
    derived: &[DerivedShares],
) -> Option<Resolved> {
    let valid = |f: &&ShareFact| f.value > 0.0 && f.value.is_finite();
    if let Some(f) = best(facts.iter().filter(valid).filter(|f| f.end == as_of)) {
        return Some(Resolved {
            shares: f.value,
            source: Source::Filing {
                filing_id: f.filing_id.clone(),
                element: f.element.clone(),
                period_end: f.end,
            },
        });
    }
    let manual_valid = |m: &&ManualShares| m.shares > 0.0 && m.shares.is_finite();
    let before = manual
        .iter()
        .filter(manual_valid)
        .filter(|m| m.as_of <= as_of)
        .max_by_key(|m| m.as_of);
    let instant = best(
        facts
            .iter()
            .filter(valid)
            .filter(|f| f.kind == Kind::Instant && f.end < as_of),
    );
    let implied = derived
        .iter()
        .filter(|d| d.value > 0.0 && d.value.is_finite() && d.period_end <= as_of)
        .max_by_key(|d| (d.period_end, std::cmp::Reverse(d.filing_id.clone())));
    // Between a filing instant and the owner's entry the most recent date on or before
    // `as_of` wins (a rights issue the owner recorded after the last filing instant); on
    // the same date the filing wins. The EPS derivation is a fallback only: it never
    // overrides a reported or entered count, however old, because it is a weighted
    // average behind a ratio, not a count.
    if let Some(f) = instant.filter(|f| before.is_none_or(|m| m.as_of <= f.end)) {
        return Some(Resolved {
            shares: f.value,
            source: Source::LatestInstant {
                filing_id: f.filing_id.clone(),
                element: f.element.clone(),
                as_of: f.end,
            },
        });
    }
    if let Some(m) = before {
        return Some(Resolved {
            shares: m.shares,
            source: Source::Manual {
                as_of: m.as_of,
                source_note: m.source_note.clone(),
                after_date: false,
            },
        });
    }
    if let Some(d) = implied {
        return Some(Resolved {
            shares: d.value,
            source: Source::Derived {
                filing_id: d.filing_id.clone(),
                period_end: d.period_end,
                numerator_element: d.numerator_element.clone(),
                eps_element: d.eps_element.clone(),
            },
        });
    }
    let after = manual
        .iter()
        .filter(manual_valid)
        .filter(|m| m.as_of > as_of)
        .min_by_key(|m| m.as_of);
    after.map(|m| Resolved {
        shares: m.shares,
        source: Source::Manual {
            as_of: m.as_of,
            source_note: m.source_note.clone(),
            after_date: true,
        },
    })
}

/// One ticker's share inputs, resolved per date for the valuation history (R3.2).
#[derive(Clone, Debug, Default)]
pub struct ShareSeries {
    pub facts: Vec<ShareFact>,
    pub manual: Vec<ManualShares>,
    pub derived: Vec<DerivedShares>,
}

impl ShareSeries {
    pub fn at(&self, date: NaiveDate) -> Option<Resolved> {
        resolve(date, &self.facts, &self.manual, &self.derived)
    }

    /// Dates on which the resolved count may change, ascending: every fact, manual and
    /// derived date. A daily history only needs re-resolution at these points.
    pub fn breakpoints(&self) -> Vec<NaiveDate> {
        let mut d: Vec<NaiveDate> = self
            .facts
            .iter()
            .map(|f| f.end)
            .chain(self.manual.iter().map(|m| m.as_of))
            .chain(self.derived.iter().map(|x| x.period_end))
            .collect();
        d.sort_unstable();
        d.dedup();
        d
    }

    /// `(shares, source)` for every date in `dates` (ascending), resolving once per
    /// breakpoint interval rather than once per day.
    pub fn over(&self, dates: &[NaiveDate]) -> Vec<Option<Resolved>> {
        let points = self.breakpoints();
        let mut out = Vec::with_capacity(dates.len());
        let mut current: Option<((usize, bool), Option<Resolved>)> = None;
        for &date in dates {
            // Interval index: number of breakpoints at or before the date. The breakpoint
            // day itself resolves differently from the days after it (a fact ending on the
            // date is the period's own figure), so it gets its own cache key.
            let idx = points.partition_point(|p| *p <= date);
            let key = (idx, idx > 0 && points[idx - 1] == date);
            let reuse = current.as_ref().is_some_and(|(k, _)| *k == key);
            if !reuse {
                current = Some((key, self.at(date)));
            }
            out.push(current.as_ref().and_then(|(_, r)| r.clone()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn fact(filing: &str, element: &str, kind: Kind, end: &str, value: f64) -> ShareFact {
        ShareFact {
            filing_id: filing.into(),
            element: element.into(),
            kind,
            end: d(end),
            value,
        }
    }

    fn manual(as_of: &str, shares: f64, note: &str) -> ManualShares {
        ManualShares {
            as_of: d(as_of),
            shares,
            source_note: note.into(),
        }
    }

    fn pair(filing: &str, end: &str, ni: f64, eps: f64) -> EpsPair {
        EpsPair {
            filing_id: filing.into(),
            period_end: d(end),
            numerator_element: "idx-cor:ProfitLoss".into(),
            net_income: ni,
            eps_element: "idx-cor:BasicEarningsLossPerShare".into(),
            eps,
        }
    }

    #[test]
    fn the_periods_own_tag_wins_and_instants_beat_averages() {
        let facts = vec![
            fact(
                "AAAA_2025_Q2",
                "ifrs-full:WeightedAverageNumberOfOrdinarySharesOutstanding",
                Kind::Duration,
                "2025-06-30",
                990.0,
            ),
            fact(
                "AAAA_2025_Q2",
                "idx-cor:NumberOfSharesOutstanding",
                Kind::Instant,
                "2025-06-30",
                1000.0,
            ),
            fact(
                "AAAA_2025_Q1",
                "idx-cor:NumberOfSharesOutstanding",
                Kind::Instant,
                "2025-03-31",
                900.0,
            ),
        ];
        let r = resolve(
            d("2025-06-30"),
            &facts,
            &[manual("2025-01-01", 1.0, "x")],
            &[],
        )
        .unwrap();
        assert_eq!(r.shares, 1000.0);
        assert_eq!(
            r.source,
            Source::Filing {
                filing_id: "AAAA_2025_Q2".into(),
                element: "idx-cor:NumberOfSharesOutstanding".into(),
                period_end: d("2025-06-30"),
            }
        );
    }

    #[test]
    fn falls_back_to_the_latest_instant_before_the_date() {
        let facts = vec![
            fact(
                "AAAA_2025_Q1",
                "idx-cor:NumberOfSharesOutstanding",
                Kind::Instant,
                "2025-03-31",
                900.0,
            ),
            fact(
                "AAAA_2024_FY",
                "idx-cor:NumberOfSharesOutstanding",
                Kind::Instant,
                "2024-12-31",
                800.0,
            ),
            fact(
                "AAAA_2025_Q3",
                "idx-cor:NumberOfSharesOutstanding",
                Kind::Instant,
                "2025-09-30",
                1100.0,
            ),
            fact(
                "AAAA_2025_Q2",
                "ifrs-full:WeightedAverageNumberOfOrdinarySharesOutstanding",
                Kind::Duration,
                "2025-06-30",
                950.0,
            ),
        ];
        let r = resolve(d("2025-08-15"), &facts, &[], &[]).unwrap();
        assert_eq!(
            r.shares, 900.0,
            "a duration ending earlier is not an instant"
        );
        assert_eq!(
            r.source,
            Source::LatestInstant {
                filing_id: "AAAA_2025_Q1".into(),
                element: "idx-cor:NumberOfSharesOutstanding".into(),
                as_of: d("2025-03-31"),
            }
        );
    }

    #[test]
    fn manual_is_the_last_resort_and_says_when_it_is_from_a_later_date() {
        let manual = vec![
            manual("2025-01-15", 500.0, "prospektus"),
            manual("2025-09-01", 520.0, "keterbukaan informasi"),
        ];
        let r = resolve(d("2025-06-30"), &[], &manual, &[]).unwrap();
        assert_eq!(r.shares, 500.0);
        assert_eq!(
            r.source,
            Source::Manual {
                as_of: d("2025-01-15"),
                source_note: "prospektus".into(),
                after_date: false,
            }
        );
        let r = resolve(d("2024-06-30"), &[], &manual, &[]).unwrap();
        assert_eq!(r.shares, 500.0);
        assert!(matches!(
            r.source,
            Source::Manual {
                after_date: true,
                ..
            }
        ));
        assert_eq!(resolve(d("2024-06-30"), &[], &[], &[]), None);
        // A zero or negative count is never used.
        let bad = vec![fact(
            "AAAA_2025_Q2",
            "idx-cor:NumberOfSharesOutstanding",
            Kind::Instant,
            "2025-06-30",
            0.0,
        )];
        assert_eq!(resolve(d("2025-06-30"), &bad, &[], &[]), None);
    }

    #[test]
    fn share_count_facts_are_recognised_by_unit_or_element() {
        assert!(is_share_count("idx-cor:Whatever", "shares"));
        assert!(is_share_count("idx-cor:NumberOfSharesOutstanding", "pure"));
        assert!(!is_share_count(
            "idx-cor:BasicEarningsLossPerShare",
            "IDR/shares"
        ));
        assert!(!is_share_count("idx-cor:Cash", "IDR"));
        assert!(!is_share_count("idx-cor:TreasuryShares", "shares"));
        assert!(!is_share_count("idx-cor:AuthorisedShares", "shares"));
    }

    #[test]
    fn a_manual_entry_dated_after_the_last_instant_wins() {
        let facts = vec![fact(
            "AAAA_2024_FY",
            "idx-cor:NumberOfSharesOutstanding",
            Kind::Instant,
            "2024-12-31",
            800.0,
        )];
        let manual = vec![manual("2025-03-01", 1000.0, "rights issue")];
        let r = resolve(d("2025-06-30"), &facts, &manual, &[]).unwrap();
        assert_eq!(r.shares, 1000.0);
        assert!(matches!(
            r.source,
            Source::Manual {
                after_date: false,
                ..
            }
        ));
        // Before the manual entry's date the filing instant still applies.
        let r = resolve(d("2025-02-01"), &facts, &manual, &[]).unwrap();
        assert_eq!(r.shares, 800.0);
        assert!(matches!(r.source, Source::LatestInstant { .. }));
    }

    #[test]
    fn eps_implied_counts_are_a_labelled_derivation_between_manual_before_and_after() {
        let derived = implied_from_eps(&[
            pair("AAAA_2024_FY", "2024-12-31", 1_000_000.0, 100.0),
            pair("AAAA_2025_Q2", "2025-06-30", 550_000.0, 50.0),
            pair("AAAA_2025_Q3", "2025-09-30", 0.0, 0.0),
            pair("AAAA_2023_FY", "2023-12-31", -300_000.0, 30.0),
        ]);
        assert_eq!(
            derived.len(),
            2,
            "zero EPS and a negative ratio are dropped"
        );
        assert_eq!(derived[0].value, 10_000.0);
        assert_eq!(derived[1].value, 11_000.0);

        // No manual entry: the latest period ending on or before the date.
        let r = resolve(d("2025-08-15"), &[], &[], &derived).unwrap();
        assert_eq!(r.shares, 11_000.0);
        assert_eq!(
            r.source,
            Source::Derived {
                filing_id: "AAAA_2025_Q2".into(),
                period_end: d("2025-06-30"),
                numerator_element: "idx-cor:ProfitLoss".into(),
                eps_element: "idx-cor:BasicEarningsLossPerShare".into(),
            }
        );
        assert_eq!(r.source.label(), "turunan (laba ÷ LPS)");
        // Before any period: nothing, unless the owner entered a later count.
        assert_eq!(resolve(d("2024-06-30"), &[], &[], &derived), None);
        let later = vec![manual("2026-01-10", 12_000.0, "keterbukaan")];
        let r = resolve(d("2024-06-30"), &[], &later, &derived).unwrap();
        assert!(matches!(
            r.source,
            Source::Manual {
                after_date: true,
                ..
            }
        ));
        // Once the owner's entry is in force a newer derivation does not override it.
        let r = resolve(d("2026-03-01"), &[], &later, &derived).unwrap();
        assert_eq!(r.shares, 12_000.0);
        let older = vec![manual("2025-01-10", 8_000.0, "prospektus")];
        let r = resolve(d("2025-08-15"), &[], &older, &derived).unwrap();
        assert_eq!(
            r.shares, 8_000.0,
            "a derivation is a fallback, never an override"
        );
        // Same date: the owner's entry beats the derivation.
        let same = vec![manual("2025-06-30", 9_000.0, "RUPS")];
        let r = resolve(d("2025-07-01"), &[], &same, &derived).unwrap();
        assert_eq!(r.shares, 9_000.0);
    }

    #[test]
    fn eps_pairs_prefer_the_parent_attributable_numerator() {
        let mut other = pair("AAAA_2025_Q2", "2025-06-30", 400_000.0, 50.0);
        other.numerator_element = "idx-cor:ProfitLossAttributableToParentEntity".into();
        let derived =
            implied_from_eps(&[pair("AAAA_2025_Q2", "2025-06-30", 550_000.0, 50.0), other]);
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].value, 8_000.0);
        assert_eq!(
            derived[0].numerator_element,
            "idx-cor:ProfitLossAttributableToParentEntity"
        );
    }

    #[test]
    fn a_series_resolves_per_breakpoint_interval() {
        let series = ShareSeries {
            facts: vec![],
            manual: vec![manual("2025-03-01", 1000.0, "rights issue")],
            derived: implied_from_eps(&[pair("AAAA_2024_FY", "2024-12-31", 900_000.0, 1000.0)]),
        };
        assert_eq!(series.breakpoints(), vec![d("2024-12-31"), d("2025-03-01")]);
        let dates = [
            d("2024-12-30"),
            d("2024-12-31"),
            d("2025-02-28"),
            d("2025-03-01"),
            d("2025-06-30"),
        ];
        let out = series.over(&dates);
        let shares: Vec<Option<f64>> = out.iter().map(|r| r.as_ref().map(|r| r.shares)).collect();
        assert_eq!(
            shares,
            vec![
                Some(1000.0),
                Some(900.0),
                Some(900.0),
                Some(1000.0),
                Some(1000.0)
            ]
        );
        assert!(matches!(
            out[0].as_ref().unwrap().source,
            Source::Manual {
                after_date: true,
                ..
            }
        ));
        assert!(matches!(
            out[1].as_ref().unwrap().source,
            Source::Derived { .. }
        ));
        for (i, date) in dates.iter().enumerate() {
            assert_eq!(out[i], series.at(*date), "{date}");
        }
        // A duration fact ending on a breakpoint day is that day's own figure and must not
        // leak into the following days (review R3).
        let with_fact = ShareSeries {
            facts: vec![fact(
                "AAAA_2025_Q2",
                "ifrs-full:WeightedAverageNumberOfOrdinarySharesOutstanding",
                Kind::Duration,
                "2025-06-30",
                950.0,
            )],
            manual: vec![manual("2025-03-01", 1000.0, "rights issue")],
            derived: vec![],
        };
        let dates = [
            d("2025-06-29"),
            d("2025-06-30"),
            d("2025-07-01"),
            d("2025-07-02"),
        ];
        let out = with_fact.over(&dates);
        let shares: Vec<f64> = out.iter().map(|r| r.as_ref().unwrap().shares).collect();
        assert_eq!(shares, vec![1000.0, 950.0, 1000.0, 1000.0]);
        for (i, date) in dates.iter().enumerate() {
            assert_eq!(out[i], with_fact.at(*date), "{date}");
        }
    }
}
