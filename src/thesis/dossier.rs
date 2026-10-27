//! Dossier reader for `finance/stock/<status>/<TICKER>` in the vault, ported
//! from MC `backend/api/thesis_bank.py` (`_scan_issuers`, `_parse_*`,
//! `_issuer_summary`, `_build_detail`, `_public_thesis_detail`).
//!
//! Strictly read-only. MC's gap-reason machinery (`_build_gap_reasons`) is
//! not ported: its output is removed by `_public_thesis_detail` and never
//! reaches the API.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{Local, NaiveDate};
use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use serde_json::{json, Map, Value};

use super::md::{
    drop_header, fm_get, frontmatter, md_table_rows, num, period_key, re, section, sections,
};
use crate::py::{
    self, iso_dt, isoformat, py_fsum, py_str, read_text, splitlines, strip, truthy, PyDateTime,
    ReadFailure,
};

pub const STATUSES: [&str; 3] = ["holdings", "watchlist", "archived"];
pub const DOSSIER_FILES: [&str; 5] = [
    "00-lens.md",
    "10-fundamental.md",
    "20-naratif.md",
    "thesis.md",
    "review-log.md",
];
const STATUS_AXES: [&str; 3] = ["fundamental_status", "growth_status", "narrative_status"];
const READ_RETRY_DELAY: Duration = Duration::from_millis(500);
const MAX_RETRIES_PER_BUILD: u32 = 4;

macro_rules! fre {
    ($name:ident, $pat:expr) => {
        static $name: std::sync::LazyLock<FancyRegex> =
            std::sync::LazyLock::new(|| FancyRegex::new($pat).expect("valid regex"));
    };
}

fn ffind<'t>(re: &FancyRegex, s: &'t str) -> Option<fancy_regex::Captures<'t>> {
    re.captures(s).ok().flatten()
}

fn collapse_ws(s: &str) -> String {
    WS.replace_all(s, " ").to_string()
}

re!(WS, r"\s+");
re!(PARA_SPLIT, r"\n\s*\n");
re!(H3_NAME_SPLIT, r"\s+[—–-]\s+");
re!(FALSIFIER_ITEM, r"^\s*(\d+)\.\s+(.*)$");
re!(ISO_DATE, r"\d{4}-\d{2}-\d{2}");
re!(BACKTICK, r"`([^`]+)`");
re!(
    STATUS_LINE,
    r"(?im)^[ \t]*(?:[-+][ \t]+)?\*{0,2}(fundamental_status|growth_status|narrative_status)[^`\n:]*:?[ \t]*\*{0,2}[ \t]*`([^`]+)`[^\n]*\n?"
);
re!(REVIEW_HEAD, r"(?m)^##\s+(.+?)\s*$");
re!(REVIEW_TITLE_DATE, r"^\d{4}-\d{2}-\d{2}\s*[—–-]\s*");
re!(BULLET, r"^(\s*)-\s+(.*)$");
re!(TRIGGER_ITEM, r"^\s*(?:\d+\.|\*|-)\s+(.*)$");
re!(YEAR_DIR, r"^\d{4}$");
re!(HISTORY_NAME, r"^financial_history_\d{4}_\d{4}\.json$");
re!(Q_PERIOD, r"^Q[1-4]$");
re!(MISSING_PERIOD, r"^(?:20\d{2}[-/]Q[1-4]|20\d{2})$");
re!(TINDAK_LANJUT, r"\*\*Tindak lanjut");
re!(ARTICLE_DATE, r"^(\d{4}-\d{2}-\d{2})");
re!(
    STATUS_HARGA,
    r"(?i)status(?:\s+harga)?\s*:\s*`?([a-zA-Z0-9_-]+)`?"
);
re!(SUMBER_SPLIT, r"\s*[—–-]\s*`?sumber`?\s*:");
re!(
    TAHAP_FM,
    r"(?m)^tahap:\s*\*{0,2}([a-zA-Z0-9_-]+)\*{0,2}(?:\s*[—–-]\s*(.+?))?$"
);
fre!(
    ALASAN,
    r"(?ms)^-\s*`alasan_masuk`\s*:\s*(.+?)(?=\n-\s*`|\n\n)"
);
fre!(
    BAYAR,
    r"(?ms)^-\s*`?bayar_risiko`?\s*:\s*(.+?)(?=\n-\s*`|\n\n|\z)"
);
fre!(
    HARGA,
    r"(?ms)^-\s*`?harga_acuan`?\s*:\s*(.+?)(?=\n-\s*`|\n\n|\z)"
);
fre!(
    SUMBER,
    r"(?i)(?:`?sumber`?|sumber)\s*:\s*(.+?)(?=\s*[—–-]\s*status|\z)"
);
fre!(
    TAHAP,
    r"(?ms)^-\s*`?tahap`?\s*:\s*\*{0,2}([a-zA-Z0-9_-]+)\*{0,2}(?:\s*[—–-]\s*(.+?))?(?=\n-\s*`|\n\n|\n#|\z)"
);

/// Per-build read state: MC retries a failed dossier read once, at most
/// four times per build.
#[derive(Default)]
pub struct Build {
    retries_used: u32,
}

impl Build {
    /// `_read_text(path, retry=True)`.
    fn read(&mut self, path: &Path) -> Result<Option<String>, ReadFailure> {
        match read_text(path) {
            Err(_) if self.retries_used < MAX_RETRIES_PER_BUILD => {
                self.retries_used += 1;
                std::thread::sleep(READ_RETRY_DELAY);
                read_text(path)
            }
            other => other,
        }
    }
}

