//! Markdown helpers ported from MC `thesis_bank.py` (`_frontmatter`,
//! `_sections`, `_md_table_rows`, `_drop_header`, `_num`, `_period_key`).

use regex::Regex;
use serde_json::Value;

use crate::py::{int_or_float, py_float, splitlines, strip};

pub const DASH_VALUES: [&str; 7] = ["", "—", "-", "–", "n/a", "N/A", "TBD"];

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| regex::Regex::new($pat).expect("valid regex"));
    };
}
pub(crate) use re;

re!(FM_LINE, r"^([A-Za-z0-9_-]+):\s?(.*)$");
re!(TABLE_SEP, r"^:?-{2,}:?$");

/// Tolerant frontmatter: `---` block, or a bare leading `key: value` block.
pub fn frontmatter(text: Option<&str>) -> Vec<(String, String)> {
    let Some(text) = text.filter(|t| !t.is_empty()) else {
        return vec![];
    };
    let block: String = if let Some(rest) = text.strip_prefix("---") {
        match rest.find("\n---") {
            Some(end) => rest[..end].to_string(),
            None => rest.to_string(),
        }
    } else {
        let mut lines = Vec::new();
        for line in splitlines(text) {
            if FM_LINE.is_match(line) {
                lines.push(line);
            } else if !lines.is_empty() {
                break;
            }
        }
        lines.join("\n")
    };
    let mut fm: Vec<(String, String)> = Vec::new();
    for line in splitlines(&block) {
        if let Some(c) = FM_LINE.captures(line) {
            let (k, v) = (c[1].to_string(), strip(&c[2]).to_string());
            match fm.iter_mut().find(|(key, _)| *key == k) {
                Some(e) => e.1 = v,
                None => fm.push((k, v)),
            }
        }
    }
    fm
}

pub fn fm_get<'a>(fm: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fm.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Heading → body for headings of exactly `level`; a body ends at any
/// heading of level ≤ `level`. Later duplicates overwrite, keeping position.
pub fn sections(text: &str, level: usize) -> Vec<(String, String)> {
    let head = Regex::new(&format!(r"(?m)^#{{{level}}}\s+(.+?)\s*$")).expect("regex");
    let next = Regex::new(&format!(r"(?m)^#{{1,{level}}}\s+")).expect("regex");
    let mut out: Vec<(String, String)> = Vec::new();
    for c in head.captures_iter(text) {
        let m = c.get(0).expect("match");
        let start = m.end();
        let end = next
            .find(&text[start..])
            .map(|n| start + n.start())
            .unwrap_or(text.len());
        let (k, v) = (strip(&c[1]).to_string(), text[start..end].to_string());
        match out.iter_mut().find(|(key, _)| *key == k) {
            Some(e) => e.1 = v,
            None => out.push((k, v)),
        }
    }
    out
}

pub fn section<'a>(secs: &'a [(String, String)], name: &str) -> &'a str {
    secs.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

pub fn md_table_rows(block: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for ln in splitlines(block) {
        let s = strip(ln);
        if s.starts_with('|') && s.ends_with('|') && s.chars().count() > 2 {
            let cells: Vec<String> = s
                .trim_matches('|')
                .split('|')
                .map(|c| strip(c).to_string())
                .collect();
            if cells
                .iter()
                .filter(|c| !c.is_empty())
                .all(|c| TABLE_SEP.is_match(c))
            {
                continue; // alignment separator
            }
            rows.push(cells);
        }
    }
    rows
}

pub fn drop_header(mut rows: Vec<Vec<String>>, first_cell: &str) -> Vec<Vec<String>> {
    if rows
        .first()
        .and_then(|r| r.first())
        .is_some_and(|c| strip(c).to_lowercase() == first_cell.to_lowercase())
    {
        rows.remove(0);
    }
    rows
}

/// `_num`: table value → int/float, `null` for dashes and non-numbers.
pub fn num(v: &str) -> Value {
    let s = strip(v).replace('\u{a0}', "");
    if DASH_VALUES.contains(&s.as_str()) {
        return Value::Null;
    }
    let s2 = s.replace(' ', "").replace(',', ".");
    match py_float(&s2) {
        Some(f) => int_or_float(f),
        None => Value::Null,
    }
}

re!(PK_Q1, r"(?i)^(\d{4})\s*[-/]?\s*Q([1-4])$");
re!(PK_Q2, r"(?i)^Q([1-4])\s*[-/]?\s*(\d{4})$");
re!(PK_H1, r"(?i)^(\d{4})\s*[-/]?\s*H([1-2])$");
re!(PK_H2, r"(?i)^H([1-2])\s*[-/]?\s*(\d{4})$");
re!(PK_9M1, r"(?i)^(\d{4})\s*[-/]?\s*9M$");
re!(PK_9M2, r"(?i)^9M\s*[-/]?\s*(\d{4})$");
re!(PK_FY1, r"(?i)^(\d{4})\s*[-/]?\s*FY$");
re!(PK_FY2, r"(?i)^FY\s*[-/]?\s*(\d{4})$");
re!(PK_YEAR, r"(?i)^(\d{4})(?:\s*\(guidance\))?$");
re!(PK_ID_DATE, r"^(\d{1,2})\s+([A-Za-z]+)\s+(\d{4})$");
re!(PK_ISO, r"^(\d{4})-(\d{2})-(\d{2})$");

