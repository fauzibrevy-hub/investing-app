//! Calibration (research plan §6 R4.6; prd.md A4.4; design.md §4.3): every journal level and
//! decision and every claim with the price move since the day it was written, shown once at
//! least 30 days have passed. It is the owner's track record held up to the owner, not a
//! score of the stock: no ranking, no colour, no threshold on what a good result is.
//!
//! Pure: entries, closes and splits in, rows out. Closes are today's split-adjusted history,
//! so a price stamped before a split is moved onto the same basis first.

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use super::valuation::split_factor;

/// Days an entry must be old before its forward return is shown (A4.4).
pub const MATURITY_DAYS: i64 = 30;

/// Serialized as the Indonesian type word of the table (`klaim`, `level`, `keputusan`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    #[serde(rename = "klaim")]
    Claim,
    #[serde(rename = "level")]
    Level,
    #[serde(rename = "keputusan")]
    Decision,
}

impl Kind {
    /// The Indonesian type word of the filter (`klaim`, `level`, `keputusan`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claim => "klaim",
            Self::Level => "level",
            Self::Decision => "keputusan",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Claim, Self::Level, Self::Decision]
            .into_iter()
            .find(|k| k.as_str() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Claim => "Klaim",
            Self::Level => "Level",
            Self::Decision => "Keputusan",
        }
    }
}

/// Something the owner wrote down that has a price to be measured from.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub ticker: String,
    pub kind: Kind,
    /// The day it was written.
    pub date: NaiveDate,
    pub text: String,
    /// The price stamped when it was saved (journal), or `None` for a claim, which is
    /// measured from the close of its day.
    pub price_then: Option<f64>,
    /// The line of a `level` entry.
    pub level: Option<f64>,
    /// The Indonesian status word of a claim.
    pub status: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Row {
    pub id: String,
    pub ticker: String,
    pub kind: Kind,
    pub kind_label: &'static str,
    pub date: NaiveDate,
    pub text: String,
    /// On today's split-adjusted basis.
    pub price_then: Option<f64>,
    pub price_now: Option<f64>,
    pub price_now_date: Option<NaiveDate>,
    pub days: i64,
    /// At least `MATURITY_DAYS` old: only then is a return shown.
    pub matured: bool,
    pub matures_on: NaiveDate,
    /// `(now ÷ then − 1) × 100`, only for a matured row with both prices.
    pub return_pct: Option<f64>,
    pub level: Option<f64>,
    /// A level that a close has reached since it was written, from the side the price was on.
    pub reached: Option<bool>,
    /// A claim's status word, or a level's `Tercapai` / `Belum tercapai`.
    pub status: Option<&'static str>,
    /// Why there is no return, when there is not.
    pub reason: Option<&'static str>,
}

/// Latest close on or before `on` of an ascending series.
fn close_on(series: &[(NaiveDate, f64)], on: NaiveDate) -> Option<f64> {
    let i = series.partition_point(|(d, _)| *d <= on);
    (i > 0).then(|| series[i - 1].1)
}

/// One calibration row. `series` is the ticker's ascending closes up to `today`; `splits`
/// are `(date, ratio)` pairs of the ticker.
pub fn row(
    e: &Entry,
    series: &[(NaiveDate, f64)],
    splits: &[(NaiveDate, f64)],
    today: NaiveDate,
) -> Row {
    let days = (today - e.date).num_days();
    let matured = days >= MATURITY_DAYS;
    let basis = split_factor(splits, e.date);
    // A journal stamp was written on the basis of its day and moves with the splits since;
    // a claim reads the stored closes, which already are on today's basis.
    let then = match e.price_then {
        Some(p) if basis > 0.0 => Some(p / basis),
        Some(_) => None,
        None => close_on(series, e.date),
    }
    .filter(|p| p.is_finite() && *p > 0.0);
    let now = series.last().copied();
    let level = e.level.map(|l| l / basis);
    let reached = match (e.kind, level, then) {
        (Kind::Level, Some(l), Some(t)) => {
            Some(series.iter().filter(|(d, _)| *d > e.date).any(|(_, c)| {
                if t < l {
                    *c >= l
                } else {
                    *c <= l
                }
            }))
        }
        _ => None,
    };
    let return_pct = match (matured, then, now) {
        (true, Some(t), Some((_, n))) => Some((n / t - 1.0) * 100.0),
        _ => None,
    };
    let reason = if return_pct.is_some() {
        None
    } else if !matured {
        Some("belum 30 hari")
    } else if then.is_none() {
        Some("harga saat itu tidak tersimpan")
    } else {
        Some("harga kini tidak tersedia")
    };
    Row {
        id: e.id.clone(),
        ticker: e.ticker.clone(),
        kind: e.kind,
        kind_label: e.kind.label(),
        date: e.date,
        text: e.text.clone(),
        price_then: then,
        price_now: now.map(|(_, c)| c),
        price_now_date: now.map(|(d, _)| d),
        days,
        matured,
        matures_on: e.date + Duration::days(MATURITY_DAYS),
        return_pct,
        level,
        reached,
        status: match e.kind {
            Kind::Claim => e.status,
            Kind::Level => reached.map(|r| if r { "Tercapai" } else { "Belum tercapai" }),
            Kind::Decision => None,
        },
        reason,
    }
}

