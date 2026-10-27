//! Rule-based KPI extraction from document text (research plan §6 R8, prd.md A8.1 to A8.3).
//!
//! Pure and deterministic: the same pages and the same KPI targets always give the same
//! proposals, with no model, network or clock. For every KPI of the pack in effect the
//! label (or an `extract_aliases` entry of the pack file) is searched on each line of each
//! page, the nearest number is read in Indonesian or English format, scaled into the KPI's
//! unit, and kept with its page and a snippet of at most 160 characters. Several candidates
//! per KPI are ranked; a KPI with no match produces nothing.
//!
//! Readings taken where the brief left room (ASUMSI R8):
//! - A number with one separator and exactly three digits after it (`1.234`, `1,234`) is a
//!   thousands group, except when it starts with `0` (`0.125` is a decimal). With both
//!   separators the last one is the decimal point. `1.234,5` and `1,234.5` both read 1234.5.
//! - `(12,5)` and `-12,5` are negative. `12,5%` is a percentage and is refused for a KPI
//!   that is not a ratio; a bare number for a `%` KPI is kept, ranked lower.
//! - A number with a scale word (`ribu`/`thousand`/`k`, `juta`/`million`/`jt`/`mn`,
//!   `miliar`/`billion`/`bn`, `triliun`/`trillion`/`tn`) or a unit (`kt`, `Mt`, `Mbcm`,
//!   `koz`, `Moz`, `ton`) is converted into the KPI's unit; one with none is taken as
//!   already in the KPI's unit and ranked lower. A number in a different dimension (`Mt` for
//!   a rupiah KPI, `USD` for a rupiah KPI) is refused.
//! - A bare four-digit year, and a number that follows `kuartal`, `Q`, `FY`, `tahun`,
//!   `halaman` and the like, is never a value.

use std::collections::BTreeMap;

/// One KPI the extractor looks for.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// The KPI metric id (`production_mt`).
    pub metric: String,
    pub label: String,
    /// `extract_aliases` of the pack file.
    pub aliases: Vec<String>,
    /// The pack's unit (`Mt`, `USD/t`, `IDR miliar`, ...), also the unit of the value.
    pub unit: String,
}

/// A ranked suggestion: `rank` 1 is the best candidate for that metric.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub metric: String,
    pub value: f64,
    pub unit: String,
    /// 1-based page or slide.
    pub page: u32,
    pub snippet: String,
    pub rank: u32,
}

pub const MAX_SNIPPET_CHARS: usize = 160;
/// Candidates kept per KPI.
pub const MAX_PER_KPI: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dim {
    Mass,
    Volume,
    Oz,
    Idr,
    Usd,
    Count,
}

/// What follows (and, for a currency, precedes) a number.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Suffix {
    percent: bool,
    /// Multiplier of the scale word or of the unit (`Mt` is 1e6, `kt` 1e3).
    scale: Option<f64>,
    dim: Option<Dim>,
    /// Bytes after the number that the suffix used.
    len: usize,
    /// Letters directly after the number that are no known suffix.
    stray: bool,
}

#[derive(Clone, Debug)]
struct Num {
    start: usize,
    end: usize,
    value: f64,
    suffix: Suffix,
    /// A currency written before the number (`Rp 5`, `US$ 3`).
    prefix_dim: Option<Dim>,
}

fn scale_word(w: &str) -> Option<f64> {
    Some(match w {
        "ribu" | "thousand" | "k" => 1e3,
        "juta" | "jt" | "million" | "mn" | "mio" => 1e6,
        "miliar" | "billion" | "bn" | "bio" => 1e9,
        "triliun" | "trillion" | "tn" => 1e12,
        _ => return None,
    })
}

/// A unit word: its dimension and, for a physical unit, the multiplier it carries by itself
/// (`Mt` is a million tonnes, `ton` one). A currency or a count says no magnitude.
fn unit_word(w: &str) -> Option<(Dim, Option<f64>)> {
    Some(match w {
        "mt" | "mmt" => (Dim::Mass, Some(1e6)),
        "kt" => (Dim::Mass, Some(1e3)),
        "ton" | "tons" | "tonne" | "tonnes" | "tonnage" => (Dim::Mass, Some(1.0)),
        "mbcm" => (Dim::Volume, Some(1e6)),
        "bcm" => (Dim::Volume, Some(1.0)),
        "moz" => (Dim::Oz, Some(1e6)),
        "koz" => (Dim::Oz, Some(1e3)),
        "oz" => (Dim::Oz, Some(1.0)),
        "usd" | "dollar" => (Dim::Usd, None),
        "rp" | "idr" | "rupiah" => (Dim::Idr, None),
        "kapal" | "vessel" | "vessels" | "unit" | "units" | "hari" | "days" | "day" | "tahun"
        | "years" | "year" | "x" => (Dim::Count, None),
        _ => return None,
    })
}

fn letters_at(s: &str, from: usize) -> &str {
    let rest = &s[from..];
    let n = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphabetic() || *b == b'$')
        .count();
    &rest[..n]
}

