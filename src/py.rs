//! Python-compatibility helpers so code ported from MC (CPython 3.14)
//! produces identical output: text reading, string helpers, `float()`,
//! `round()`, `sum()`, `str()`, `isoformat()`.

use std::path::Path;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike};
use serde_json::Value;

/// Why a vault read failed, in MC's wording (`read_failure_detail`).
#[derive(Debug, Clone, PartialEq)]
pub struct ReadFailure {
    /// Python exception class name, e.g. `PermissionError`, `OSError`.
    pub class: String,
    pub errno: Option<i32>,
}

pub const READ_ERROR_MARK: &str = "gagal dibaca";

impl ReadFailure {
    pub fn named(class: &str) -> Self {
        Self {
            class: class.into(),
            errno: None,
        }
    }

    pub fn detail(&self) -> String {
        let errno = self
            .errno
            .map(|e| format!(" errno {e}"))
            .unwrap_or_default();
        format!(
            "{READ_ERROR_MARK} ({}{errno}) — berkas kemungkinan belum terunduh dari iCloud",
            self.class
        )
    }

    fn from_io(e: &std::io::Error) -> Self {
        use std::io::ErrorKind as K;
        let class = match e.kind() {
            K::PermissionDenied => "PermissionError",
            K::IsADirectory => "IsADirectoryError",
            K::NotADirectory => "NotADirectoryError",
            K::WouldBlock => "BlockingIOError",
            K::Interrupted => "InterruptedError",
            K::TimedOut => "TimeoutError",
            K::AlreadyExists => "FileExistsError",
            K::InvalidData => "UnicodeDecodeError",
            _ => "OSError",
        };
        Self {
            class: class.into(),
            errno: e.raw_os_error(),
        }
    }
}

/// `Path.read_text(encoding="utf-8")`: `Ok(None)` when the file does not
/// exist, universal-newline translation like Python text mode.
pub fn read_text(path: &Path) -> Result<Option<String>, ReadFailure> {
    match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => Ok(Some(universal_newlines(s))),
            Err(_) => Err(ReadFailure::named("UnicodeDecodeError")),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ReadFailure::from_io(&e)),
    }
}

fn universal_newlines(s: String) -> String {
    if s.contains('\r') {
        s.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        s
    }
}

/// `str.splitlines()` (Python splits on more than `\n`).
pub fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\u{0b}'
                | '\u{0c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if is_break {
            out.push(&s[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r' {
                if let Some((j, '\n')) = chars.peek().copied() {
                    chars.next();
                    end = j + 1;
                }
            }
            start = end;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Python `str.strip()` (whitespace per `str.isspace`).
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_py_space)
}

fn is_py_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

/// `float(str)` restricted to finite results; `None` when Python would raise
/// or produce inf/nan (which MC could not serialize anyway).
pub fn py_float(s: &str) -> Option<f64> {
    let s = strip(s);
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    let valid = !body.is_empty()
        && !body.starts_with('_')
        && !body.ends_with('_')
        && !body.contains("__")
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-' | '_'));
    if !valid {
        return None;
    }
    // Underscores are only allowed between digits.
    let b = body.as_bytes();
    for (i, ch) in b.iter().enumerate() {
        if *ch == b'_'
            && !(b[i - 1].is_ascii_digit() && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            return None;
        }
    }
    s.replace('_', "")
        .parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
}

