//! Event log, coverage and the Desk (research plan §3.1 `events.rs`).
//!
//! Coverage (R2.4) is what the coverage board and the pack section show (prd.md S3, S4;
//! design.md §4.1 block 3): the share of XBRL pack metrics populated for the latest quarter,
//! and the KPI quarters still missing. The inbox (R4.3; prd.md A4.3) lists what needs the
//! owner: a new filing, a claim that changed status, KPIs that came due, a journal level a
//! close crossed, a restatement, stale data, a hard accounting flag. Each kind has a pure
//! producer in `producers` (inputs in, `NewEvent`s out; ids are deterministic, so running a
//! producer twice stores one event) and the store-facing pass in `react`.
//!
//! This file is pure: kinds, wording and coverage; nothing here reads SQLite.

pub mod producers;
pub mod react;
pub mod text_fmt;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::packs::{Cadence, Quarter};
use super::store::EventRow;
use text_fmt::{days, period, plain, readable, scaled, UNREADABLE};

/// Inbox kinds (prd.md A4.3). The Indonesian wording is fact-only: numbers and dates, never
/// a judgement of them (D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    FilingNew,
    ClaimStatus,
    KpiDue,
    LevelCrossed,
    Restatement,
    StaleData,
    AccountingFlag,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Self::FilingNew,
        Self::ClaimStatus,
        Self::KpiDue,
        Self::LevelCrossed,
        Self::Restatement,
        Self::StaleData,
        Self::AccountingFlag,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FilingNew => "filing_new",
            Self::ClaimStatus => "claim_status",
            Self::KpiDue => "kpi_due",
            Self::LevelCrossed => "level_crossed",
            Self::Restatement => "restatement",
            Self::StaleData => "stale_data",
            Self::AccountingFlag => "accounting_flag",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// The filter chip the item sits under (design.md §4.1 block 2).
    pub fn group(self) -> &'static str {
        match self {
            Self::FilingNew => "laporan",
            Self::ClaimStatus => "klaim",
            Self::KpiDue => "kpi",
            Self::LevelCrossed => "level",
            Self::Restatement | Self::StaleData => "data",
            Self::AccountingFlag => "bendera",
        }
    }

    /// The ticker page section **Buka** opens: `overview`, `pack`, `financials`,
    /// `valuation`, `price`, `sources`, or `claims` for the right pane (A4.3).
    pub fn section(self, payload: &Value) -> &'static str {
        match self {
            Self::FilingNew | Self::Restatement | Self::AccountingFlag => "financials",
            Self::ClaimStatus => "claims",
            Self::KpiDue => "pack",
            Self::LevelCrossed => "price",
            Self::StaleData => match payload["source"].as_str() {
                Some("filing") => "sources",
                _ => "price",
            },
        }
    }
}

/// Filter chips of the inbox, `(group, Indonesian label)`.
pub const FILTERS: [(&str, &str); 7] = [
    ("semua", "Semua"),
    ("laporan", "Laporan baru"),
    ("klaim", "Klaim"),
    ("kpi", "KPI"),
    ("level", "Level"),
    ("bendera", "Bendera"),
    ("data", "Data"),
];

/// An event a producer wants stored.
#[derive(Clone, Debug, PartialEq)]
pub struct NewEvent {
    pub id: String,
    /// RFC 3339.
    pub at: String,
    pub ticker: Option<String>,
    pub kind: Kind,
    pub payload: Value,
}

/// `<kind>-<16 hex>` of the producer's dedupe key: the same fact always gets the same id.
pub fn event_id(kind: Kind, key: &str) -> String {
    let digest = hex::encode(Sha256::digest(
        format!("{}\n{key}", kind.as_str()).as_bytes(),
    ));
    format!("{}-{}", kind.as_str(), &digest[..16])
}