/// The suffix of the number that ends at `end` in `line`.
fn suffix_at(line: &str, end: usize) -> Suffix {
    let rest = &line[end..];
    let (gap, after) = match rest.strip_prefix(' ') {
        Some(r) => (1, r),
        None => (0, rest),
    };
    let mut out = Suffix::default();
    if after.starts_with('%') {
        out.percent = true;
        out.len = gap + 1;
        return out;
    }
    let w1 = letters_at(line, end + gap);
    if w1.is_empty() {
        return out;
    }
    let lw1 = w1.to_ascii_lowercase();
    let after_w1 = end + gap + w1.len();
    if let Some(s) = scale_word(&lw1) {
        out.scale = Some(s);
        out.len = gap + w1.len();
        // A unit may follow the scale word: `juta ton`, `million tonnes`, `miliar rupiah`.
        if line[after_w1..].starts_with(' ') {
            let w2 = letters_at(line, after_w1 + 1);
            if let Some((d, us)) = unit_word(&w2.to_ascii_lowercase()) {
                // A scale word and a unit that carries its own scale (`juta Mt`) do not stack.
                let stacks = matches!(us, None | Some(1.0));
                if stacks && !matches!(d, Dim::Count) {
                    out.dim = Some(d);
                    out.len += 1 + w2.len();
                }
            }
        }
        return out;
    }
    if let Some((d, s)) = unit_word(&lw1) {
        out.dim = Some(d);
        out.scale = s;
        out.len = gap + w1.len();
        return out;
    }
    // Letters glued to the number that mean nothing (`3Q26`): not a quantity.
    out.stray = gap == 0;
    out
}

fn prefix_currency(line: &str, start: usize) -> Option<Dim> {
    // Looks at the last few bytes only: lowercasing the whole prefix made a long line
    // quadratic.
    let before = line[..start].trim_end_matches([' ', '.']).as_bytes();
    for (word, dim) in [
        ("us$", Dim::Usd),
        ("usd", Dim::Usd),
        ("$", Dim::Usd),
        ("idr", Dim::Idr),
        ("rp", Dim::Idr),
    ] {
        let w = word.as_bytes();
        if before.len() >= w.len() && before[before.len() - w.len()..].eq_ignore_ascii_case(w) {
            let head = &before[..before.len() - w.len()];
            let glued = head.last().is_some_and(u8::is_ascii_alphanumeric);
            if !glued || word == "$" {
                return Some(dim);
            }
        }
    }
    None
}