/// A number the way MC's `_num` returns it: int when integral, else float.
pub fn int_or_float(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9.0e15 {
        Value::from(f as i64)
    } else {
        serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

/// `round(x, ndigits)` on a float: round-half-even on the exact binary value,
/// which is what Rust's fixed-precision formatting does.
pub fn py_round(x: f64, ndigits: usize) -> f64 {
    format!("{x:.ndigits$}").parse().unwrap_or(x)
}

/// `round(x)` / `int(round(x))`: half-even to an integer.
pub fn py_round_int(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// `str.title()` for ASCII words.
pub fn py_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_alpha = false;
    for c in s.chars() {
        out.extend(if prev_alpha {
            c.to_lowercase().collect::<Vec<_>>()
        } else {
            c.to_uppercase().collect()
        });
        prev_alpha = c.is_alphabetic();
    }
    out
}

/// CPython 3.12+ `sum()` over floats (Neumaier compensated summation).
pub fn py_fsum(values: &[f64]) -> f64 {
    let (mut total, mut c) = (0.0f64, 0.0f64);
    for &x in values {
        let t = total + x;
        if total.abs() >= x.abs() {
            c += (total - t) + x;
        } else {
            c += (x - t) + total;
        }
        total = t;
    }
    if c != 0.0 && c.is_finite() {
        total += c;
    }
    total
}

/// Python truthiness of a decoded JSON value.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `str(value)` for a JSON value as Python would print the decoded object.
pub fn py_str(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_i64().or(None) {
            Some(i) => i.to_string(),
            None => n
                .as_u64()
                .map(|u| u.to_string())
                .unwrap_or_else(|| py_float_repr(n.as_f64().unwrap_or(0.0))),
        },
        Value::Array(a) => format!("[{}]", a.iter().map(py_repr).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("{}: {}", py_repr(&Value::String(k.clone())), py_repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn py_repr(v: &Value) -> String {
    match v {
        Value::String(s) => {
            if s.contains('\'') && !s.contains('"') {
                format!("\"{s}\"")
            } else {
                format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
            }
        }
        other => py_str(other),
    }
}

/// `repr(float)`.
pub fn py_float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let sci = format!("{f:e}");
    let (mantissa, exp) = sci.split_once('e').expect("exponent");
    let exp: i32 = exp.parse().expect("int");
    if (-4..16).contains(&exp) {
        let s = f.to_string();
        if s.contains('.') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exp.abs())
    }
}

/// A timezone-aware datetime as produced by MC's `_iso_dt`.
pub type PyDateTime = DateTime<FixedOffset>;

/// `datetime.fromisoformat` (3.11+ common forms) after `Z` → `+00:00`;
/// naive results are UTC (MC `_iso_dt`).
pub fn iso_dt(raw: &str) -> Option<PyDateTime> {
    let s = strip(raw).replace('Z', "+00:00");
    if s.is_empty() {
        return None;
    }
    let (date_part, rest) = if s.len() >= 10 && s.is_char_boundary(10) {
        s.split_at(10)
    } else {
        (s.as_str(), "")
    };
    let date = NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
        .ok()
        .filter(|_| date_part.len() == 10)
        .or_else(|| {
            NaiveDate::parse_from_str(&s, "%Y%m%d")
                .ok()
                .filter(|_| s.len() == 8)
        })?;
    if s.len() == 8 && !s.contains('-') {
        return Some(utc().from_utc_datetime(&date.and_time(NaiveTime::MIN)));
    }
    if rest.is_empty() {
        return Some(utc().from_utc_datetime(&date.and_time(NaiveTime::MIN)));
    }
    let mut chars = rest.chars();
    chars.next()?; // any single separator char, like Python 3.11+
    let rest = chars.as_str();
    let tz_at = rest.find(['+', '-']);
    let (time_s, tz_s) = match tz_at {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let time = parse_time(time_s)?;
    let naive = NaiveDateTime::new(date, time);
    if tz_s.is_empty() {
        return Some(utc().from_utc_datetime(&naive));
    }
    let offset = parse_offset(tz_s)?;
    offset.from_local_datetime(&naive).single()
}

fn utc() -> FixedOffset {
    FixedOffset::east_opt(0).expect("utc")
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    let (hms, frac) = match s.split_once(['.', ',']) {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    };
    let parts: Vec<&str> = hms.split(':').collect();
    let num = |p: &str| {
        (p.len() == 2 && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse::<u32>().ok())
            .flatten()
    };
    let (h, m, sec) = match parts.as_slice() {
        [h] => (num(h)?, 0, 0),
        [h, m] => (num(h)?, num(m)?, 0),
        [h, m, s] => (num(h)?, num(m)?, num(s)?),
        _ => return None,
    };
    let micros = match frac {
        None => 0,
        Some(f) if !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()) => {
            let mut digits: String = f.chars().take(6).collect();
            while digits.len() < 6 {
                digits.push('0');
            }
            digits.parse().ok()?
        }
        _ => return None,
    };
    NaiveTime::from_hms_micro_opt(h, m, sec, micros)
}

fn parse_offset(s: &str) -> Option<FixedOffset> {
    let sign = if s.starts_with('-') { -1 } else { 1 };
    let digits: String = s[1..].chars().filter(|c| *c != ':').collect();
    if !digits.bytes().all(|b| b.is_ascii_digit()) || !matches!(digits.len(), 2 | 4 | 6) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| {
        digits
            .get(r)
            .map(|d| d.parse::<i32>().unwrap_or(0))
            .unwrap_or(0)
    };
    let secs = n(0..2) * 3600 + n(2..4) * 60 + n(4..6);
    FixedOffset::east_opt(sign * secs)
}

/// `datetime.isoformat()` for an aware datetime.
pub fn isoformat(dt: &PyDateTime) -> String {
    let base = if dt.nanosecond() / 1000 == 0 {
        dt.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        dt.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
    };
    let off = dt.offset().local_minus_utc();
    let sign = if off < 0 { '-' } else { '+' };
    let off = off.abs();
    let (h, m, s) = (off / 3600, off % 3600 / 60, off % 60);
    if s == 0 {
        format!("{base}{sign}{h:02}:{m:02}")
    } else {
        format!("{base}{sign}{h:02}:{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn splitlines_like_python() {
        assert_eq!(
            splitlines("a\r\nb\rc\n\nd\u{2028}e"),
            ["a", "b", "c", "", "d", "e"]
        );
        assert_eq!(splitlines("a\n"), ["a"]);
        assert!(splitlines("").is_empty());
    }

    #[test]
    fn floats_and_sums() {
        assert_eq!(py_float("1_000.5"), Some(1000.5));
        assert_eq!(py_float("1__0"), None);
        assert_eq!(py_float("1e3"), Some(1000.0));
        assert_eq!(py_float("inf"), None);
        assert_eq!(py_float("1.2.3"), None);
        assert_eq!(
            py_fsum(&[0.1, 0.2, 0.3]),
            0.6,
            "Python 3.12+: sum([.1,.2,.3]) == 0.6"
        );
        assert_eq!(py_fsum(&[1e16, 1.0, -1e16]), 1.0);
        assert_eq!(int_or_float(5.0), json!(5));
        assert_eq!(int_or_float(5.5), json!(5.5));
    }

    #[test]
    fn rounding_and_title() {
        assert_eq!(py_round(2.675, 2), 2.67);
        assert_eq!(py_round_int(2.5), 2);
        assert_eq!(py_round_int(3.5), 4);
        assert_eq!(py_round_int(-0.5), 0);
        assert_eq!(py_title("ust rates"), "Ust Rates");
        assert_eq!(py_title("inflation cb"), "Inflation Cb");
    }

    #[test]
    fn python_str_and_repr() {
        assert_eq!(py_str(&json!(2024)), "2024");
        assert_eq!(py_str(&json!(2024.0)), "2024.0");
        assert_eq!(py_str(&json!(true)), "True");
        assert_eq!(py_str(&json!(["a", 1])), "['a', 1]");
        assert_eq!(py_float_repr(1e16), "1e+16");
        assert_eq!(py_float_repr(0.0001), "0.0001");
        assert_eq!(py_float_repr(1e-5), "1e-05");
    }

    #[test]
    fn iso_datetimes() {
        let dt = iso_dt("2026-09-17T10:20:30.5+07:00").unwrap();
        assert_eq!(isoformat(&dt), "2026-09-17T10:20:30.500000+07:00");
        assert_eq!(
            isoformat(&iso_dt("2026-09-17T10:20:30Z").unwrap()),
            "2026-09-17T10:20:30+00:00"
        );
        assert_eq!(
            isoformat(&iso_dt("2026-09-17").unwrap()),
            "2026-09-17T00:00:00+00:00"
        );
        assert_eq!(
            isoformat(&iso_dt("2026-09-17 08:00").unwrap()),
            "2026-09-17T08:00:00+00:00"
        );
        assert!(iso_dt("17/09/2026").is_none());
        assert!(iso_dt("").is_none());
    }

    #[test]
    fn read_failures_are_worded_like_mc() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_text(&dir.path().join("nope.md")), Ok(None));
        let err = read_text(dir.path()).unwrap_err();
        assert_eq!(err.class, "IsADirectoryError");
        assert_eq!(err.detail(), "gagal dibaca (IsADirectoryError errno 21) — berkas kemungkinan belum terunduh dari iCloud");
        std::fs::write(dir.path().join("crlf.md"), "a\r\nb\rc").unwrap();
        assert_eq!(
            read_text(&dir.path().join("crlf.md")).unwrap().unwrap(),
            "a\nb\nc"
        );
        std::fs::write(dir.path().join("bad.md"), [0xff, 0xfe]).unwrap();
        assert_eq!(
            read_text(&dir.path().join("bad.md")).unwrap_err().detail(),
            "gagal dibaca (UnicodeDecodeError) — berkas kemungkinan belum terunduh dari iCloud"
        );
    }
}
