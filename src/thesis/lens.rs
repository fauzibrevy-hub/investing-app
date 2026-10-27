//! `GET /api/thesis/macro`: macro lens over the vault's ricky-lens maps,
//! ported from MC `thesis_bank.py` (`_parse_macro_theses`,
//! `_parse_macro_frameworks`, `_build_macro`). Read-only.

use std::path::Path;
use std::sync::LazyLock;

use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use serde_json::{json, Map, Value};

use crate::py::{read_text, splitlines, strip, ReadFailure};

pub const TESIS_MAP: &str = "finance/article-bank/ricky-lens/01-tesis-map.md";
pub const FRAMEWORK_MAP: &str = "finance/article-bank/ricky-lens/00-framework-map.md";

static H2: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^##\s+(T\d+[a-z]?)\.\s+(.*?)(?:\s*\((.*?)\))?$").expect("regex"));
static H3: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^###\s+(T\d+[a-z]?)\.\s+([A-Za-z0-9/][^—\n\r]*?)(?:\s*—\s*(.*?))?(?:\s*\((.*?)\))?$",
    )
    .expect("regex")
});
static BOLD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*([^*]+)\*\*").expect("regex"));

fn field(body: &str, label: &str) -> String {
    let re = FancyRegex::new(&format!(r"(?s){label}\s*(.*?)(?=\n-|\z)")).expect("regex");
    re.captures(body)
        .ok()
        .flatten()
        .and_then(|c| c.get(1).map(|m| strip(m.as_str()).to_string()))
        .unwrap_or_default()
}

fn fields_all(body: &str, label: &str) -> Vec<String> {
    let re = FancyRegex::new(&format!(r"(?s){label}\s*(.*?)(?=\n-|\z)")).expect("regex");
    re.captures_iter(body)
        .flatten()
        .filter_map(|c| c.get(1).map(|m| strip(m.as_str()).to_string()))
        .collect()
}

struct Item {
    obj: Map<String, Value>,
    lines: Vec<String>,
}

fn heading_item(line: &str, c: &regex::Captures, seen: &mut Vec<(String, usize)>) -> Item {
    let code = c[1].to_string();
    let n = match seen.iter_mut().find(|(k, _)| *k == code) {
        Some(e) => {
            e.1 += 1;
            e.1
        }
        None => {
            seen.push((code.clone(), 1));
            1
        }
    };
    let id = if n == 1 {
        code.clone()
    } else {
        format!("{code}-{n}")
    };
    let meta = c.get(3).map(|m| m.as_str()).unwrap_or("");
    let mut obj = Map::new();
    obj.insert("id".into(), json!(id));
    obj.insert("code".into(), json!(code));
    obj.insert("title".into(), json!(strip(&c[2])));
    obj.insert("raw_heading".into(), json!(line));
    obj.insert("meta".into(), json!(meta));
    obj.insert(
        "stance".into(),
        json!(if meta.is_empty() { "unstated" } else { meta }),
    );
    Item { obj, lines: vec![] }
}