impl NewEvent {
    pub fn new(kind: Kind, key: &str, at: &str, ticker: Option<&str>, payload: Value) -> Self {
        Self {
            id: event_id(kind, key),
            at: at.into(),
            ticker: ticker.map(str::to_string),
            kind,
            payload,
        }
    }

    pub fn row(&self) -> EventRow {
        EventRow {
            id: self.id.clone(),
            at: self.at.clone(),
            ticker: self.ticker.clone(),
            kind: self.kind.as_str().into(),
            payload: self.payload.to_string(),
            dismissed_at: None,
        }
    }
}

fn list_text(items: &[Value]) -> String {
    let names: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
    match names.len() {
        0..=3 => names.join(", "),
        n => format!("{} dan {} lagi", names[..3].join(", "), n - 3),
    }
}

/// An amount in the filing's currency as the Desk says it: `−8 juta rupiah` for IDR (also when
/// the payload predates the currency field), `−US$1,19 juta` for USD, `−EUR 1,19 juta` for
/// others; or the unreadable phrase.
fn money(v: &Value, currency: &Value) -> String {
    let Some(n) = readable(v) else {
        return UNREADABLE.to_string();
    };
    match currency.as_str().map(str::to_ascii_uppercase).as_deref() {
        None | Some("IDR") => format!("{} rupiah", scaled(n)),
        Some(code) => {
            let symbol = if code == "USD" {
                "US$".to_string()
            } else {
                format!("{code} ")
            };
            let sign = if n < 0.0 { "\u{2212}" } else { "" };
            format!("{sign}{symbol}{}", scaled(n.abs()))
        }
    }
}

/// The one-line inbox wording of an event. Numbers and periods follow `text_fmt`
/// (design.md §7); a value the payload does not hold is said as "nilai tidak terbaca".
pub fn text(kind: Kind, p: &Value) -> String {
    let s = |k: &str| p[k].as_str().unwrap_or("?").to_string();
    let per = |k: &str| period(p[k].as_str().unwrap_or("?"));
    match kind {
        Kind::FilingNew => format!(
            "Laporan {} masuk ({})",
            per("label"),
            if p["audited"].as_bool() == Some(true) {
                "diaudit"
            } else {
                "belum diaudit"
            }
        ),
        Kind::ClaimStatus => {
            let value = match (&p["value"], readable(&p["value"])) {
                (_, Some(v)) => format!(" (nilai {})", plain(v)),
                (Value::Null, None) => String::new(),
                _ => format!(" ({UNREADABLE})"),
            };
            format!(
                "Klaim \"{}\": {} → {}{}",
                s("statement"),
                s("from_label"),
                s("to_label"),
                value
            )
        }
        Kind::KpiDue => format!(
            "KPI {} belum diisi: {}",
            per("label"),
            list_text(p["missing"].as_array().map_or(&[][..], Vec::as_slice))
        ),
        Kind::LevelCrossed => {
            let way = match p["direction"].as_str() {
                Some("up") => "naik melewati",
                Some("down") => "turun melewati",
                _ => "melewati",
            };
            let note = p["text"]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(|t| format!(" (\"{t}\")"))
                .unwrap_or_default();
            match (readable(&p["to"]), readable(&p["level"])) {
                (Some(to), Some(level)) => {
                    format!("Harga {} {way} level {}{note}", plain(to), plain(level))
                }
                _ => format!("Harga {way} level ({UNREADABLE}){note}"),
            }
        }
        Kind::Restatement => {
            let moved = match (readable(&p["original"]), readable(&p["restated"])) {
                (Some(a), Some(b)) => format!(": {} → {}", scaled(a), scaled(b)),
                _ => format!(" ({UNREADABLE})"),
            };
            format!(
                "{} {} disajikan kembali{moved}",
                s("element"),
                s("period_end")
            )
        }
        Kind::StaleData => match p["source"].as_str() {
            Some("filing") => format!(
                "Laporan {} belum masuk; batas {}, lewat {} hari",
                per("quarter"),
                s("deadline"),
                days(&p["age_days"])
            ),
            _ => format!(
                "Harga terakhir {}, {} hari lalu",
                s("last"),
                days(&p["age_days"])
            ),
        },
        Kind::AccountingFlag => match p["flag"].as_str() {
            Some("negative_equity") => {
                format!(
                    "Ekuitas negatif pada {}: {}",
                    per("quarter"),
                    money(&p["value"], &p["currency"])
                )
            }
            Some("two_loss_quarters") => {
                let q = p["quarters"].as_array().map_or(&[][..], Vec::as_slice);
                let parts: Vec<String> = q
                    .iter()
                    .map(|x| {
                        format!(
                            "{} {}",
                            period(x["quarter"].as_str().unwrap_or("?")),
                            money(&x["value"], &p["currency"])
                        )
                    })
                    .collect();
                format!(
                    "Rugi bersih dua kuartal berturut-turut: {}",
                    parts.join(", ")
                )
            }
            _ => "Tanda akuntansi tidak dikenal".to_string(),
        },
    }
}