/// Reads `1.234,5`, `1,234.5`, `1.234`, `12,5`, `12.5`, `1234`; `None` for `1.2.3`.
pub fn parse_number(tok: &str) -> Option<f64> {
    let dots = tok.matches('.').count();
    let commas = tok.matches(',').count();
    let digits_only = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let groups_ok = |s: &str, sep: char| {
        let mut it = s.split(sep);
        let head = it.next().unwrap_or("");
        digits_only(head)
            && head.len() <= 3
            && head != "0"
            && it.all(|g| g.len() == 3 && digits_only(g))
    };
    let cleaned = if dots > 0 && commas > 0 {
        let last_dot = tok.rfind('.')?;
        let last_comma = tok.rfind(',')?;
        let (thousand, decimal) = if last_dot > last_comma {
            (',', '.')
        } else {
            ('.', ',')
        };
        let (int_part, frac) = tok.rsplit_once(decimal)?;
        if !digits_only(frac) || !groups_ok(int_part, thousand) {
            return None;
        }
        format!("{}.{frac}", int_part.replace(thousand, ""))
    } else if dots + commas == 0 {
        if !digits_only(tok) {
            return None;
        }
        tok.to_string()
    } else {
        let sep = if dots > 0 { '.' } else { ',' };
        let count = dots + commas;
        if count > 1 {
            if !groups_ok(tok, sep) {
                return None;
            }
            tok.replace(sep, "")
        } else {
            let (head, tail) = tok.split_once(sep)?;
            if !digits_only(head) || !digits_only(tail) {
                return None;
            }
            if tail.len() == 3 && head.len() <= 3 && head != "0" {
                format!("{head}{tail}")
            } else {
                format!("{head}.{tail}")
            }
        }
    };
    cleaned.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Words after which a number is a period or a locator, never a value.
const PERIOD_WORDS: [&str; 22] = [
    "kuartal", "quarter", "triwulan", "semester", "tw", "q", "fy", "tahun", "year", "per", "no",
    "halaman", "hal", "hlm", "page", "pages", "pg", "p", "pp", "slide", "sheet", "bab",
];

const MONTHS: &[&str] = &[
    "jan",
    "januari",
    "january",
    "feb",
    "februari",
    "february",
    "mar",
    "maret",
    "march",
    "apr",
    "april",
    "mei",
    "may",
    "jun",
    "juni",
    "june",
    "jul",
    "juli",
    "july",
    "agu",
    "agt",
    "agustus",
    "aug",
    "august",
    "sep",
    "sept",
    "september",
    "okt",
    "oktober",
    "oct",
    "october",
    "nov",
    "november",
    "des",
    "desember",
    "dec",
    "december",
];

fn is_month(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    MONTHS.contains(&lower.as_str())
}

fn previous_word(line: &str, start: usize) -> &str {
    let before = line[..start].trim_end_matches([' ', '.']);
    let n = before
        .bytes()
        .rev()
        .take_while(|b| b.is_ascii_alphabetic())
        .count();
    &before[before.len() - n..]
}

/// The end of a numeric date that starts at `start`: `30-06-2026`, `30.06.26`, `2026-06-30`.
/// (`30/06/2026` is handled where the number is read.) A thousands number such as `1.234.567`
/// does not match: its groups have three digits.
fn date_at(b: &[u8], start: usize) -> Option<usize> {
    let digits = |from: usize, min: usize, max: usize| -> Option<usize> {
        let n = b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
        (min..=max).contains(&n).then_some(from + n)
    };
    let sep = |at: usize| matches!(b.get(at), Some(b'-' | b'.' | b'/'));
    for (first, middle, last) in [((1, 2), (1, 2), (2, 4)), ((4, 4), (1, 2), (1, 2))] {
        // Group lengths are tried longest first, so `30` is not read as `3` + `0`.
        let a = digits(start, first.0, first.1);
        let Some(a) = a else { continue };
        if !sep(a) {
            continue;
        }
        let Some(m) = digits(a + 1, middle.0, middle.1) else {
            continue;
        };
        if !sep(m) {
            continue;
        }
        if let Some(z) = digits(m + 1, last.0, last.1) {
            if !b.get(z).is_some_and(u8::is_ascii_digit) {
                return Some(z);
            }
        }
    }
    None
}

/// `12 345` and `1 234,5`: a number whose thousands groups are separated by a space (the
/// text normaliser turns a no-break space into one). `from` is the end of the leading
/// one to three digits. Returns the end of the number and its value.
fn spaced_number(line: &str, start: usize, from: usize) -> Option<(usize, f64)> {
    let b = line.as_bytes();
    let mut end = from;
    while b.get(end) == Some(&b' ')
        && b.len() >= end + 4
        && b[end + 1..end + 4].iter().all(u8::is_ascii_digit)
        && !b.get(end + 4).is_some_and(u8::is_ascii_digit)
    {
        end += 4;
    }
    if end == from {
        return None;
    }
    let mut text: String = line[start..end].replace(' ', "");
    if matches!(b.get(end), Some(b',' | b'.')) && b.get(end + 1).is_some_and(u8::is_ascii_digit) {
        let frac = b[end + 1..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        text.push('.');
        text.push_str(&line[end + 1..end + 1 + frac]);
        end += 1 + frac;
        // `1 234,567,8` is nothing we can read.
        if matches!(b.get(end), Some(b',' | b'.')) && b.get(end + 1).is_some_and(u8::is_ascii_digit)
        {
            return Some((end, f64::NAN));
        }
    }
    Some((end, text.parse::<f64>().unwrap_or(f64::NAN)))
}

/// A day of the month before a month name (`30 Juni 2026`, `5-Mei-26`).
fn day_before_month(line: &str, end: usize) -> bool {
    let rest = &line[end..];
    let Some(rest) = rest.strip_prefix([' ', '-', '/']) else {
        return false;
    };
    let word = letters_at(rest, 0);
    !word.is_empty()
        && is_month(word)
        && !rest[word.len()..].starts_with(|c: char| c.is_alphabetic())
}

/// A day after a month name and before a year (`Juni 30, 2026`).
fn day_after_month(line: &str, start: usize, end: usize) -> bool {
    if !is_month(previous_word(line, start)) {
        return false;
    }
    let rest = line[end..].trim_start_matches(',');
    let Some(rest) = rest.strip_prefix(' ') else {
        return false;
    };
    let n = rest.bytes().take_while(u8::is_ascii_digit).count();
    n == 4
}

/// A two digit year after a day and a month (`5 Mei 26`).
fn year_after_day_month(line: &str, start: usize) -> bool {
    if !is_month(previous_word(line, start)) {
        return false;
    }
    let before = line[..start].trim_end_matches([' ', '.', '-', '/']);
    let word = before
        .bytes()
        .rev()
        .take_while(u8::is_ascii_alphabetic)
        .count();
    before[..before.len() - word]
        .trim_end_matches([' ', '-', '/'])
        .ends_with(|c: char| c.is_ascii_digit())
}

/// Every number token of `line`, in order of appearance. Numbers that are not figures are
/// skipped: years, periods, page references, footnote markers, dates and times.
fn numbers(line: &str) -> Vec<Num> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        if let Some(e) = date_at(b, start) {
            i = e;
            continue;
        }
        let mut j = i;
        while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.' || b[j] == b',') {
            j += 1;
        }
        // The token ends on a digit: `12,` in "12, 13" is the number 12.
        let mut end = j;
        while end > start && !b[end - 1].is_ascii_digit() {
            end -= 1;
        }
        i = j.max(end);
        let glued_before = start > 0 && b[start - 1].is_ascii_alphanumeric();
        let date_like =
            (start > 0 && b[start - 1] == b'/' && start > 1 && b[start - 2].is_ascii_digit())
                || (b.get(end) == Some(&b'/') && b.get(end + 1).is_some_and(u8::is_ascii_digit));
        if glued_before || date_like {
            continue;
        }
        let plain_int = line[start..end].bytes().all(|c| c.is_ascii_digit());
        let mut spaced = false;
        let mut value = if plain_int && end - start <= 3 && b.get(end) == Some(&b' ') {
            match spaced_number(line, start, end) {
                Some((e, v)) => {
                    end = e;
                    i = i.max(e);
                    spaced = true;
                    if v.is_nan() {
                        continue;
                    }
                    v
                }
                None => match parse_number(&line[start..end]) {
                    Some(v) => v,
                    None => continue,
                },
            }
        } else {
            let Some(v) = parse_number(&line[start..end]) else {
                continue;
            };
            v
        };
        let suffix = suffix_at(line, end);
        if suffix.stray {
            continue;
        }
        let plain_int = plain_int && !spaced;
        let year_like = plain_int && end - start == 4 && (1990.0..=2100.0).contains(&value);
        let bare = !suffix.percent && suffix.scale.is_none() && suffix.dim.is_none();
        if year_like && bare {
            continue;
        }
        if PERIOD_WORDS.contains(&previous_word(line, start).to_ascii_lowercase().as_str()) {
            continue;
        }
        let before = &line[..start];
        let ndig = end - start;
        // A footnote marker or a list number: `(1)`, `1)`, `[1]`. A bracketed figure with a
        // decimal or three digits is an accounting negative (`(12,5)`), not a marker.
        let square = before.ends_with('[') && b.get(end) == Some(&b']');
        let round = before.ends_with('(') && b.get(end) == Some(&b')');
        let closes = matches!(b.get(end), Some(b')' | b']'));
        if plain_int && bare && ((square && ndig <= 3) || ((round || closes) && ndig <= 2)) {
            continue;
        }
        // A time of day: `12:30`.
        let time = (b.get(end) == Some(&b':') && b.get(end + 1).is_some_and(u8::is_ascii_digit))
            || (before.ends_with(':') && before.len() >= 2 && b[start - 2].is_ascii_digit());
        if time {
            continue;
        }
        // A day of the month: `30 Juni 2026`, `Juni 30, 2026`.
        if plain_int && ndig <= 2 && bare {
            let day = (1.0..=31.0).contains(&value)
                && (day_before_month(line, end) || day_after_month(line, start, end));
            if day || (ndig == 2 && year_after_day_month(line, start)) {
                continue;
            }
        }
        // Sign: a minus (or U+2212) right before, or accounting parentheses around.
        let minus = before.ends_with('-') || before.ends_with('\u{2212}');
        let minus = minus && {
            let cut = before.trim_end_matches(['-', '\u{2212}']);
            !cut.ends_with(|c: char| c.is_ascii_alphanumeric())
        };
        let after = &line[end + suffix.len..];
        let paren = before.ends_with('(') && after.starts_with(')');
        if minus || paren {
            value = -value;
        }
        out.push(Num {
            start,
            end: end + suffix.len,
            value,
            suffix,
            prefix_dim: prefix_currency(line, start),
        });
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Mass(f64),
    Volume(f64),
    Oz(f64),
    Idr(f64),
    Usd(f64),
    Percent,
    Plain,
}