pub fn parse_theses(content: &str) -> (Vec<Value>, Vec<Value>) {
    let mut theses: Vec<Item> = Vec::new();
    let mut recs: Vec<Item> = Vec::new();
    // Which list the current item lives in: (is_reconciliation, index).
    let mut current: Option<(bool, usize)> = None;
    let mut in_rec = false;
    let mut seen: Vec<(String, usize)> = Vec::new();

    for line in splitlines(content) {
        if line.starts_with("## Rekonsiliasi") {
            in_rec = true;
            current = None;
            continue;
        }
        if line.starts_with("## ") {
            if let Some(c) = H2.captures(line) {
                theses.push(heading_item(line, &c, &mut seen));
                current = Some((false, theses.len() - 1));
                continue;
            }
        } else if line.starts_with("### ") && in_rec {
            if let Some(c) = H3.captures(line) {
                let meta_note = c.get(3).map(|m| m.as_str()).unwrap_or("");
                let count_note = c.get(4).map(|m| m.as_str()).filter(|s| !s.is_empty());
                let meta = match count_note {
                    Some(n) => format!("{meta_note} ({n})"),
                    None => meta_note.to_string(),
                };
                let mut obj = Map::new();
                obj.insert("code".into(), json!(&c[1]));
                obj.insert("title".into(), json!(strip(&c[2])));
                obj.insert("meta".into(), json!(strip(&meta)));
                obj.insert("article_count".into(), json!(count_note.unwrap_or("")));
                recs.push(Item { obj, lines: vec![] });
                current = Some((true, recs.len() - 1));
                continue;
            }
        }
        match current {
            Some((false, i)) => theses[i].lines.push(line.to_string()),
            Some((true, i)) => recs[i].lines.push(line.to_string()),
            None => {}
        }
    }

    let theses = theses
        .into_iter()
        .map(|mut t| {
            let body = strip(&t.lines.join("\n")).to_string();
            let o = &mut t.obj;
            o.insert("raw_body".into(), json!(body));
            o.insert("chain".into(), json!(field(&body, r"\*\*Rantai:\*\*")));
            o.insert(
                "preseden".into(),
                json!(field(&body, r"-\s+\*\*Preseden:\*\*")),
            );
            let dukung = field(&body, r"-\s+\*\*Dukung:\*\*");
            let dukung: Vec<String> = dukung
                .split(',')
                .map(|d| strip(d).to_string())
                .filter(|d| !d.is_empty())
                .collect();
            o.insert("dukung".into(), json!(dukung));
            o.insert("guard".into(), json!(field(&body, r"-\s+\*\*Guard:\*\*")));
            o.insert(
                "bukti".into(),
                json!(field(&body, r"-\s+\*\*Bukti(?:\s*\([^)]*\))?:\*\*")),
            );
            o.insert(
                "koneksi".into(),
                json!(fields_all(&body, r"-\s+\*\*Koneksi[^*]*:\*\*")),
            );
            o.insert(
                "updates".into(),
                json!(fields_all(&body, r"-\s+\*\*Update[^*]*:\*\*")),
            );
            Value::Object(t.obj)
        })
        .collect();
    let recs = recs
        .into_iter()
        .map(|mut r| {
            let body = strip(&r.lines.join("\n")).to_string();
            let o = &mut r.obj;
            o.insert("raw_body".into(), json!(body));
            o.insert("core".into(), json!(field(&body, r"\*\*Inti:\*\*")));
            o.insert(
                "articles_ref".into(),
                json!(field(&body, r"-\s+\*\*Artikel:\*\*")),
            );
            o.insert("guard".into(), json!(field(&body, r"-\s+\*\*Guard:\*\*")));
            o.insert(
                "updates".into(),
                json!(fields_all(&body, r"-\s+\*\*Update[^*]*:\*\*")),
            );
            Value::Object(r.obj)
        })
        .collect();
    (theses, recs)
}

fn table_parts(line: &str) -> Vec<String> {
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() < 2 {
        return vec![];
    }
    parts[1..parts.len() - 1]
        .iter()
        .map(|p| strip(p).to_string())
        .collect()
}

fn is_table_row(line: &str) -> bool {
    line.starts_with('|') && !line.starts_with("| Framework") && !line.starts_with("|---")
}

pub fn parse_frameworks(content: &str) -> (Vec<Value>, String, Vec<Value>) {
    let mut frameworks = Vec::new();
    let mut meta_pattern = String::new();
    let mut recs: Vec<(String, Vec<Value>, String)> = Vec::new();
    let mut sec = "main";

    for line in splitlines(content) {
        if line.starts_with("## Pola muara") {
            sec = "meta_pattern";
            continue;
        } else if line.starts_with("## Rekonsiliasi") {
            sec = "reconciliation";
            recs.push((
                strip(&line.replace("##", "")).to_string(),
                vec![],
                String::new(),
            ));
            continue;
        }
        match sec {
            "meta_pattern" => {
                if !strip(line).is_empty() && !line.starts_with('#') {
                    meta_pattern.push_str(strip(line));
                    meta_pattern.push(' ');
                }
            }
            "main" => {
                if is_table_row(line) {
                    let p = table_parts(line);
                    if p.len() >= 4 {
                        frameworks.push(json!({
                            "name": BOLD.replace_all(&p[0], "$1"), "raw_name": p[0],
                            "tally": p[1], "examples": p[2], "gist": p[3],
                        }));
                    }
                }
            }
            _ => {
                let Some(rec) = recs.last_mut() else { continue };
                if is_table_row(line) {
                    let p = table_parts(line);
                    if p.len() >= 4 {
                        let wide = p.len() > 4;
                        rec.1.push(json!({
                            "name": BOLD.replace_all(&p[0], "$1"),
                            "raw_name": p[0],
                            "classification": if wide { json!(p[1]) } else { Value::Null },
                            "delta": if wide { &p[2] } else { &p[1] },
                            "total": if wide { &p[3] } else { &p[2] },
                            "references": if wide { &p[4] } else { &p[3] },
                            "notes": p.get(5).cloned().unwrap_or_default(),
                        }));
                    }
                } else if line.contains("**Guard:**") {
                    rec.2 = strip(line.rsplit("**Guard:**").next().unwrap_or("")).to_string();
                }
            }
        }
    }
    let recs = recs
        .into_iter()
        .map(|(title, rows, guard)| json!({"title": title, "rows": rows, "guard": guard}))
        .collect();
    (frameworks, strip(&meta_pattern).to_string(), recs)
}