/// `_read_text_or_none`: single attempt, failures read as missing.
fn read_or_none(path: &Path) -> Option<String> {
    read_text(path).ok().flatten()
}

/// `str(VaultReadError)`.
fn vault_read_error(path: &Path, f: &ReadFailure) -> String {
    format!("{}: {}", file_name(path), f.detail())
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub struct Issuer {
    pub ticker: String,
    pub status: &'static str,
    pub dir: PathBuf,
}

fn sorted_children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn hidden_or_private(p: &Path) -> bool {
    let n = file_name(p);
    n.starts_with('.') || n.starts_with('_')
}

pub fn scan_issuers(stock_root: &Path) -> Vec<Issuer> {
    let mut issuers = Vec::new();
    for status in STATUSES {
        let base = stock_root.join(status);
        if !base.is_dir() {
            continue;
        }
        let mut candidates = Vec::new();
        for child in sorted_children(&base) {
            if !child.is_dir() || hidden_or_private(&child) {
                continue;
            }
            candidates.push(child.clone());
            if status == "archived" {
                for deeper in sorted_children(&child) {
                    if deeper.is_dir() && !hidden_or_private(&deeper) {
                        candidates.push(deeper);
                    }
                }
            }
        }
        for tdir in candidates {
            if tdir.join("_stock-status.md").exists() || tdir.join("thesis").is_dir() {
                issuers.push(Issuer {
                    ticker: file_name(&tdir).to_uppercase(),
                    status,
                    dir: tdir,
                });
            }
        }
    }
    issuers
}

fn read_dossier(build: &mut Build, tdir: &Path, file: &str) -> Result<String, String> {
    let path = tdir.join("thesis").join(file);
    match build.read(&path) {
        Ok(Some(t)) => Ok(t),
        Ok(None) => Err(format!("{file} tidak ditemukan")),
        Err(f) => Err(vault_read_error(&path, &f)),
    }
}

pub fn parse_lens(build: &mut Build, tdir: &Path) -> Value {
    let text = match read_dossier(build, tdir, "00-lens.md") {
        Ok(t) => t,
        Err(e) => return json!({"metrics": [], "locked": null, "approved_at": null, "error": e}),
    };
    let fm = frontmatter(Some(&text));
    let secs = sections(&text, 2);
    let metrics: Vec<Value> = drop_header(md_table_rows(section(&secs, "Metrik")), "metrik")
        .into_iter()
        .filter(|r| r.len() >= 5)
        .map(|r| json!({"metrik": r[0], "definisi": r[1], "alasan": r[2], "sumber_data": r[3], "field": r[4]}))
        .collect();
    json!({"metrics": metrics, "locked": fm_get(&fm, "status_terkunci"), "approved_at": fm_get(&fm, "disetujui_pada")})
}

fn sort_by_period(items: &mut [Value], field: &str) {
    items.sort_by_key(|o| period_key(o[field].as_str().unwrap_or("")));
}

pub fn parse_fundamental(build: &mut Build, tdir: &Path) -> Value {
    let text = match read_dossier(build, tdir, "10-fundamental.md") {
        Ok(t) => t,
        Err(e) => return json!({"series": {}, "doc_evidence": [], "needs_source": [], "error": e}),
    };
    let h2 = sections(&text, 2);

    let mut series = Map::new();
    for (h3, body) in sections(section(&h2, "Observasi kuartalan"), 3) {
        let name = strip(H3_NAME_SPLIT.splitn(&h3, 2).next().unwrap_or("")).to_string();
        let mut obs: Vec<Value> = drop_header(md_table_rows(&body), "periode")
            .into_iter()
            .filter(|r| r.len() >= 5)
            .map(|r| json!({"periode": r[0], "nilai": num(&r[1]), "raw": r[1], "unit": r[2], "basis": r[3], "sumber": r[4]}))
            .collect();
        sort_by_period(&mut obs, "periode");
        if !obs.is_empty() {
            series.insert(name, Value::Array(obs));
        }
    }

    let mut doc_evidence: Vec<Value> = drop_header(
        md_table_rows(section(&h2, "Observasi dari dokumen")),
        "metrik",
    )
    .into_iter()
    .filter(|r| r.len() >= 8)
    .map(|r| {
        json!({"metrik": r[0], "periode": r[1], "nilai": num(&r[2]), "raw": r[2], "unit": r[3],
                   "dokumen": r[4], "hal": r[5], "metode": r[6], "catatan": r[7]})
    })
    .collect();
    doc_evidence.sort_by(|a, b| {
        let key = |d: &Value| {
            (
                d["metrik"].as_str().unwrap_or("").to_string(),
                period_key(d["periode"].as_str().unwrap_or("")),
            )
        };
        key(a).cmp(&key(b))
    });

    let needs_source: Vec<Value> = drop_header(md_table_rows(section(&h2, "Metrik yang masih menunggu sumber")), "metrik")
        .into_iter()
        .filter(|r| r.len() >= 5)
        .map(|r| json!({"metrik": r[0], "definisi": r[1], "sumber_data": r[2], "status": r[3], "catatan": r[4]}))
        .collect();

    json!({"series": series, "doc_evidence": doc_evidence, "needs_source": needs_source})
}

pub fn parse_falsifier_block(body: &str) -> Vec<Value> {
    let mut items: Vec<(i64, String)> = Vec::new();
    for ln in splitlines(body) {
        if let Some(c) = FALSIFIER_ITEM.captures(ln) {
            items.push((c[1].parse().unwrap_or(0), strip(&c[2]).to_string()));
        } else if let Some(cur) = items.last_mut().filter(|_| !strip(ln).is_empty()) {
            cur.1 = strip(&format!("{} {}", cur.1, strip(ln))).to_string();
        }
    }
    items
        .into_iter()
        .map(|(n, text)| {
            let dates: Vec<&str> = ISO_DATE.find_iter(&text).map(|m| m.as_str()).collect();
            json!({"n": n, "text": text, "dates": dates})
        })
        .collect()
}

fn status_lines(body: &str) -> Map<String, Value> {
    let mut out: Map<String, Value> = STATUS_AXES
        .iter()
        .map(|a| (a.to_string(), Value::Null))
        .collect();
    for c in STATUS_LINE.captures_iter(body) {
        out.insert(c[1].to_lowercase(), json!(strip(&c[2])));
    }
    out
}

fn drop_status_paragraphs(body: &str) -> String {
    PARA_SPLIT
        .split(strip(body))
        .filter(|p| !STATUS_LINE.is_match(p))
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn parse_naratif(build: &mut Build, tdir: &Path) -> Value {
    let mut out = json!({"chain": [], "predictions": [], "verdict": null, "verdict_text": null, "falsifiers": [], "error": null});
    for (k, v) in status_lines("") {
        out[k] = v;
    }
    let text = match read_dossier(build, tdir, "20-naratif.md") {
        Ok(t) => t,
        Err(e) => {
            out["error"] = json!(e);
            return out;
        }
    };
    let h2 = sections(&text, 2);
    out["chain"] = drop_header(md_table_rows(section(&h2, "Mata rantai")), "pernyataan")
        .into_iter()
        .filter(|r| r.len() >= 3)
        .map(|r| json!({"pernyataan": r[0], "status": r[1], "bukti": r[2]}))
        .collect();
    out["predictions"] = drop_header(md_table_rows(section(&h2, "Prediksi")), "tanggal")
        .into_iter()
        .filter(|r| r.len() >= 4)
        .map(|r| json!({"tanggal": r[0], "prediksi": r[1], "status": r[2], "cara_cek": r[3]}))
        .collect();

    let verdict_body = section(&h2, "Verdict");
    if let Some(c) = BACKTICK.captures(verdict_body) {
        out["verdict"] = json!(strip(&c[1]));
    }
    for (k, v) in status_lines(verdict_body) {
        out[k] = v;
    }
    let rest = drop_status_paragraphs(verdict_body);
    let rest = strip(&rest);
    let first = PARA_SPLIT.splitn(rest, 2).next().unwrap_or("");
    out["verdict_text"] = if first.is_empty() {
        Value::Null
    } else {
        json!(strip(first))
    };
    out["falsifiers"] = Value::Array(parse_falsifier_block(section(&h2, "Falsifier")));
    out
}

fn trigger_list(key: &str, body: &str) -> Vec<String> {
    let re = FancyRegex::new(&format!(
        r"(?ms)^-\s*`?{key}(?:\[\])?`?\s*:\s*(.*?)(?=\n-\s*`|\n#|\z)"
    ))
    .expect("regex");
    let Some(c) = ffind(&re, body) else {
        return vec![];
    };
    let mut items: Vec<String> = Vec::new();
    let mut has_cur = false;
    for ln in splitlines(c.get(1).map(|m| m.as_str()).unwrap_or("")) {
        if let Some(m) = TRIGGER_ITEM.captures(ln) {
            let item = strip(&m[1]).to_string();
            has_cur = !item.is_empty();
            items.push(item);
        } else if has_cur && !strip(ln).is_empty() {
            let last = items.last_mut().expect("has current");
            *last = strip(&format!("{last} {}", strip(ln))).to_string();
        }
    }
    items
}

pub fn parse_thesis_md(tdir: &Path) -> Value {
    let Some(text) = read_or_none(&tdir.join("thesis").join("thesis.md")) else {
        return json!({"alasan_masuk": null, "falsifiers": [], "bayar_risiko": null, "harga_acuan": null, "trigger_tambah": [], "trigger_buang": []});
    };
    let alasan = ffind(&ALASAN, &text).map(|c| strip(&collapse_ws(&c[1])).to_string());
    let sec = sections(&text, 2);
    let fals = parse_falsifier_block(section(&sec, "Falsifier"));
    let kr_body = sec
        .iter()
        .find(|(k, _)| k.starts_with("Kerangka Risiko"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");

    let (mut bayar, mut harga, mut tambah, mut buang) = (Value::Null, Value::Null, vec![], vec![]);
    if !kr_body.is_empty() {
        if let Some(c) = ffind(&BAYAR, kr_body) {
            bayar = json!(strip(&collapse_ws(&c[1])));
        }
        if let Some(c) = ffind(&HARGA, kr_body) {
            let ha_raw = strip(&collapse_ws(&c[1])).to_string();
            let sumber = ffind(&SUMBER, &ha_raw)
                .map(|m| strip(strip(&m[1]).trim_end_matches('—')).to_string());
            let status = STATUS_HARGA
                .captures(&ha_raw)
                .map(|m| strip(&m[1]).to_lowercase());
            let nilai = strip(SUMBER_SPLIT.splitn(&ha_raw, 2).next().unwrap_or("")).to_string();
            harga = json!({"raw": ha_raw, "nilai": nilai, "sumber": sumber, "status": status});
        }
        tambah = trigger_list("trigger_tambah", kr_body);
        buang = trigger_list("trigger_buang", kr_body);
    }

    let tahap_caps = |c: &dyn Fn(usize) -> Option<String>| {
        (
            c(1).map(|t| strip(&t).to_lowercase()),
            c(2).map(|a| strip(&collapse_ws(&a)).to_string()),
        )
    };
    let (tahap, tahap_alasan) = if let Some(c) = ffind(&TAHAP, &text) {
        tahap_caps(&|i| c.get(i).map(|m| m.as_str().to_string()))
    } else if let Some(c) = TAHAP_FM.captures(&text) {
        tahap_caps(&|i| c.get(i).map(|m| m.as_str().to_string()))
    } else {
        (None, None)
    };

    json!({
        "alasan_masuk": alasan, "falsifiers": fals, "bayar_risiko": bayar, "harga_acuan": harga,
        "trigger_tambah": tambah, "trigger_buang": buang, "tahap": tahap, "tahap_alasan": tahap_alasan,
    })
}

pub fn parse_review_log(build: &mut Build, tdir: &Path) -> Value {
    let text = match read_dossier(build, tdir, "review-log.md") {
        Ok(t) => t,
        Err(e) => return json!({"entries": [], "error": e}),
    };
    let heads: Vec<_> = REVIEW_HEAD.captures_iter(&text).collect();
    let mut entries: Vec<Value> = Vec::new();
    for (i, c) in heads.iter().enumerate() {
        let start = c.get(0).expect("match").end();
        let end = heads
            .get(i + 1)
            .map(|n| n.get(0).expect("match").start())
            .unwrap_or(text.len());
        let title_raw = strip(&c[1]).to_string();
        let date = ISO_DATE.find(&title_raw).map(|m| m.as_str().to_string());
        let stripped = REVIEW_TITLE_DATE.replace(&title_raw, "");
        let title = if strip(&stripped).is_empty() {
            title_raw.clone()
        } else {
            strip(&stripped).to_string()
        };
        let mut bullets: Vec<(i64, String)> = Vec::new();
        for ln in splitlines(&text[start..end]) {
            if let Some(b) = BULLET.captures(ln) {
                let level = if b[1].chars().count() >= 2 { 1 } else { 0 };
                bullets.push((level, strip(&b[2]).to_string()));
            } else if let Some(cur) = bullets.last_mut() {
                let s = strip(ln);
                if !s.is_empty() && !s.starts_with('#') {
                    cur.1 = strip(&format!("{} {s}", cur.1)).to_string();
                }
            }
        }
        let bullets: Vec<Value> = bullets
            .into_iter()
            .map(|(l, t)| json!({"level": l, "text": t}))
            .collect();
        entries.push(
            json!({"date": date, "title": title, "title_raw": title_raw, "bullets": bullets}),
        );
    }
    entries.sort_by(|a, b| {
        let d = |e: &Value| e["date"].as_str().unwrap_or("0000-00-00").to_string();
        d(b).cmp(&d(a))
    });
    json!({"entries": entries})
}

/// `sorted(tdir.rglob("*"))`: every entry below `dir`, symlinked dirs not
/// followed, sorted component-wise like `PurePath` ordering.
fn rglob_all_sorted(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(it) = std::fs::read_dir(dir) else {
            return;
        };
        for e in it.flatten() {
            let p = e.path();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            out.push(p.clone());
            if is_dir {
                walk(&p, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

fn rel(p: &Path, base: &Path) -> String {
    p.strip_prefix(base)
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub struct Archive {
    pub pdf: usize,
    pub years: Map<String, Value>,
    pub last_retrieved_at: Option<String>,
    pub sidecar_index: Vec<Value>,
    pub errors: Vec<String>,
}

fn json_error_class(e: &serde_json::Error) -> &'static str {
    if e.is_io() {
        "OSError"
    } else {
        "JSONDecodeError"
    }
}

pub fn parse_archive(tdir: &Path) -> Archive {
    let (mut pdf, mut years, mut sidecars, mut errors) =
        (0usize, Vec::<(String, i64)>::new(), Vec::new(), Vec::new());
    let mut last: Option<PyDateTime> = None;
    for p in rglob_all_sorted(tdir) {
        if !p.is_file() {
            continue;
        }
        let name = file_name(&p);
        if name.ends_with(".source.json") {
            let parsed = match read_text(&p) {
                Ok(Some(t)) => serde_json::from_str::<Value>(&t)
                    .map_err(|e| ReadFailure::named(json_error_class(&e))),
                Ok(None) => Err(ReadFailure::named("FileNotFoundError")),
                Err(f) => Err(f),
            };
            let d = match parsed {
                Ok(d) => d,
                Err(f) => {
                    errors.push(format!("{}: {}", rel(&p, tdir), f.detail()));
                    continue;
                }
            };
            // `d.get("retrieved_at") or d.get("saved_at")`: the second value even when falsy.
            let dt_raw = d
                .get("retrieved_at")
                .filter(|v| truthy(v))
                .or_else(|| d.get("saved_at"))
                .cloned()
                .unwrap_or(Value::Null);
            if let Some(dt) = (!dt_raw.is_null())
                .then(|| iso_dt(&py_str(&dt_raw)))
                .flatten()
            {
                if last.is_none_or(|l| dt > l) {
                    last = Some(dt);
                }
            }
            let get = |k: &str| d.get(k).cloned().unwrap_or(Value::Null);
            sidecars.push(json!({
                "file": name, "period": get("period"), "fiscal_year": get("fiscal_year"),
                "document_type": get("document_type"), "retrieved_at": dt_raw, "date": dt_raw,
            }));
        } else if p
            .extension()
            .is_some_and(|e| e.to_string_lossy().to_lowercase() == "pdf")
        {
            pdf += 1;
            let r = p.strip_prefix(tdir).unwrap_or(&p);
            let first = r
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .unwrap_or_default();
            let year = if YEAR_DIR.is_match(&first) {
                first
            } else {
                "lainnya".into()
            };
            match years.iter_mut().find(|(y, _)| *y == year) {
                Some(e) => e.1 += 1,
                None => years.push((year, 1)),
            }
        }
    }
    years.sort_by(|a, b| a.0.cmp(&b.0));
    Archive {
        pdf,
        years: years.into_iter().map(|(y, n)| (y, json!(n))).collect(),
        last_retrieved_at: last.as_ref().map(isoformat),
        sidecar_index: sidecars,
        errors,
    }
}

/// `_norm_notes`.
pub fn norm_notes(v: Option<&Value>) -> Vec<String> {
    match v {
        None | Some(Value::Null) => vec![],
        Some(Value::String(s)) => Some(strip(s).to_string())
            .filter(|s| !s.is_empty())
            .into_iter()
            .collect(),
        Some(Value::Array(a)) => a
            .iter()
            .filter(|x| !x.is_null())
            .map(|x| strip(&py_str(x)).to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Some(other) => Some(strip(&py_str(other)).to_string())
            .filter(|s| !s.is_empty())
            .into_iter()
            .collect(),
    }
}

/// `Path.glob("financial_history_*.json")` in directory (readdir) order.
fn history_files(tdir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(tdir)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let n = file_name(p);
                    n.starts_with("financial_history_") && n.ends_with(".json")
                })
                .collect()
        })
        .unwrap_or_default()
}

fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&read_text(p).ok()??).ok()
}

pub struct History {
    pub source: Option<String>,
    pub integrity: Value,
    pub coverage: Value,
    pub missing_periods: Vec<String>,
    pub notes: Vec<String>,
}

/// `_parse_history`, reduced to the fields that reach the API.
pub fn parse_history(tdir: &Path) -> History {
    let mut best: Option<(String, Vec<Value>, Value)> = None;
    for p in history_files(tdir) {
        if !HISTORY_NAME.is_match(&file_name(&p)) {
            continue;
        }
        let Some(d) = read_json(&p) else { continue };
        let rows = d
            .get("rows")
            .filter(|r| truthy(r))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if rows.len() > best.as_ref().map(|b| b.1.len()).unwrap_or(0) {
            let meta = d
                .get("metadata")
                .filter(|m| truthy(m))
                .cloned()
                .unwrap_or(json!({}));
            best = Some((file_name(&p), rows, meta));
        }
    }
    let Some((source, rows, meta)) = best else {
        return History {
            source: None,
            integrity: json!({"status": "n/a", "max_abs_diff": null, "checked": 0}),
            coverage: Value::Null,
            missing_periods: vec![],
            notes: vec![],
        };
    };
    let missing_periods = norm_notes(meta.get("missing_periods"))
        .into_iter()
        .map(|mp| {
            if MISSING_PERIOD.is_match(strip(&mp)) {
                format!("periode {} tidak dapat dibaca/tidak dipublikasikan (daftar missing_periods vault)", strip(&mp))
            } else {
                mp
            }
        })
        .collect();
    History {
        source: Some(source),
        integrity: integrity(&rows),
        coverage: meta.get("coverage").cloned().unwrap_or(Value::Null),
        missing_periods,
        notes: norm_notes(meta.get("notes")),
    }
}

fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// `_integrity`: Q1..Q4 must add up to FY for every flow figure.
pub fn integrity(rows: &[Value]) -> Value {
    let mut by_year: std::collections::BTreeMap<i64, Map<String, Value>> = Default::default();
    for r in rows {
        let Some(y) = r
            .get("fiscal_year")
            .and_then(|y| y.as_i64().filter(|_| y.is_i64() || y.is_u64()))
        else {
            continue;
        };
        let p = py_str(r.get("period").unwrap_or(&Value::String(String::new())));
        if p == "FY" || Q_PERIOD.is_match(&p) {
            by_year.entry(y).or_default().insert(p, r.clone());
        }
    }
    let (mut checked, mut max_diff) = (0i64, 0.0f64);
    for d in by_year.values() {
        let Some(fy) = d.get("FY").and_then(Value::as_object) else {
            continue;
        };
        let qs: Vec<&Value> = ["Q1", "Q2", "Q3", "Q4"]
            .iter()
            .filter_map(|q| d.get(*q))
            .collect();
        if qs.len() != 4 {
            continue;
        }
        for (k, fyv) in fy {
            if k == "fiscal_year"
                || k == "reporting_months"
                || k.ends_with("page")
                || k.starts_with("cumulative_")
            {
                continue;
            }
            let Some(fyv) = as_number(fyv) else { continue };
            let vals: Option<Vec<f64>> = qs.iter().map(|q| q.get(k).and_then(as_number)).collect();
            let Some(vals) = vals else { continue };
            checked += 1;
            let diff = (py_fsum(&vals) - fyv).abs();
            if diff > max_diff {
                max_diff = diff;
            }
        }
    }
    if checked == 0 {
        return json!({"status": "n/a", "max_abs_diff": null, "checked": 0});
    }
    json!({"status": if max_diff < 1.0 { "ok" } else { "diff" }, "max_abs_diff": max_diff, "checked": checked})
}

/// `Path.rglob("*.md")` exactly as CPython 3.14 walks it: files of the root
/// first, then an explicit-stack DFS over readdir order.
fn rglob_md(root: &Path) -> Vec<PathBuf> {
    let list = |dir: &Path| -> Vec<(PathBuf, bool)> {
        std::fs::read_dir(dir)
            .map(|it| {
                it.flatten()
                    .map(|e| (e.path(), e.file_type().map(|t| t.is_dir()).unwrap_or(false)))
                    .collect()
            })
            .unwrap_or_default()
    };
    let md = |dir: &Path, out: &mut Vec<PathBuf>| {
        for (p, _) in list(dir) {
            if file_name(&p).ends_with(".md") {
                out.push(p);
            }
        }
    };
    let mut out = Vec::new();
    md(root, &mut out);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for (p, is_dir) in list(&dir) {
            if is_dir {
                md(&p, &mut out);
                stack.push(p);
            }
        }
    }
    out
}

/// (relative article path, date prefix of its file name).
type Mention = (String, Option<String>);
/// (document_type, sidecars).
type DocGroup = (String, Vec<Value>);

/// `_article_index`: word-boundary, case-sensitive ticker mentions.
pub fn article_index(article_bank: &Path, tickers: &[String]) -> Map<String, Value> {
    // MC keys a dict by ticker: duplicates collapse to one.
    let mut unique: Vec<String> = Vec::new();
    for t in tickers {
        if !unique.contains(t) {
            unique.push(t.clone());
        }
    }
    let tickers = &unique;
    let pats: Vec<(String, Regex)> = tickers
        .iter()
        .map(|t| {
            (
                t.clone(),
                Regex::new(&format!(r"\b{}\b", regex::escape(t))).expect("regex"),
            )
        })
        .collect();
    let mut found: Vec<(String, Vec<Mention>)> =
        tickers.iter().map(|t| (t.clone(), vec![])).collect();
    if article_bank.is_dir() {
        for p in rglob_md(article_bank) {
            let Some(text) = read_or_none(&p) else {
                continue;
            };
            let hit: Vec<&String> = pats
                .iter()
                .filter(|(_, rx)| rx.is_match(&text))
                .map(|(t, _)| t)
                .collect();
            if hit.is_empty() {
                continue;
            }
            let date = ARTICLE_DATE
                .captures(&file_name(&p))
                .map(|c| c[1].to_string());
            let name = rel(&p, article_bank);
            for t in hit {
                if let Some((_, items)) = found.iter_mut().find(|(k, _)| k == t) {
                    items.push((name.clone(), date.clone()));
                }
            }
        }
    }
    found
        .into_iter()
        .map(|(t, items)| {
            let mut dated: Vec<&(String, Option<String>)> =
                items.iter().filter(|(_, d)| d.is_some()).collect();
            dated.sort_by(|a, b| b.1.cmp(&a.1));
            let recent: Vec<Value> = dated
                .iter()
                .take(12)
                .map(|(n, d)| json!({"name": n, "date": d}))
                .collect();
            let v = json!({
                "mentions_approx": items.len(),
                "latest_date": dated.first().and_then(|(_, d)| d.clone()),
                "recent": recent,
            });
            (t, v)
        })
        .collect()
}

pub fn empty_articles() -> Value {
    json!({"mentions_approx": 0, "latest_date": null, "recent": []})
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn falsifier_clock(falsifiers: &[Value]) -> (Value, i64) {
    let today = today();
    let mut upcoming: Vec<(i64, Value)> = Vec::new();
    let mut past = 0;
    for f in falsifiers {
        for ds in f["dates"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let Ok(d) = NaiveDate::parse_from_str(ds, "%Y-%m-%d") else {
                continue;
            };
            if d >= today {
                let days = (d - today).num_days();
                upcoming.push((days, json!({"date": ds, "days_left": days, "text": f.get("text").cloned().unwrap_or(json!(""))})));
            } else {
                past += 1;
            }
        }
    }
    // `min()` keeps the first of equal keys.
    let nearest = upcoming
        .iter()
        .fold(None::<&(i64, Value)>, |acc, x| match acc {
            Some(a) if a.0 <= x.0 => Some(a),
            _ => Some(x),
        });
    (nearest.map(|n| n.1.clone()).unwrap_or(Value::Null), past)
}

/// Everything `_issuer_summary` / `_build_detail` need for one issuer.
pub struct Parsed {
    pub lens: Value,
    pub fund: Value,
    pub naratif: Value,
    pub thesis_md: Value,
    pub review: Value,
    pub archive: Archive,
    pub history: History,
}

pub fn parse_all(build: &mut Build, tdir: &Path) -> Parsed {
    // Same read order as MC's `_issuer_summary` (matters for the retry budget).
    let naratif = parse_naratif(build, tdir);
    let thesis_md = parse_thesis_md(tdir);
    let lens = parse_lens(build, tdir);
    let fund = parse_fundamental(build, tdir);
    let review = parse_review_log(build, tdir);
    let archive = parse_archive(tdir);
    let history = parse_history(tdir);
    Parsed {
        lens,
        fund,
        naratif,
        thesis_md,
        review,
        archive,
        history,
    }
}

fn falsifiers_of(p: &Parsed) -> Vec<Value> {
    let n = p.naratif["falsifiers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if n.is_empty() {
        p.thesis_md["falsifiers"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    } else {
        n
    }
}

pub fn issuer_summary(it: &Issuer, p: &Parsed, articles: Value) -> Value {
    let tdir = &it.dir;
    let fm = frontmatter(read_or_none(&tdir.join("_stock-status.md")).as_deref());
    let missing: Vec<&str> = DOSSIER_FILES
        .iter()
        .copied()
        .filter(|f| !tdir.join("thesis").join(f).exists())
        .collect();

    let falsifiers = falsifiers_of(p);
    let (nearest, past_count) = falsifier_clock(&falsifiers);
    let mut errors: Vec<Value> = [&p.naratif, &p.fund, &p.lens, &p.review]
        .iter()
        .filter_map(|s| s.get("error").filter(|e| truthy(e)).cloned())
        .collect();
    errors.extend(p.archive.errors.iter().map(|e| json!(e)));

    let series = p.fund["series"].as_object().cloned().unwrap_or_default();
    let doc_metrics: Vec<&str> = p.fund["doc_evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["metrik"].as_str())
        .collect();
    let metric_names: Vec<&str> = p.lens["metrics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["metrik"].as_str())
        .collect();
    let resolved = metric_names
        .iter()
        .filter(|m| series.get(**m).is_some_and(truthy))
        .count();
    let doc_quoted = metric_names
        .iter()
        .filter(|m| !series.contains_key(**m) && doc_metrics.contains(m))
        .count();
    let total = metric_names.len();
    let missing_metrics = total.saturating_sub(resolved + doc_quoted);

    let entries = p.review["entries"].as_array().cloned().unwrap_or_default();
    let last = entries.first();
    let last_date = last.and_then(|l| l["date"].as_str()).map(str::to_string);
    let open_followups = last
        .and_then(|l| l["bullets"].as_array())
        .into_iter()
        .flatten()
        .filter(|b| TINDAK_LANJUT.is_match(b["text"].as_str().unwrap_or("")))
        .count();

    let last_review_day = last_date
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let last_retrieved = p.archive.last_retrieved_at.as_deref().and_then(iso_dt);
    let (mut new_evidence, mut due) = (0usize, false);
    if let (Some(day), Some(_)) = (last_review_day, last_retrieved) {
        new_evidence = p
            .archive
            .sidecar_index
            .iter()
            .filter(|s| {
                (!s["retrieved_at"].is_null())
                    .then(|| iso_dt(&py_str(&s["retrieved_at"])))
                    .flatten()
                    .is_some_and(|d| d.date_naive() > day)
            })
            .count();
        due = new_evidence > 0;
    }
    let days_ago = last_review_day.map(|d| (today() - d).num_days());

    let naratif = &p.naratif;
    let axes: Map<String, Value> = STATUS_AXES
        .iter()
        .map(|a| (a.to_string(), naratif[*a].clone()))
        .collect();
    let fm_opt = |k: &str| fm_get(&fm, k).filter(|v| !v.is_empty());
    let tm = &p.thesis_md;
    let or_empty = |v: &Value| if truthy(v) { v.clone() } else { json!([]) };
    json!({
        "ticker": it.ticker,
        "company": fm_opt("company"),
        "status": it.status,
        "status_since": fm_opt("status_since"),
        "verdict": naratif["verdict"],
        "fundamental_status": naratif["fundamental_status"],
        "growth_status": naratif["growth_status"],
        "narrative_status": naratif["narrative_status"],
        "axes": axes,
        "complete": missing.is_empty(),
        "missing_files": missing,
        "falsifiers": {
            "count": falsifiers.len(),
            "dated_count": falsifiers.iter().filter(|f| truthy(&f["dates"])).count(),
            "nearest": nearest,
            "past_count": past_count,
        },
        "review": {
            "last_date": last_date, "days_ago": days_ago, "due": due,
            "new_evidence_count": new_evidence, "open_followups": open_followups,
        },
        "evidence": {"resolved": resolved, "doc_quoted": doc_quoted, "missing": missing_metrics, "total": total},
        "archive": {
            "pdf": p.archive.pdf, "sidecars": p.archive.sidecar_index.len(), "years": p.archive.years,
            "last_retrieved_at": p.archive.last_retrieved_at,
        },
        "articles": articles,
        "integrity": p.history.integrity,
        "history_source": p.history.source,
        "coverage": p.history.coverage,
        "missing_periods": p.history.missing_periods,
        "notes": p.history.notes,
        "tahap": tm.get("tahap").cloned().unwrap_or(Value::Null),
        "tahap_alasan": tm.get("tahap_alasan").cloned().unwrap_or(Value::Null),
        "bayar_risiko": tm["bayar_risiko"],
        "harga_acuan": tm["harga_acuan"],
        "trigger_tambah": or_empty(&tm["trigger_tambah"]),
        "trigger_buang": or_empty(&tm["trigger_buang"]),
        "errors": errors,
    })
}

/// `_public_series`.
fn public_series(series: &Value) -> Value {
    let mut out = Map::new();
    for (metric, obs) in series.as_object().into_iter().flatten() {
        if metric == "period_basis" {
            continue;
        }
        let points: Vec<Value> = obs
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| {
                let basis = if truthy(&p["basis"]) {
                    py_str(&p["basis"]).to_lowercase()
                } else {
                    String::new()
                };
                json!({
                    "periode": p.get("periode").cloned().unwrap_or(Value::Null),
                    "nilai": p.get("nilai").cloned().unwrap_or(Value::Null),
                    "unit": p.get("unit").cloned().unwrap_or(Value::Null),
                    "derived": basis.starts_with("derived"),
                    "is_cumulative": basis == "reported_ytd" || basis == "cumulative_ytd",
                })
            })
            .collect();
        out.insert(metric.clone(), Value::Array(points));
    }
    Value::Object(out)
}

/// `_build_detail` + `_public_thesis_detail`.
pub fn public_detail(it: &Issuer, p: &Parsed, summary: Value) -> Value {
    let mut timeline: Vec<(String, Value)> = Vec::new();
    if let Some(since) = summary["status_since"].as_str() {
        timeline.push((
            since.into(),
            json!({"type": "entry", "date": since, "label": format!("Tercatat {}", it.status)}),
        ));
    }
    for a in summary["articles"]["recent"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(d) = a["date"].as_str().filter(|d| !d.is_empty()) {
            timeline.push((
                d.into(),
                json!({"type": "article", "date": d, "label": a["name"]}),
            ));
        }
    }
    for e in p.review["entries"].as_array().into_iter().flatten() {
        if let Some(d) = e["date"].as_str().filter(|d| !d.is_empty()) {
            timeline.push((
                d.into(),
                json!({"type": "review", "date": d, "label": e["title"]}),
            ));
        }
    }
    if let Some(last) = p.archive.last_retrieved_at.as_deref() {
        if let Some(d) = iso_dt(last) {
            let day = d.date_naive().format("%Y-%m-%d").to_string();
            timeline.push((day.clone(), json!({"type": "archive", "date": day, "label": format!("Dokumen terbaru diambil · {} PDF", p.archive.pdf)})));
        }
    }
    timeline.sort_by(|a, b| a.0.cmp(&b.0));

    // fiscal_year → document_type → sidecars, each list sorted by retrieved_at.
    let mut years: Vec<(String, Vec<DocGroup>)> = Vec::new();
    for s in &p.archive.sidecar_index {
        let key = |k: &str| {
            if truthy(&s[k]) {
                py_str(&s[k])
            } else {
                "lainnya".to_string()
            }
        };
        let (y, t) = (key("fiscal_year"), key("document_type"));
        let yi = years.iter().position(|e| e.0 == y).unwrap_or_else(|| {
            years.push((y.clone(), vec![]));
            years.len() - 1
        });
        let types = &mut years[yi].1;
        match types.iter_mut().find(|e| e.0 == t) {
            Some(e) => e.1.push(s.clone()),
            None => types.push((t, vec![s.clone()])),
        }
    }
    let archive_detail: Map<String, Value> = years
        .into_iter()
        .map(|(y, types)| {
            let types: Map<String, Value> = types
                .into_iter()
                .map(|(t, mut items)| {
                    let k = |s: &Value| {
                        if truthy(&s["retrieved_at"]) {
                            py_str(&s["retrieved_at"])
                        } else {
                            String::new()
                        }
                    };
                    items.sort_by_key(k);
                    (t, Value::Array(items))
                })
                .collect();
            (y, Value::Object(types))
        })
        .collect();

    let mut out = summary.as_object().cloned().unwrap_or_default();
    let tm = &p.thesis_md;
    let or_empty = |v: &Value| if truthy(v) { v.clone() } else { json!([]) };
    let extra = json!({
        "alasan_masuk": tm["alasan_masuk"],
        "lens": {"metrics": or_empty(&p.lens["metrics"]), "locked": p.lens["locked"], "approved_at": p.lens["approved_at"]},
        "series": public_series(&p.fund["series"]),
        "doc_evidence": or_empty(&p.fund["doc_evidence"]),
        "needs_source": or_empty(&p.fund["needs_source"]),
        "chain": or_empty(&p.naratif["chain"]),
        "predictions": or_empty(&p.naratif["predictions"]),
        "verdict_text": p.naratif["verdict_text"],
        "falsifier_items": falsifiers_of(p),
        "review_log": or_empty(&p.review["entries"]),
        "archive_detail": archive_detail,
        "timeline": timeline.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
    });
    out.extend(extra.as_object().cloned().unwrap_or_default());
    Value::Object(out)
}

/// Article lookup for a ticker list (cached by the caller, MC: 600 s).
pub type Articles<'a> = &'a dyn Fn(&[String]) -> Map<String, Value>;

pub fn summary(stock_root: &Path, articles: Articles, now_unix: u64) -> Value {
    if !stock_root.is_dir() {
        return json!({"root_ok": false, "generated_at": now_unix, "issuers": []});
    }
    let mut build = Build::default();
    let issuers = scan_issuers(stock_root);
    let tickers: Vec<String> = issuers.iter().map(|i| i.ticker.clone()).collect();
    let articles = articles(&tickers);
    let out: Vec<Value> = issuers
        .iter()
        .map(|it| {
            let parsed = parse_all(&mut build, &it.dir);
            issuer_summary(
                it,
                &parsed,
                articles
                    .get(&it.ticker)
                    .cloned()
                    .unwrap_or_else(empty_articles),
            )
        })
        .collect();
    json!({"root_ok": true, "generated_at": now_unix, "issuers": out})
}

pub fn detail(stock_root: &Path, articles: Articles, ticker: &str) -> Option<Value> {
    let mut build = Build::default();
    let issuers = scan_issuers(stock_root);
    let it = issuers.iter().find(|i| i.ticker == ticker)?;
    let parsed = parse_all(&mut build, &it.dir);
    let articles = articles(&[ticker.to_string()]);
    let summary = issuer_summary(
        it,
        &parsed,
        articles.get(ticker).cloned().unwrap_or_else(empty_articles),
    );
    Some(public_detail(it, &parsed, summary))
}

/// `_has_read_error`: payloads carrying a vault read failure are not cached.
pub fn has_read_error(v: &Value) -> bool {
    match v {
        Value::Object(o) => {
            let direct = o
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errs| errs.iter().any(|e| py_str(e).contains(py::READ_ERROR_MARK)));
            direct || o.values().any(has_read_error)
        }
        Value::Array(a) => a.iter().any(has_read_error),
        _ => false,
    }
}