fn kind_of(unit: &str) -> Kind {
    match unit {
        "Mt" | "juta DWT" => Kind::Mass(1e6),
        "Mbcm" => Kind::Volume(1e6),
        "oz" => Kind::Oz(1.0),
        "IDR miliar" => Kind::Idr(1e9),
        "USD juta" => Kind::Usd(1e6),
        "%" | "pp" => Kind::Percent,
        _ => Kind::Plain,
    }
}

/// The number in the KPI's unit and whether the text said so (`false`: taken as already
/// in that unit). `None` when the text is in another dimension.
fn convert(unit: &str, n: &Num) -> Option<(f64, bool)> {
    let s = &n.suffix;
    let dim = s.dim.or(n.prefix_dim);
    let scaled = |target_dim: Dim, base: f64| -> Option<(f64, bool)> {
        if s.percent || dim.is_some_and(|d| d != target_dim) {
            return None;
        }
        match s.scale {
            Some(scale) => Some((n.value * scale / base, true)),
            None => Some((n.value, false)),
        }
    };
    match kind_of(unit) {
        Kind::Mass(base) => scaled(Dim::Mass, base),
        Kind::Volume(base) => scaled(Dim::Volume, base),
        Kind::Oz(base) => scaled(Dim::Oz, base),
        Kind::Idr(base) => scaled(Dim::Idr, base),
        Kind::Usd(base) => scaled(Dim::Usd, base),
        Kind::Percent => {
            if s.scale.is_some() || dim.is_some() {
                None
            } else {
                Some((n.value, s.percent))
            }
        }
        Kind::Plain => {
            if s.percent && unit != "x" {
                return None;
            }
            // A price or a rate written with its currency: `USD 65 /t` for `USD/t`.
            let currency_ok = |d: Dim| {
                (d == Dim::Usd && unit.starts_with("USD"))
                    || (d == Dim::Idr && unit.starts_with("IDR"))
            };
            match dim {
                Some(Dim::Count) | None => {}
                Some(d) if currency_ok(d) => {}
                Some(_) => return None,
            }
            let explicit = dim.is_some() || s.scale.is_some();
            Some((n.value * s.scale.unwrap_or(1.0), explicit))
        }
    }
}

struct Match {
    target: usize,
    start: usize,
    end: usize,
}

