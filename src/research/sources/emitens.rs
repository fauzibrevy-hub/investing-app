//! `emitens.csv` (scraper master data, read-only): the company name per ticker. A missing
//! file or ticker gives no name; the page header then shows the ticker alone.

use std::collections::HashMap;
use std::path::Path;

/// Splits one CSV line, honouring double quotes.
fn split_line(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Ticker to trimmed, non-empty company name.
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else {
        return HashMap::new();
    };
    let cols: Vec<String> = split_line(header)
        .iter()
        .map(|c| c.trim().to_ascii_lowercase())
        .collect();
    let at = |name: &str| cols.iter().position(|c| c == name);
    let (Some(t), Some(n)) = (at("ticker"), at("name")) else {
        return HashMap::new();
    };
    lines
        .filter_map(|l| {
            let f = split_line(l);
            let ticker = f.get(t)?.trim().to_ascii_uppercase();
            let name = f.get(n)?.trim().to_string();
            (!name.is_empty()).then_some((ticker, name))
        })
        .collect()
}

pub fn load(path: &Path) -> std::io::Result<HashMap<String, String>> {
    Ok(parse(&std::fs::read_to_string(path)?))
}