fn read_map(path: &Path, missing_label: &str, errors: &mut Vec<String>) -> Option<String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match read_text(path) {
        Ok(Some(t)) if !t.is_empty() => Some(t),
        Ok(_) => {
            errors.push(format!("Berkas {missing_label} tidak ditemukan: {name}"));
            None
        }
        // MC wraps the failure in VaultReadError before wording it.
        Err(_) => {
            errors.push(format!(
                "{name}: {}",
                ReadFailure::named("VaultReadError").detail()
            ));
            None
        }
    }
}

pub fn build(vault_root: &Path, now_unix: u64) -> Value {
    let mut errors = Vec::new();
    let (mut theses, mut theses_recs) = (vec![], vec![]);
    let (mut frameworks, mut meta_pattern, mut fw_recs) = (vec![], String::new(), vec![]);
    if let Some(t) = read_map(&vault_root.join(TESIS_MAP), "tesis map", &mut errors) {
        (theses, theses_recs) = parse_theses(&t);
    }
    if let Some(f) = read_map(
        &vault_root.join(FRAMEWORK_MAP),
        "framework map",
        &mut errors,
    ) {
        (frameworks, meta_pattern, fw_recs) = parse_frameworks(&f);
    }
    json!({
        "source_files": {"tesis_map": TESIS_MAP, "framework_map": FRAMEWORK_MAP},
        "meta_pattern": meta_pattern,
        "theses": theses,
        "theses_reconciliations": theses_recs,
        "frameworks": frameworks,
        "framework_reconciliations": fw_recs,
        "errors": errors,
        "generated_at": now_unix,
        "is_readonly": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TESIS: &str = "# Map\n\n## T1. Minyak naik (bullish)\n**Rantai:** supply → harga\n- **Preseden:** 2008\n- **Dukung:** a, b ,\n- **Update 2026:** satu\n- **Update 2027:** dua\n## T1. Ulang\nbody\n## Catatan bebas\n## Rekonsiliasi\n### T1. Minyak — dicek (3 artikel)\n**Inti:** x\n- **Guard:** g\n";

    #[test]
    fn theses_and_reconciliations() {
        let (t, r) = parse_theses(TESIS);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0]["id"], "T1");
        assert_eq!(t[0]["stance"], "bullish");
        assert_eq!(t[0]["chain"], "supply → harga");
        assert_eq!(t[0]["dukung"], json!(["a", "b"]));
        assert_eq!(t[0]["updates"], json!(["satu", "dua"]));
        assert_eq!(t[1]["id"], "T1-2");
        assert_eq!(t[1]["stance"], "unstated");
        assert_eq!(
            t[1]["raw_body"], "body\n## Catatan bebas",
            "non-matching ## lines stay in the body"
        );
        assert_eq!(r[0]["meta"], "dicek (3 artikel)");
        assert_eq!(r[0]["article_count"], "3 artikel");
        assert_eq!(r[0]["guard"], "g");
    }

    #[test]
    fn frameworks_meta_and_reconciliation() {
        let f = "| Framework | Tally | Contoh | Gist |\n|---|---|---|---|\n| **Likuiditas** | 3 | a | b |\n## Pola muara\nsatu\n\ndua\n## Rekonsiliasi Okt\n| Framework | Δ | Total | Ref |\n| X | +1 | 4 | r |\n| Y | k | +2 | 5 | r | n |\n**Guard:** hati-hati\n";
        let (fw, meta, recs) = parse_frameworks(f);
        assert_eq!(
            fw,
            vec![
                json!({"name": "Likuiditas", "raw_name": "**Likuiditas**", "tally": "3", "examples": "a", "gist": "b"})
            ]
        );
        assert_eq!(meta, "satu dua");
        assert_eq!(recs[0]["title"], "Rekonsiliasi Okt");
        assert_eq!(recs[0]["rows"][0]["classification"], Value::Null);
        assert_eq!(recs[0]["rows"][0]["delta"], "+1");
        assert_eq!(recs[0]["rows"][1]["classification"], "k");
        assert_eq!(recs[0]["rows"][1]["notes"], "n");
        assert_eq!(recs[0]["guard"], "hati-hati");
    }

    #[test]
    fn missing_maps_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let v = build(dir.path(), 5);
        assert_eq!(
            v["errors"],
            json!([
                "Berkas tesis map tidak ditemukan: 01-tesis-map.md",
                "Berkas framework map tidak ditemukan: 00-framework-map.md"
            ])
        );
        assert_eq!(v["generated_at"], 5);
    }
}