/// `_period_key`: sortable (year, position-in-year) for a period label.
pub fn period_key(p: &str) -> (i64, i64) {
    let s = strip(p);
    let n = |x: &str| x.parse::<i64>().unwrap_or(0);
    if let Some(c) = PK_Q1.captures(s) {
        return (n(&c[1]), 10 * n(&c[2]));
    }
    if let Some(c) = PK_Q2.captures(s) {
        return (n(&c[2]), 10 * n(&c[1]));
    }
    if let Some(c) = PK_H1
        .captures(s)
        .map(|c| (n(&c[1]), n(&c[2])))
        .or_else(|| PK_H2.captures(s).map(|c| (n(&c[2]), n(&c[1]))))
    {
        return (c.0, if c.1 == 1 { 25 } else { 45 });
    }
    if let Some(c) = PK_9M1.captures(s).or_else(|| PK_9M2.captures(s)) {
        return (n(&c[1]), 35);
    }
    if let Some(c) = PK_FY1.captures(s).or_else(|| PK_FY2.captures(s)) {
        return (n(&c[1]), 50);
    }
    if let Some(c) = PK_YEAR.captures(s) {
        return (n(&c[1]), 50);
    }
    if let Some(c) = PK_ID_DATE.captures(s) {
        let mon: String = c[2].to_lowercase().chars().take(3).collect();
        let month = match mon.as_str() {
            "jan" => 1,
            "feb" => 2,
            "mar" => 3,
            "apr" => 4,
            "mei" => 5,
            "jun" => 6,
            "jul" => 7,
            "agu" | "ags" => 8,
            "sep" => 9,
            "okt" => 10,
            "nov" => 11,
            "des" => 12,
            _ => 0,
        };
        return (n(&c[3]), month * 4 + (n(&c[1]) / 8).min(3));
    }
    if let Some(c) = PK_ISO.captures(s) {
        return (n(&c[1]), n(&c[2]) * 4 + (n(&c[3]) / 8).min(3));
    }
    (9999, 9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frontmatter_both_styles() {
        let fm = frontmatter(Some(
            "---\ncompany: PT A\nstatus_since: 2026-01-02\n---\nbody",
        ));
        assert_eq!(fm_get(&fm, "company"), Some("PT A"));
        let bare = frontmatter(Some("\nstatus: holdings\ncompany:  X \n\nnot: part\n"));
        assert_eq!(fm_get(&bare, "company"), Some("X"));
        assert_eq!(fm_get(&bare, "not"), None);
    }

    #[test]
    fn sections_stop_at_same_or_higher_heading() {
        let t = "# T\n## A\na1\n### sub\nx\n## B\nb\n# Top\ny";
        let s = sections(t, 2);
        assert_eq!(
            s.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(section(&s, "A"), "\na1\n### sub\nx\n");
        assert_eq!(section(&s, "B"), "\nb\n");
        assert_eq!(section(&sections(t, 3), "sub"), "\nx\n");
    }

    #[test]
    fn tables_skip_separators() {
        let rows = md_table_rows("| Metrik | Def |\n|---|:--:|\n| a | b |\n| | |\nnot a row\n||");
        assert_eq!(rows, vec![vec!["Metrik", "Def"], vec!["a", "b"]]);
        assert_eq!(drop_header(rows, "metrik"), vec![vec!["a", "b"]]);
    }

    #[test]
    fn numbers_and_periods() {
        assert_eq!(num("1 234,5"), json!(1234.5));
        assert_eq!(num("12"), json!(12));
        assert_eq!(num("—"), Value::Null);
        assert_eq!(num("1.234.567"), Value::Null);
        assert_eq!(period_key("2026 Q1"), (2026, 10));
        assert_eq!(period_key("Q3-2025"), (2025, 30));
        assert_eq!(period_key("H2 2025"), (2025, 45));
        assert_eq!(period_key("9M 2025"), (2025, 35));
        assert_eq!(period_key("FY2025"), (2025, 50));
        assert_eq!(period_key("2026 (guidance)"), (2026, 50));
        assert_eq!(period_key("31 Des 2025"), (2025, 51));
        assert_eq!(period_key("2026-06-30"), (2026, 27));
        assert_eq!(period_key("lainnya"), (9999, 9));
    }
}