/// One inbox row as the Desk renders it.
pub fn item_json(row: &EventRow, today: NaiveDate) -> Value {
    let payload: Value = serde_json::from_str(&row.payload).unwrap_or(Value::Null);
    let kind = Kind::parse(&row.kind);
    let age = row
        .at
        .get(..10)
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .map(|d| (today - d).num_days().max(0));
    json!({
        "id": row.id, "at": row.at, "ticker": row.ticker, "kind": row.kind,
        "group": kind.map(Kind::group),
        "section": kind.map(|k| k.section(&payload)),
        "text": kind.map(|k| text(k, &payload)),
        "age_days": age,
        "dismissed_at": row.dismissed_at,
        "payload": payload,
    })
}

/// One KPI metric's expected quarters and what the owner entered for them.
#[derive(Clone, Debug, PartialEq)]
pub struct KpiStatus {
    pub metric: String,
    pub cadence: Cadence,
    /// Quarters the metric should have by now (last four for quarterly, last Q4 for annual).
    pub expected: Vec<Quarter>,
    /// Quarters with a value.
    pub entered: Vec<Quarter>,
    /// Quarters marked "unknown" (counted as answered, listed apart).
    pub unknown: Vec<Quarter>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct XbrlCoverage {
    /// Formula metrics with a value at the latest quarter.
    pub populated: usize,
    pub total: usize,
    /// `populated / total × 100`, `None` without metrics.
    pub pct: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KpiCoverage {
    /// Expected cells answered with a value.
    pub entered: usize,
    /// Expected cells answered "unknown".
    pub unknown: usize,
    pub expected: usize,
    /// `metric fyQq` for every expected cell with nothing entered, oldest first.
    pub missing: Vec<String>,
}

/// Data state words of design.md §3.6. `Basi` comes from ages (the Desk, R4.3), not from
/// coverage: `coverage` only returns the first two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataState {
    Lengkap,
    Sebagian,
    Basi,
}

impl DataState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lengkap => "Lengkap",
            Self::Sebagian => "Sebagian",
            Self::Basi => "Basi",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Coverage {
    pub has_pack: bool,
    pub latest: Quarter,
    pub xbrl: XbrlCoverage,
    pub kpi: KpiCoverage,
    pub state: DataState,
}

/// Expected quarters of a KPI inside the shown window (ASUMSI R2.4): the last four shown
/// quarters for a quarterly KPI, the last Q4 in the window for an annual one.
pub fn expected_quarters(cadence: Cadence, shown: &[Quarter]) -> Vec<Quarter> {
    match cadence {
        Cadence::Quarterly => shown.iter().rev().take(4).rev().copied().collect(),
        Cadence::Annual => shown
            .iter()
            .rev()
            .find(|q| q.q == 4)
            .map(|q| vec![*q])
            .unwrap_or_default(),
    }
}

/// `xbrl_populated` is one flag per formula metric of the applied packs: a value at the
/// latest quarter. `Lengkap` means every formula metric populated and no KPI cell missing.
pub fn coverage(
    has_pack: bool,
    latest: Quarter,
    xbrl_populated: &[bool],
    kpis: &[KpiStatus],
) -> Coverage {
    let total = xbrl_populated.len();
    let populated = xbrl_populated.iter().filter(|p| **p).count();
    let pct = (total > 0).then(|| populated as f64 / total as f64 * 100.0);
    let mut entered = 0;
    let mut unknown = 0;
    let mut expected = 0;
    let mut missing = Vec::new();
    for k in kpis {
        for q in &k.expected {
            expected += 1;
            if k.entered.contains(q) {
                entered += 1;
            } else if k.unknown.contains(q) {
                unknown += 1;
            } else {
                missing.push(format!("{} {q}", k.metric));
            }
        }
    }
    let state = if populated == total && missing.is_empty() {
        DataState::Lengkap
    } else {
        DataState::Sebagian
    };
    Coverage {
        has_pack,
        latest,
        xbrl: XbrlCoverage {
            populated,
            total,
            pct,
        },
        kpi: KpiCoverage {
            entered,
            unknown,
            expected,
            missing,
        },
        state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qs(from: Quarter, n: u32) -> Vec<Quarter> {
        (0..n).rev().map(|i| from.back(i)).collect()
    }

    #[test]
    fn expected_quarters_follow_cadence() {
        let shown = qs(Quarter::new(2026, 2), 12);
        assert_eq!(
            expected_quarters(Cadence::Quarterly, &shown),
            qs(Quarter::new(2026, 2), 4)
        );
        assert_eq!(
            expected_quarters(Cadence::Annual, &shown),
            vec![Quarter::new(2025, 4)]
        );
        assert!(expected_quarters(Cadence::Annual, &qs(Quarter::new(2026, 2), 2)).is_empty());
    }

    #[test]
    fn coverage_counts_and_lists_missing_cells() {
        let latest = Quarter::new(2026, 2);
        let kpi = KpiStatus {
            metric: "production_mt".into(),
            cadence: Cadence::Quarterly,
            expected: qs(latest, 4),
            entered: vec![Quarter::new(2026, 2), Quarter::new(2025, 4)],
            unknown: vec![Quarter::new(2026, 1)],
        };
        let c = coverage(true, latest, &[true, true, false, true], &[kpi]);
        assert_eq!(
            (c.xbrl.populated, c.xbrl.total, c.xbrl.pct),
            (3, 4, Some(75.0))
        );
        assert_eq!((c.kpi.entered, c.kpi.unknown, c.kpi.expected), (2, 1, 4));
        assert_eq!(c.kpi.missing, ["production_mt 2025Q3"]);
        assert_eq!(c.state, DataState::Sebagian);
        let full = coverage(true, latest, &[true], &[]);
        assert_eq!(full.state, DataState::Lengkap);
        let none = coverage(false, latest, &[], &[]);
        assert_eq!(none.xbrl.pct, None);
        assert_eq!(none.state, DataState::Lengkap);
    }

    #[test]
    fn unreadable_values_are_said_never_shown_as_nan_or_zero() {
        let flag = json!({"flag": "negative_equity", "quarter": "2025Q4", "value": null});
        assert_eq!(
            text(Kind::AccountingFlag, &flag),
            "Ekuitas negatif pada Q4 25: nilai tidak terbaca"
        );
        let restated = json!({"element": "Revenue", "period_end": "2025-03-31", "original": 5.0});
        assert_eq!(
            text(Kind::Restatement, &restated),
            "Revenue 2025-03-31 disajikan kembali (nilai tidak terbaca)"
        );
        let level = json!({"direction": "up", "to": null, "level": 10.0, "text": ""});
        assert_eq!(
            text(Kind::LevelCrossed, &level),
            "Harga naik melewati level (nilai tidak terbaca)"
        );
    }
}
