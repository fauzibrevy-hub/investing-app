//! Element name → display label (research plan §3.1).
//!
//! A label file (`element,label_id,label_en` CSV, one line per element) is used when it
//! exists; otherwise the element's local name is split into words, which is readable
//! enough for the fact drawer and never wrong about which element it names.

use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Label {
    pub id: String,
    pub en: String,
}

#[derive(Clone, Debug, Default)]
pub struct Labels {
    overrides: HashMap<String, Label>,
}

impl Labels {
    /// Loads `labels.csv` when it exists; a missing or unreadable file gives the fallback.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_csv(&text),
            Err(_) => Self::default(),
        }
    }

    /// `element,label_id,label_en`; a header line and blank lines are ignored.
    pub fn from_csv(text: &str) -> Self {
        let mut overrides = HashMap::new();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let mut parts = line.splitn(3, ',');
            let (Some(element), Some(id)) = (parts.next(), parts.next()) else {
                continue;
            };
            if element == "element" {
                continue;
            }
            let en = parts.next().unwrap_or(id).trim().to_string();
            overrides.insert(
                element.trim().to_string(),
                Label {
                    id: id.trim().to_string(),
                    en,
                },
            );
        }
        Self { overrides }
    }

    pub fn label(&self, element: &str) -> Label {
        if let Some(l) = self.overrides.get(element) {
            return l.clone();
        }
        let words = humanize(element.rsplit(':').next().unwrap_or(element));
        Label {
            id: words.clone(),
            en: words,
        }
    }
}

/// `WadiahDemandDeposits` → `Wadiah Demand Deposits`; `CASARatio` → `CASA Ratio`;
/// `Prior2YearEndDate` → `Prior 2 Year End Date`.
pub fn humanize(local: &str) -> String {
    let chars: Vec<char> = local.chars().collect();
    let mut out = String::with_capacity(local.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 {
            let prev = chars[i - 1];
            let next = chars.get(i + 1).copied();
            let boundary = (c.is_uppercase()
                && (prev.is_lowercase()
                    || prev.is_ascii_digit()
                    || (prev.is_uppercase() && next.is_some_and(|n| n.is_lowercase()))))
                || (c.is_ascii_digit() && !prev.is_ascii_digit())
                || (!c.is_ascii_digit() && prev.is_ascii_digit());
            if boundary {
                out.push(' ');
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanizes_camel_case_and_acronyms() {
        assert_eq!(humanize("WadiahDemandDeposits"), "Wadiah Demand Deposits");
        assert_eq!(humanize("CASARatio"), "CASA Ratio");
        assert_eq!(humanize("Prior2YearEndDate"), "Prior 2 Year End Date");
        assert_eq!(humanize("Cash"), "Cash");
        assert_eq!(humanize("NPFGross"), "NPF Gross");
    }

    #[test]
    fn overrides_win_and_fallback_strips_the_prefix() {
        let l = Labels::from_csv(
            "element,label_id,label_en\nidx-cor:Cash,Kas,Cash\nidx-cor:Loans,Pembiayaan\n\n",
        );
        assert_eq!(
            l.label("idx-cor:Cash"),
            Label {
                id: "Kas".into(),
                en: "Cash".into()
            }
        );
        assert_eq!(l.label("idx-cor:Loans").en, "Pembiayaan");
        assert_eq!(l.label("idx-cor:SalesAndRevenue").en, "Sales And Revenue");
        assert_eq!(
            Labels::load(Path::new("/nonexistent/labels.csv"))
                .label("idx-cor:Cash")
                .id,
            "Cash"
        );
    }
}
