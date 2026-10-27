//! Decision journal (research plan §3.1 `journal.rs`, D9; prd.md A4.2; design.md §3.5):
//! append-only entries (`catatan`, `level`, `keputusan`, `sistem`), the price stamped from
//! the latest close at save time, annotations kept apart. An entry is never edited or
//! deleted, so calibration (A4.4) can trust what it compares.
//!
//! Pure: validation and level-crossing detection over in-memory inputs. The store and the
//! routes are `store/r4.rs` and `api/r4.rs`; there is no update or delete anywhere.

use chrono::NaiveDate;
use serde::Serialize;

pub const MAX_TEXT_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Catatan,
    Level,
    Keputusan,
    /// Written by the system (fidelity samples, rules changes), never by the API.
    Sistem,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Catatan => "catatan",
            Self::Level => "level",
            Self::Keputusan => "keputusan",
            Self::Sistem => "sistem",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Catatan, Self::Level, Self::Keputusan, Self::Sistem]
            .into_iter()
            .find(|k| k.as_str() == s)
    }
}

/// A validation failure naming the field (HTTP 422).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub kind: &'static str,
    pub message: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewEntry {
    pub kind: Kind,
    pub text: String,
    pub level: Option<f64>,
}

/// Text is trimmed and must hold 1 to 2000 characters.
pub fn check_text(text: &str, field: &'static str) -> Result<String, FieldError> {
    let t = text.trim();
    let n = t.chars().count();
    if n == 0 || n > MAX_TEXT_CHARS {
        return Err(FieldError {
            field,
            kind: "string_length",
            message: "Text must be 1 to 2000 characters",
        });
    }
    Ok(t.to_string())
}

/// An owner-written entry. `sistem` is refused (the system writes those itself); `level`
/// needs a positive price and no other kind takes one.
pub fn check_entry(kind: &str, text: &str, level: Option<f64>) -> Result<NewEntry, FieldError> {
    let kind = match Kind::parse(kind) {
        Some(Kind::Sistem) => {
            return Err(FieldError {
                field: "kind",
                kind: "enum",
                message: "sistem entries are written by the system",
            })
        }
        Some(k) => k,
        None => {
            return Err(FieldError {
                field: "kind",
                kind: "enum",
                message: "Input should be 'catatan', 'level' or 'keputusan'",
            })
        }
    };
    let text = check_text(text, "text")?;
    match (kind, level) {
        (Kind::Level, Some(l)) if l.is_finite() && l > 0.0 => {}
        (Kind::Level, _) => {
            return Err(FieldError {
                field: "level",
                kind: "greater_than",
                message: "A level entry needs a price greater than 0",
            })
        }
        (_, Some(_)) => {
            return Err(FieldError {
                field: "level",
                kind: "value_error",
                message: "A price level only goes with a level entry",
            })
        }
        (_, None) => {}
    }
    Ok(NewEntry { kind, text, level })
}

/// A journal level as the chart and the crossing detector see it.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelMark {
    pub id: String,
    /// On today's split-adjusted basis (the price series the line is drawn on and compared
    /// with).
    pub level: f64,
    /// The price the owner wrote, before any split since.
    pub entered: f64,
    /// The day the level was written (WIB).
    pub at: NaiveDate,
    pub text: String,
}