fn norm_line(raw: &str) -> String {
    raw.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_word_char(c: char) -> bool {
    // A superscript digit is a footnote mark, not part of the word it follows.
    c.is_alphanumeric() && !matches!(c, '\u{b9}' | '\u{b2}' | '\u{b3}' | '\u{2070}'..='\u{2079}')
}

/// Every term of every target found in `lower` (an ASCII-lowercased line), the longest term
/// winning where two overlap.
fn find_matches(lower: &str, terms: &[(usize, String)]) -> Vec<Match> {
    let mut found: Vec<Match> = Vec::new();
    for (target, term) in terms {
        let mut from = 0;
        while let Some(at) = lower[from..].find(term.as_str()) {
            let start = from + at;
            let end = start + term.len();
            from = start + 1;
            while !lower.is_char_boundary(from) {
                from += 1;
            }
            let ok_before = lower[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !is_word_char(c));
            let ok_after = lower[end..].chars().next().is_none_or(|c| !is_word_char(c));
            if ok_before && ok_after {
                found.push(Match {
                    target: *target,
                    start,
                    end,
                });
            }
        }
    }
    // Longest first, then earliest; keep a match only when no kept match overlaps it.
    found.sort_by(|a, b| {
        (b.end - b.start)
            .cmp(&(a.end - a.start))
            .then(a.start.cmp(&b.start))
            .then(a.target.cmp(&b.target))
    });
    // Kept matches never overlap, so the only one that can overlap `m` is the last one
    // starting before `m.end`: a lookup in an ordered map, not a scan of everything kept.
    let mut kept: BTreeMap<usize, Match> = BTreeMap::new();
    for m in found {
        let clash = kept
            .range(..m.end)
            .next_back()
            .is_some_and(|(_, k)| k.end > m.start);
        if !clash {
            kept.insert(m.start, m);
        }
    }
    // One match per start: ordered by start already.
    kept.into_values().collect()
}

fn snippet(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= MAX_SNIPPET_CHARS {
        return text.to_string();
    }
    let mut s: String = chars[..MAX_SNIPPET_CHARS - 1].iter().collect();
    s.push('\u{2026}');
    s
}

/// A window of `line` of at most `MAX_SNIPPET_CHARS` characters that holds the byte span
/// `from..to`. Costs O(window), never O(line): a long line has many matches.
fn window(line: &str, from: usize, to: usize) -> String {
    if line.len() <= MAX_SNIPPET_CHARS {
        return line.to_string();
    }
    let span = line
        .get(from..to)
        .map_or(0, |t| t.chars().take(MAX_SNIPPET_CHARS).count());
    let room = MAX_SNIPPET_CHARS.saturating_sub(span) / 2;
    let back = |upto: usize, n: usize| -> usize {
        line[..upto]
            .char_indices()
            .rev()
            .take(n)
            .last()
            .map_or(upto, |(b, _)| b)
    };
    let mut lo = if from <= line.len() && line.is_char_boundary(from) {
        back(from, room)
    } else {
        0
    };
    let hi = match line[lo..].char_indices().nth(MAX_SNIPPET_CHARS) {
        Some((b, _)) => lo + b,
        None => {
            lo = back(line.len(), MAX_SNIPPET_CHARS);
            line.len()
        }
    };
    line[lo..hi].to_string()
}

#[derive(Debug)]
struct Cand {
    target: usize,
    value: f64,
    page: u32,
    line: usize,
    snippet: String,
    score: i32,
}

/// The ranked proposals of `pages` (page 1 is `pages[0]`) for `targets`. `annual_ok` is
/// false when the reported quarter is not Q4: an annual KPI is only reported then, so it is
/// not looked for.
pub fn propose(
    targets: &[Target],
    pages: &[String],
    annual_ok: &dyn Fn(&Target) -> bool,
) -> Vec<Proposal> {
    let mut terms: Vec<(usize, String)> = Vec::new();
    for (i, t) in targets.iter().enumerate() {
        if !annual_ok(t) {
            continue;
        }
        let mut seen: Vec<String> = Vec::new();
        for raw in std::iter::once(&t.label).chain(t.aliases.iter()) {
            let term = norm_line(raw).to_ascii_lowercase();
            if term.chars().count() >= 3 && !seen.contains(&term) {
                seen.push(term.clone());
                terms.push((i, term));
            }
        }
    }
    let mut cands: Vec<Cand> = Vec::new();
    for (pi, page) in pages.iter().enumerate() {
        let page_no = pi as u32 + 1;
        let lines: Vec<String> = page.lines().map(norm_line).collect();
        let lowered: Vec<String> = lines.iter().map(|l| l.to_ascii_lowercase()).collect();
        let matches: Vec<Vec<Match>> = lowered.iter().map(|l| find_matches(l, &terms)).collect();
        let last_text = lines.iter().rposition(|l| !l.is_empty());
        // Numbers of a line, computed once (a label-only line asks for the next line's).
        let mut cache: Vec<Option<Vec<Num>>> = vec![None; lines.len()];
        for (li, line) in lines.iter().enumerate() {
            if matches[li].is_empty() {
                continue;
            }
            let nums = cache[li].take().unwrap_or_else(|| numbers(line));
            for (mi, m) in matches[li].iter().enumerate() {
                let target = &targets[m.target];
                let next_start = matches[li].get(mi + 1).map_or(line.len(), |n| n.start);
                let prev_end = if mi == 0 { 0 } else { matches[li][mi - 1].end };
                // `nums` is ordered by position: binary-search the span instead of scanning
                // every number of a long line for every label.
                let limit = next_start.max(m.end);
                let lo = nums.partition_point(|n| n.start < m.end);
                let after = nums[lo..]
                    .iter()
                    .take_while(|n| n.end <= limit)
                    .find_map(|n| convert(&target.unit, n).map(|c| (n, c)));
                let hi = nums.partition_point(|n| n.end <= m.start);
                let hit = if let Some((n, (value, explicit))) = after {
                    let dist = (n.start - m.end).min(60) as i32;
                    Some((value, explicit, 100 - dist, window(line, m.start, n.end)))
                } else if let Some((n, (value, explicit))) = nums[..hi]
                    .iter()
                    .rev()
                    .take_while(|n| n.start >= prev_end)
                    .find_map(|n| convert(&target.unit, n).map(|c| (n, c)))
                {
                    let dist = (m.start - n.end).min(60) as i32;
                    Some((value, explicit, 80 - dist, window(line, n.start, m.end)))
                } else {
                    // The label stands alone (a slide heading, a table cell): the number is
                    // the first one of the next line, when that line names no KPI itself.
                    // A bare small integer alone on the last line of a page is a page
                    // number, not a figure.
                    let next = lines.get(li + 1).filter(|_| matches[li + 1].is_empty());
                    next.and_then(|nl| {
                        let next_nums = cache[li + 1].get_or_insert_with(|| numbers(nl));
                        let page_number_row = Some(li + 1) == last_text;
                        next_nums
                            .iter()
                            .filter(|n| {
                                let bare = n.suffix == Suffix::default() && n.prefix_dim.is_none();
                                !(page_number_row && bare && n.end - n.start <= 3)
                            })
                            .find_map(|n| convert(&target.unit, n).map(|c| (n.clone(), c)))
                            .map(|(n, (value, explicit))| {
                                let joined = format!(
                                    "{} | {}",
                                    window(line, m.start, m.end),
                                    window(nl, n.start, n.end)
                                );
                                (value, explicit, 60, joined)
                            })
                    })
                };
                if let Some((value, explicit, base, snip)) = hit {
                    if value.abs() > 1e15 {
                        continue;
                    }
                    cands.push(Cand {
                        target: m.target,
                        value,
                        page: page_no,
                        line: li,
                        snippet: snippet(&snip),
                        score: base + if explicit { 15 } else { -10 },
                    });
                }
            }
        }
    }
    // Best first per target; the same value on the same page is one candidate.
    let mut by_target: BTreeMap<usize, Vec<Cand>> = BTreeMap::new();
    for c in cands {
        by_target.entry(c.target).or_default().push(c);
    }
    let mut out = Vec::new();
    for (target, mut list) in by_target {
        list.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(a.page.cmp(&b.page))
                .then(a.line.cmp(&b.line))
                .then(a.value.total_cmp(&b.value))
        });
        let mut kept: Vec<Cand> = Vec::new();
        for c in list {
            if !kept
                .iter()
                .any(|k| k.page == c.page && (k.value - c.value).abs() < f64::EPSILON)
            {
                kept.push(c);
            }
        }
        kept.truncate(MAX_PER_KPI);
        for (i, c) in kept.into_iter().enumerate() {
            out.push(Proposal {
                metric: targets[target].metric.clone(),
                value: c.value,
                unit: targets[target].unit.clone(),
                page: c.page,
                snippet: c.snippet,
                rank: i as u32 + 1,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(metric: &str, label: &str, aliases: &[&str], unit: &str) -> Target {
        Target {
            metric: metric.into(),
            label: label.into(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            unit: unit.into(),
        }
    }

    fn run(targets: &[Target], pages: &[&str]) -> Vec<Proposal> {
        let pages: Vec<String> = pages.iter().map(|s| s.to_string()).collect();
        propose(targets, &pages, &|_| true)
    }

    fn first(targets: &[Target], text: &str) -> Option<f64> {
        run(targets, &[text]).first().map(|p| p.value)
    }

    #[test]
    fn numbers_read_indonesian_and_english_formats() {
        for (tok, want) in [
            ("1.234,5", Some(1234.5)),
            ("1,234.5", Some(1234.5)),
            ("12,5", Some(12.5)),
            ("12.5", Some(12.5)),
            ("1.234", Some(1234.0)),
            ("1,234", Some(1234.0)),
            ("0.125", Some(0.125)),
            ("1.234.567", Some(1_234_567.0)),
            ("1,234,567.89", Some(1_234_567.89)),
            ("42", Some(42.0)),
            ("1.2.3", None),
            ("12.34.5", None),
            ("1,23,456", None),
        ] {
            assert_eq!(parse_number(tok), want, "{tok}");
        }
    }

    #[test]
    fn the_nearest_number_after_the_label_is_taken() {
        let targets = [t("production_mt", "Produksi", &[], "Mt")];
        assert_eq!(first(&targets, "Produksi 12,5 Mt"), Some(12.5));
        assert_eq!(
            first(&targets, "Produksi    1.234,5 Mt   (2025: 10,0)"),
            Some(1234.5)
        );
        assert_eq!(
            first(&targets, "produksi kuartal 2 2026: 12,5 Mt"),
            Some(12.5)
        );
        // Case-insensitive, whole words only.
        assert_eq!(first(&targets, "PRODUKSI 3,2 Mt"), Some(3.2));
        assert_eq!(first(&targets, "Reproduksi 3,2 Mt"), None);
    }

    #[test]
    fn a_number_before_the_label_is_the_fallback() {
        let targets = [t("production_mt", "Produksi", &[], "Mt")];
        let got = run(&targets, &["12,5 Mt Produksi"]);
        assert_eq!(got[0].value, 12.5);
    }

    #[test]
    fn two_labels_on_a_line_each_keep_their_own_number() {
        let targets = [
            t("production_mt", "Produksi", &[], "Mt"),
            t("sales_mt", "Penjualan", &[], "Mt"),
        ];
        let got = run(&targets, &["Produksi 12,5 Mt Penjualan 11,0 Mt"]);
        assert_eq!(got.len(), 2);
        assert_eq!(
            (got[0].metric.as_str(), got[0].value),
            ("production_mt", 12.5)
        );
        assert_eq!((got[1].metric.as_str(), got[1].value), ("sales_mt", 11.0));
    }

    #[test]
    fn scale_words_and_units_convert_into_the_kpi_unit() {
        let mt = [t("production_mt", "Produksi", &[], "Mt")];
        for (text, want) in [
            ("Produksi 12,5 Mt", 12.5),
            ("Produksi 12,5 juta ton", 12.5),
            ("Produksi 12.5 million tonnes", 12.5),
            ("Produksi 3.400 kt", 3.4),
            ("Produksi 3,400 kt", 3.4),
            ("Produksi 2.500 ribu ton", 2.5),
            ("Produksi 12.500.000 ton", 12.5),
            ("Produksi 12,5", 12.5),
        ] {
            assert_eq!(first(&mt, text), Some(want), "{text}");
        }
        let idr = [t("seg_rev", "Pendapatan segmen mesin", &[], "IDR miliar")];
        for (text, want) in [
            ("Pendapatan segmen mesin Rp 5,5 triliun", 5500.0),
            ("Pendapatan segmen mesin Rp 5.500 miliar", 5500.0),
            ("Pendapatan segmen mesin IDR 7.250.000 juta", 7_250.0),
            ("Pendapatan segmen mesin 2,5 billion", 2.5),
            ("Pendapatan segmen mesin 1.200", 1200.0),
        ] {
            assert_eq!(first(&idr, text), Some(want), "{text}");
        }
        let usd = [t("backlog", "Backlog", &[], "USD juta")];
        assert_eq!(first(&usd, "Backlog US$ 1,2 miliar"), Some(1200.0));
        assert_eq!(first(&usd, "Backlog USD 350 million"), Some(350.0));
        let mbcm = [t("ob", "Overburden", &[], "Mbcm")];
        assert_eq!(first(&mbcm, "Overburden 45,2 Mbcm"), Some(45.2));
        assert_eq!(first(&mbcm, "Overburden 45.200.000 bcm"), Some(45.2));
        let oz = [t("gold", "Emas terjual", &[], "oz")];
        assert_eq!(first(&oz, "Emas terjual 52,3 koz"), Some(52_300.0));
        let dwt = [t(
            "fleet_capacity_mdwt",
            "Kapasitas armada",
            &["DWT"],
            "juta DWT",
        )];
        assert_eq!(first(&dwt, "Kapasitas armada 1,2 juta DWT"), Some(1.2));
        assert_eq!(first(&dwt, "DWT 1,5 juta"), Some(1.5));
    }

    #[test]
    fn percentages_parentheses_and_signs() {
        let pct = [t("car", "CAR", &["KPMM"], "%")];
        assert_eq!(first(&pct, "CAR 23,4%"), Some(23.4));
        assert_eq!(first(&pct, "KPMM 23,4 %"), Some(23.4));
        assert_eq!(first(&pct, "CAR 23,4"), Some(23.4));
        // A percent explicitly stated outranks a bare number.
        let got = run(&pct, &["CAR 11,0", "CAR 23,4%"]);
        assert_eq!(got[0].value, 23.4);
        assert_eq!(got[0].page, 2);
        assert_eq!(got[1].page, 1);
        let mt = [t("production_mt", "Produksi", &[], "Mt")];
        assert_eq!(first(&mt, "Produksi (1,5) Mt"), Some(-1.5));
        assert_eq!(first(&mt, "Produksi -1,5 Mt"), Some(-1.5));
        // A percentage is no tonnage; the next number is not guessed either.
        assert_eq!(first(&mt, "Produksi naik 5% dibanding tahun lalu"), None);
    }

    #[test]
    fn years_quarters_and_page_numbers_are_not_values() {
        let mt = [t("production_mt", "Produksi", &[], "Mt")];
        assert_eq!(first(&mt, "Produksi Q2 2026 12,5 Mt"), Some(12.5));
        assert_eq!(first(&mt, "Produksi 2Q26 12,5 Mt"), Some(12.5));
        assert_eq!(first(&mt, "Produksi FY2026 12,5 Mt"), Some(12.5));
        assert_eq!(first(&mt, "Produksi 2026"), None);
        assert_eq!(first(&mt, "Produksi tahun 2026 12,5 Mt"), Some(12.5));
        assert_eq!(first(&mt, "Produksi per 30/06/2026 12,5 Mt"), Some(12.5));
    }

    #[test]
    fn a_wrong_dimension_is_refused() {
        let mt = [t("production_mt", "Produksi", &[], "Mt")];
        assert_eq!(first(&mt, "Produksi Rp 12,5 miliar"), None);
        let idr = [t("seg_rev", "Pendapatan", &[], "IDR miliar")];
        assert_eq!(first(&idr, "Pendapatan USD 12 juta"), None);
        assert_eq!(first(&idr, "Pendapatan 12 Mt"), None);
        assert_eq!(first(&idr, "Pendapatan 12%"), None);
        let price = [t("asp", "Harga jual rata-rata", &["ASP"], "USD/t")];
        assert_eq!(first(&price, "ASP USD 65,3/t"), Some(65.3));
        assert_eq!(first(&price, "ASP 12%"), None);
    }

    #[test]
    fn aliases_match_and_the_longest_term_wins() {
        let targets = [
            t(
                "asp_usd_t",
                "Harga jual rata-rata",
                &["average selling price", "ASP"],
                "USD/t",
            ),
            t("cash_cost", "Biaya tunai", &["cash cost"], "USD/t"),
        ];
        assert_eq!(
            first(&targets, "Average selling price 65,3 USD/t"),
            Some(65.3)
        );
        let got = run(&targets, &["ASP 65,3 Cash cost 41,0"]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].metric, "asp_usd_t");
        assert_eq!(got[1].value, 41.0);
        // `Biaya tunai` inside a longer label of another KPI loses to that label.
        let two = [
            t("a", "Biaya tunai", &[], "USD/t"),
            t("b", "Biaya tunai per ton", &[], "USD/t"),
        ];
        let got = run(&two, &["Biaya tunai per ton 41,0"]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].metric, "b");
    }

    #[test]
    fn a_label_alone_on_a_line_takes_the_next_line() {
        let targets = [t("production_mt", "Produksi", &[], "Mt")];
        let got = run(&targets, &["Produksi\n12,5 Mt\nPenjualan 11,0 Mt"]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].value, 12.5);
        assert!(got[0].snippet.contains('|'));
        // A next line that names another KPI is not borrowed.
        let two = [
            t("production_mt", "Produksi", &[], "Mt"),
            t("sales_mt", "Penjualan", &[], "Mt"),
        ];
        let got = run(&two, &["Produksi\nPenjualan 11,0 Mt"]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].metric, "sales_mt");
    }

    #[test]
    fn candidates_are_ranked_capped_and_carry_their_page() {
        let targets = [t("production_mt", "Produksi", &[], "Mt")];
        let pages = [
            "Ringkasan\nProduksi 10,0",
            "Produksi 12,5 Mt\nProduksi 12,5 Mt",
            "Produksi 7,0 Mt",
            "Produksi 8,0 Mt",
            "Produksi 9,0 Mt",
        ];
        let got = run(&targets, &pages);
        assert_eq!(got.len(), MAX_PER_KPI);
        assert_eq!(got.iter().map(|p| p.rank).collect::<Vec<_>>(), [1, 2, 3]);
        // Unit stated beats unit assumed; the repeated 12,5 on page 2 is one candidate.
        assert_eq!((got[0].value, got[0].page), (12.5, 2));
        assert_eq!(got.iter().filter(|p| p.value == 12.5).count(), 1);
        assert!(got.iter().all(|p| p.unit == "Mt"));
        // Deterministic.
        assert_eq!(got, run(&targets, &pages));
    }

    #[test]
    fn no_match_is_no_proposal_and_annual_kpis_wait_for_q4() {
        let targets = [t("production_mt", "Produksi", &[], "Mt")];
        assert!(run(&targets, &["Tidak ada angka di sini", "Produksi"]).is_empty());
        assert!(run(&targets, &[]).is_empty());
        let pages = vec!["Cadangan 120 Mt".to_string()];
        let annual = [t("reserves_mt", "Cadangan", &[], "Mt")];
        assert!(propose(&annual, &pages, &|_| false).is_empty());
        assert_eq!(propose(&annual, &pages, &|_| true).len(), 1);
    }

    #[test]
    fn snippets_stay_within_160_characters() {
        let targets = [t("production_mt", "Produksi", &[], "Mt")];
        let long = format!("{} Produksi 12,5 Mt {}", "x".repeat(300), "y".repeat(300));
        let got = run(&targets, &[&long]);
        assert_eq!(got[0].value, 12.5);
        assert!(got[0].snippet.chars().count() <= MAX_SNIPPET_CHARS);
        assert!(got[0].snippet.contains("Produksi 12,5 Mt"));
        let wide = format!("Produksi {} 12,5 Mt", "ä".repeat(400));
        let got = run(&targets, &[&wide]);
        // The number is 400 characters away from the label, still the nearest, and the
        // snippet is cut on a character boundary.
        assert!(got
            .iter()
            .all(|p| p.snippet.chars().count() <= MAX_SNIPPET_CHARS));
    }
}