/// Newest first, then ticker, then id.
pub fn sorted(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| a.ticker.cmp(&b.ticker))
            .then_with(|| a.id.cmp(&b.id))
    });
    rows
}

/// The footer line: claims that held against claims that broke, per ticker. Claims not yet
/// decided (not testable, expired) are counted apart and stay out of the rate.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HitRate {
    pub ticker: String,
    pub holding: usize,
    pub broken: usize,
    pub undecided: usize,
    /// `holding ÷ (holding + broken) × 100`; `None` while nothing is decided.
    pub rate_pct: Option<f64>,
    pub text: String,
}

/// `statuses` is one `(ticker, status)` per claim (`holding`, `broken`, anything else is
/// undecided).
pub fn hit_rates(statuses: &[(String, String)]) -> Vec<HitRate> {
    let mut by: std::collections::BTreeMap<&str, (usize, usize, usize)> = Default::default();
    for (t, s) in statuses {
        let c = by.entry(t).or_default();
        match s.as_str() {
            "holding" => c.0 += 1,
            "broken" => c.1 += 1,
            _ => c.2 += 1,
        }
    }
    by.into_iter()
        .map(|(t, (holding, broken, undecided))| {
            let decided = holding + broken;
            let rate_pct = (decided > 0).then(|| holding as f64 / decided as f64 * 100.0);
            let text = match rate_pct {
                Some(r) => format!("{t}: {holding} Berlaku, {broken} Patah ({r:.0}%)"),
                None => format!("{t}: belum ada klaim yang teruji"),
            };
            HitRate {
                ticker: t.into(),
                holding,
                broken,
                undecided,
                rate_pct,
                text,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// One close a day from 2026-01-01 to 2026-04-30, rising by 1 from 100.
    fn series() -> Vec<(NaiveDate, f64)> {
        let mut out = Vec::new();
        let mut day = d("2026-01-01");
        let mut c = 100.0;
        while day <= d("2026-04-30") {
            out.push((day, c));
            c += 1.0;
            day = day.succ_opt().unwrap();
        }
        out
    }

    fn entry(kind: Kind, date: &str, price: Option<f64>) -> Entry {
        Entry {
            id: "x".into(),
            ticker: "COAL".into(),
            kind,
            date: d(date),
            text: "teks".into(),
            price_then: price,
            level: None,
            status: None,
        }
    }

    #[test]
    fn no_return_before_thirty_days_and_one_on_the_thirtieth() {
        let s = series();
        let today = d("2026-04-30");
        // Written 2026-04-01: 29 days; 2026-03-31: 30 days.
        let young = row(
            &entry(Kind::Decision, "2026-04-01", Some(190.0)),
            &s,
            &[],
            today,
        );
        assert_eq!((young.days, young.matured), (29, false));
        assert_eq!(young.return_pct, None);
        assert_eq!(young.reason, Some("belum 30 hari"));
        assert_eq!(young.matures_on, d("2026-05-01"));
        let ripe = row(
            &entry(Kind::Decision, "2026-03-31", Some(190.0)),
            &s,
            &[],
            today,
        );
        assert_eq!((ripe.days, ripe.matured), (30, true));
        // 2026-04-30 is day 120 of the series: close 219.
        assert_eq!(ripe.price_now, Some(219.0));
        assert_eq!(ripe.price_now_date, Some(today));
        let r = ripe.return_pct.unwrap();
        assert!((r - (219.0 / 190.0 - 1.0) * 100.0).abs() < 1e-9);
        assert_eq!(ripe.reason, None);
        assert_eq!(ripe.kind_label, "Keputusan");
        assert_eq!(ripe.status, None);
    }

    #[test]
    fn a_claim_is_measured_from_the_close_of_its_day() {
        let s = series();
        let mut e = entry(Kind::Claim, "2026-02-10", None);
        e.status = Some("Berlaku");
        let r = row(&e, &s, &[], d("2026-04-30"));
        // 2026-02-10 is day 41 of the series: close 100 + 40.
        assert_eq!(r.price_then, Some(140.0));
        assert!((r.return_pct.unwrap() - (219.0 / 140.0 - 1.0) * 100.0).abs() < 1e-9);
        assert_eq!(r.status, Some("Berlaku"));
        // A claim older than the price history cannot be measured, and says why.
        let early = row(
            &entry(Kind::Claim, "2025-12-01", None),
            &s,
            &[],
            d("2026-04-30"),
        );
        assert_eq!(
            (early.return_pct, early.reason),
            (None, Some("harga saat itu tidak tersimpan"))
        );
    }

    #[test]
    fn a_missing_stamp_or_missing_current_price_leaves_no_return_and_a_reason() {
        let s = series();
        let today = d("2026-04-30");
        let none = row(
            &entry(Kind::Decision, "2026-02-01", None),
            &s[..0],
            &[],
            today,
        );
        assert_eq!(none.reason, Some("harga saat itu tidak tersimpan"));
        let no_now = row(
            &entry(Kind::Decision, "2026-02-01", Some(120.0)),
            &[],
            &[],
            today,
        );
        assert_eq!(no_now.reason, Some("harga kini tidak tersedia"));
        let zero = row(
            &entry(Kind::Decision, "2026-02-01", Some(0.0)),
            &s,
            &[],
            today,
        );
        assert_eq!(zero.return_pct, None);
    }

    #[test]
    fn a_price_stamped_before_a_split_is_moved_onto_the_adjusted_basis() {
        let s = series();
        let today = d("2026-04-30");
        // A 2-for-1 split after the entry: the stamp of 200 is 100 on today's basis.
        let splits = [(d("2026-03-01"), 2.0)];
        let r = row(
            &entry(Kind::Decision, "2026-02-01", Some(200.0)),
            &s,
            &splits,
            today,
        );
        assert_eq!(r.price_then, Some(100.0));
        assert!((r.return_pct.unwrap() - 119.0).abs() < 1e-9);
        // A split before the entry does not touch it.
        let before = row(
            &entry(Kind::Decision, "2026-02-01", Some(200.0)),
            &s,
            &[(d("2026-01-15"), 2.0)],
            today,
        );
        assert_eq!(before.price_then, Some(200.0));
    }

    #[test]
    fn a_claim_reads_closes_that_are_already_split_adjusted() {
        let s = series();
        // The stored closes are already on today's basis, so a split after the claim must
        // not be applied to its price a second time.
        let splits = [(d("2026-03-01"), 2.0)];
        let r = row(
            &entry(Kind::Claim, "2026-02-10", None),
            &s,
            &splits,
            d("2026-04-30"),
        );
        assert_eq!(r.price_then, Some(140.0));
        assert!((r.return_pct.unwrap() - (219.0 / 140.0 - 1.0) * 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_level_is_reached_from_the_side_the_price_was_on() {
        let s = series();
        let today = d("2026-04-30");
        // Entered at 120 with the line at 150: reached once a close is at or above 150.
        let mut e = entry(Kind::Level, "2026-01-21", Some(120.0));
        e.level = Some(150.0);
        let r = row(&e, &s, &[], today);
        assert_eq!(r.reached, Some(true));
        assert_eq!(r.status, Some("Tercapai"));
        // A line above anything the series reached.
        e.level = Some(500.0);
        let r = row(&e, &s, &[], today);
        assert_eq!((r.reached, r.status), (Some(false), Some("Belum tercapai")));
        // A line below the entry price is reached by a close at or under it: none here.
        e.level = Some(110.0);
        assert_eq!(row(&e, &s, &[], today).reached, Some(false));
        // A close before the entry day does not count.
        e.date = d("2026-04-29");
        e.price_then = Some(218.0);
        assert_eq!(row(&e, &s, &[], today).reached, Some(false));
    }

    #[test]
    fn rows_sort_newest_first() {
        let s = series();
        let mk = |id: &str, date: &str| {
            let mut e = entry(Kind::Decision, date, Some(100.0));
            e.id = id.into();
            row(&e, &s, &[], d("2026-04-30"))
        };
        let rows = sorted(vec![
            mk("a", "2026-02-01"),
            mk("b", "2026-03-01"),
            mk("c", "2026-03-01"),
        ]);
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["b", "c", "a"]);
    }

    #[test]
    fn hit_rate_counts_decided_claims_per_ticker() {
        let statuses: Vec<(String, String)> = [
            ("BANK", "holding"),
            ("BANK", "holding"),
            ("BANK", "broken"),
            ("BANK", "not_testable"),
            ("COAL", "expired"),
        ]
        .iter()
        .map(|(t, s)| ((*t).into(), (*s).into()))
        .collect();
        let rates = hit_rates(&statuses);
        assert_eq!(rates.len(), 2);
        assert_eq!(rates[0].text, "BANK: 2 Berlaku, 1 Patah (67%)");
        assert_eq!(
            (rates[0].holding, rates[0].broken, rates[0].undecided),
            (2, 1, 1)
        );
        assert_eq!(rates[1].rate_pct, None);
        assert_eq!(rates[1].text, "COAL: belum ada klaim yang teruji");
        assert!(hit_rates(&[]).is_empty());
    }

    #[test]
    fn kinds_use_the_filter_words() {
        for k in [Kind::Claim, Kind::Level, Kind::Decision] {
            assert_eq!(Kind::parse(k.as_str()), Some(k));
        }
        assert_eq!(Kind::parse("catatan"), None);
    }
}