/// The WIB calendar date of an RFC 3339 timestamp (the exchange's day, not UTC's).
pub fn wib_day(ts: &str) -> Option<NaiveDate> {
    let at = chrono::DateTime::parse_from_rfc3339(ts).ok()?;
    Some(
        (at + chrono::Duration::hours(7))
            .with_timezone(&chrono::Utc)
            .date_naive(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// The close moved from below the level to the level or above it.
    Up,
    /// The close moved from above the level to the level or below it.
    Down,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Crossing {
    pub level_id: String,
    pub level: f64,
    pub direction: Direction,
    pub from: f64,
    pub to: f64,
    /// The close that crossed.
    pub date: NaiveDate,
}

/// Levels crossed between consecutive closes (`closes` ascending). A level only counts for
/// a close dated after the day it was written, a close that merely sits on the level (the
/// earlier close equals it) is not a crossing, and arriving on it is.
pub fn crossings(levels: &[LevelMark], closes: &[(NaiveDate, f64)]) -> Vec<Crossing> {
    let mut out = Vec::new();
    for pair in closes.windows(2) {
        let ((_, c0), (d1, c1)) = (pair[0], pair[1]);
        for l in levels.iter().filter(|l| l.at < d1) {
            let direction = if c0 < l.level && c1 >= l.level {
                Direction::Up
            } else if c0 > l.level && c1 <= l.level {
                Direction::Down
            } else {
                continue;
            };
            out.push(Crossing {
                level_id: l.id.clone(),
                level: l.level,
                direction,
                from: c0,
                to: c1,
                date: d1,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn mark(id: &str, level: f64, at: &str) -> LevelMark {
        LevelMark {
            id: id.into(),
            level,
            entered: level,
            at: d(at),
            text: String::new(),
        }
    }

    #[test]
    fn the_day_of_a_timestamp_is_the_wib_day() {
        assert_eq!(wib_day("2026-10-24T20:00:00+00:00"), Some(d("2026-10-25")));
        assert_eq!(wib_day("2026-10-24T09:00:00+00:00"), Some(d("2026-10-24")));
        assert_eq!(wib_day("kemarin"), None);
    }

    #[test]
    fn kinds_round_trip_and_the_owner_cannot_write_system_entries() {
        for k in [Kind::Catatan, Kind::Level, Kind::Keputusan, Kind::Sistem] {
            assert_eq!(Kind::parse(k.as_str()), Some(k));
        }
        assert_eq!(Kind::parse("note"), None);
        let e = check_entry("sistem", "sampel fidelitas", None).unwrap_err();
        assert_eq!(e.field, "kind");
        assert_eq!(check_entry("tesis", "x", None).unwrap_err().field, "kind");
    }

    #[test]
    fn text_is_trimmed_and_bounded() {
        let e = check_entry("catatan", "  halo  ", None).unwrap();
        assert_eq!(e.text, "halo");
        assert_eq!(
            check_entry("catatan", "   ", None).unwrap_err().field,
            "text"
        );
        assert!(check_entry("catatan", &"x".repeat(2000), None).is_ok());
        assert_eq!(
            check_entry("catatan", &"x".repeat(2001), None)
                .unwrap_err()
                .field,
            "text"
        );
        // Characters, not bytes.
        assert!(check_entry("catatan", &"é".repeat(2000), None).is_ok());
    }

    #[test]
    fn a_level_entry_needs_a_positive_price_and_only_it_takes_one() {
        assert_eq!(
            check_entry("level", "harga incaran", Some(1_200.0))
                .unwrap()
                .level,
            Some(1_200.0)
        );
        for bad in [None, Some(0.0), Some(-5.0), Some(f64::NAN)] {
            assert_eq!(
                check_entry("level", "harga incaran", bad)
                    .unwrap_err()
                    .field,
                "level"
            );
        }
        assert_eq!(
            check_entry("keputusan", "tambah", Some(10.0))
                .unwrap_err()
                .field,
            "level"
        );
        assert!(check_entry("keputusan", "tambah", None).is_ok());
    }

    #[test]
    fn a_close_crossing_a_level_is_detected_in_both_directions() {
        let levels = [mark("l1", 100.0, "2026-01-01")];
        let up = crossings(
            &levels,
            &[(d("2026-02-02"), 99.0), (d("2026-02-03"), 101.0)],
        );
        assert_eq!(up.len(), 1);
        assert_eq!(up[0].direction, Direction::Up);
        assert_eq!(up[0].date, d("2026-02-03"));
        let down = crossings(
            &levels,
            &[(d("2026-02-02"), 101.0), (d("2026-02-03"), 99.0)],
        );
        assert_eq!(down[0].direction, Direction::Down);
        // Arriving exactly on the level counts; sitting on it and moving off does not.
        assert_eq!(
            crossings(
                &levels,
                &[(d("2026-02-02"), 99.0), (d("2026-02-03"), 100.0)]
            )
            .len(),
            1
        );
        assert!(crossings(
            &levels,
            &[(d("2026-02-02"), 100.0), (d("2026-02-03"), 101.0)]
        )
        .is_empty());
        // Staying on one side is nothing.
        assert!(crossings(
            &levels,
            &[(d("2026-02-02"), 101.0), (d("2026-02-03"), 150.0)]
        )
        .is_empty());
    }

    #[test]
    fn a_level_only_counts_for_moves_after_it_was_written() {
        let levels = [mark("l1", 100.0, "2026-02-03")];
        let closes = [
            (d("2026-02-02"), 99.0),
            (d("2026-02-03"), 101.0),
            (d("2026-02-04"), 99.0),
        ];
        // The move into 2026-02-03 started before the level existed; the next one counts.
        let found = crossings(&levels, &closes);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].date, d("2026-02-04"));
        assert_eq!(found[0].direction, Direction::Down);
    }

    #[test]
    fn several_levels_and_several_days_are_all_reported_in_order() {
        let levels = [
            mark("a", 100.0, "2026-01-01"),
            mark("b", 110.0, "2026-01-01"),
        ];
        let closes = [
            (d("2026-02-02"), 95.0),
            (d("2026-02-03"), 115.0),
            (d("2026-02-04"), 105.0),
        ];
        let found = crossings(&levels, &closes);
        let got: Vec<(&str, Direction, NaiveDate)> = found
            .iter()
            .map(|c| (c.level_id.as_str(), c.direction, c.date))
            .collect();
        assert_eq!(
            got,
            [
                ("a", Direction::Up, d("2026-02-03")),
                ("b", Direction::Up, d("2026-02-03")),
                ("b", Direction::Down, d("2026-02-04")),
            ]
        );
    }
}
