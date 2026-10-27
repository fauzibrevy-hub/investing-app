//! Thesis drafts: the app-owned JSON store (`INVESTING_THESIS_DRAFTS_PATH`, same
//! file format as MC `data/thesis_drafts.json`) plus read-only vault theses
//! from `finance/thesis/*.md`. Ported from MC `thesis_bank.py`.

use std::path::Path;
use std::sync::LazyLock;

use fancy_regex::Regex as FancyRegex;
use serde_json::{json, Map, Value};

use crate::error::{validation_entry, ApiError};
use crate::py::{read_text, splitlines, strip};
use axum::http::StatusCode;

/// Load the store; a missing file is an empty store.
pub fn load(path: &Path) -> Result<Map<String, Value>, ApiError> {
    let unavailable = || {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Drafts store corrupted or unavailable",
        )
    };
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(m)) => Ok(m),
            _ => {
                tracing::error!("failed to parse thesis drafts store");
                Err(unavailable())
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => {
            tracing::error!("cannot read thesis drafts store: {e}");
            Err(unavailable())
        }
    }
}

/// Atomic save: temp file in the same directory, fsync, rename (plan.md R3).
/// Same formatting as MC (`json.dumps(indent=2, ensure_ascii=False)`).
pub fn save(path: &Path, store: &Map<String, Value>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(
            serde_json::to_string_pretty(store)
                .expect("serializable")
                .as_bytes(),
        )?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

fn string_field(
    body: &Map<String, Value>,
    name: &str,
    default: Option<&str>,
    errors: &mut Vec<Value>,
) -> Option<String> {
    match body.get(name) {
        None => match default {
            Some(d) => Some(d.to_string()),
            None => {
                errors.push(validation_entry(
                    &["body", name],
                    "missing",
                    "Field required",
                    Value::Object(body.clone()),
                ));
                None
            }
        },
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Null) if name == "notes" => None,
        Some(other) => {
            errors.push(validation_entry(
                &["body", name],
                "string_type",
                "Input should be a valid string",
                other.clone(),
            ));
            None
        }
    }
}

fn list_field(body: &Map<String, Value>, name: &str, errors: &mut Vec<Value>) -> Vec<String> {
    match body.get(name) {
        None => vec![],
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .filter_map(|(i, v)| match v {
                Value::String(s) => Some(s.clone()),
                other => {
                    let mut e = validation_entry(
                        &["body", name],
                        "string_type",
                        "Input should be a valid string",
                        other.clone(),
                    );
                    e["loc"] = json!(["body", name, i]);
                    errors.push(e);
                    None
                }
            })
            .collect(),
        Some(other) => {
            errors.push(validation_entry(
                &["body", name],
                "list_type",
                "Input should be a valid list",
                other.clone(),
            ));
            vec![]
        }
    }
}

fn bool_field(body: &Map<String, Value>, name: &str, errors: &mut Vec<Value>) -> bool {
    let parsed = match body.get(name) {
        None => Some(false),
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::Number(n)) => match n.as_f64() {
            Some(0.0) => Some(false),
            Some(1.0) => Some(true),
            _ => None,
        },
        Some(Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "1" | "on" | "t" | "true" | "y" | "yes" => Some(true),
            "0" | "off" | "f" | "false" | "n" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    };
    parsed.unwrap_or_else(|| {
        let input = body.get(name).cloned().unwrap_or(Value::Null);
        errors.push(validation_entry(
            &["body", name],
            "bool_parsing",
            "Input should be a valid boolean, unable to interpret input",
            input,
        ));
        false
    })
}

static TICKER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[A-Z0-9\-_]{2,10}$").expect("regex"));

/// `ThesisDraftCreate` validation + `create_thesis_draft` normalization.
/// `now_unix`/`now_iso`/`suffix` are injected for deterministic tests.
pub fn build_draft(
    body: &Value,
    now_unix: u64,
    now_iso: &str,
    suffix: &str,
) -> Result<Value, ApiError> {
    let Some(obj) = body.as_object() else {
        return Err(ApiError::invalid(
            &["body"],
            "model_attributes_type",
            "Input should be a valid dictionary or object to extract fields from",
            body.clone(),
        ));
    };
    let mut errors = Vec::new();
    let ticker = string_field(obj, "ticker", None, &mut errors);
    let title = string_field(obj, "title", None, &mut errors);
    let statement = string_field(obj, "thesis_statement", None, &mut errors);
    let thesis_type = string_field(obj, "thesis_type", Some("fundamental"), &mut errors);
    let is_primary = bool_field(obj, "is_primary", &mut errors);
    let scenario = string_field(obj, "scenario_initial", Some("Menunggu"), &mut errors);
    let story = string_field(
        obj,
        "story_status_initial",
        Some("Belum terkonfirmasi"),
        &mut errors,
    );
    let falsifiers = list_field(obj, "falsifiers", &mut errors);
    let triggers_add = list_field(obj, "triggers_add", &mut errors);
    let triggers_exit = list_field(obj, "triggers_exit", &mut errors);
    let notes = string_field(obj, "notes", Some(""), &mut errors);
    if !errors.is_empty() {
        return Err(ApiError::Validation(errors));
    }

    let unprocessable = |msg: &str| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, msg);
    let ticker = strip(&ticker.unwrap_or_default()).to_uppercase();
    let title = strip(&title.unwrap_or_default()).to_string();
    let statement = strip(&statement.unwrap_or_default()).to_string();
    if ticker.is_empty() {
        return Err(unprocessable("Ticker emiten wajib diisi."));
    }
    if !TICKER.is_match(&ticker) {
        return Err(unprocessable(
            "Format ticker emiten tidak valid (2-10 karakter alfanumerik).",
        ));
    }
    if title.is_empty() {
        return Err(unprocessable("Judul tesis wajib diisi."));
    }
    if statement.is_empty() {
        return Err(unprocessable(
            "Inti alasan investasi (thesis statement) wajib diisi.",
        ));
    }

    let ttype = match thesis_type.as_deref() {
        Some(t) if !t.is_empty() => strip(&t.to_lowercase()).to_string(),
        _ => "fundamental".into(),
    };
    let ttype = if ["fundamental", "narrative", "hybrid"].contains(&ttype.as_str()) {
        ttype
    } else {
        "fundamental".into()
    };
    let scenario = scenario
        .filter(|s| ["Menunggu", "Berjalan", "Melemah", "Gugur", "Tercapai"].contains(&s.as_str()))
        .unwrap_or_else(|| "Menunggu".into());
    let valid_story = [
        "Belum terkonfirmasi",
        "Terdukung sebagian",
        "Terkonfirmasi",
        "Terbantahkan",
        "Tidak berlaku",
    ];
    let story = story
        .filter(|s| valid_story.contains(&s.as_str()))
        .unwrap_or_else(|| "Belum terkonfirmasi".into());
    let clean = |v: Vec<String>| {
        v.iter()
            .map(|s| strip(s).to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    };

    Ok(json!({
        "id": format!("draft_{}_{now_unix}_{suffix}", ticker.to_lowercase()),
        "ticker": ticker,
        "title": title,
        "thesis_statement": statement,
        "thesis_type": ttype,
        "is_primary": is_primary,
        "version": 1,
        "status": "draft",
        "scenario_status": scenario,
        "story_status": story,
        "falsifiers": clean(falsifiers),
        "triggers_add": clean(triggers_add),
        "triggers_exit": clean(triggers_exit),
        "notes": notes.map(|n| strip(&n).to_string()).unwrap_or_default(),
        "created_at": now_iso,
        "updated_at": now_iso,
    }))
}

// ---- read-only vault theses (`finance/thesis/*.md`) ----

/// `_parse_markdown_tables`: each table (≥ 3 lines) as rows of header → cell.
fn markdown_tables(text: &str) -> Vec<Vec<Map<String, Value>>> {
    let mut tables: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in splitlines(text) {
        let s = strip(line);
        if s.starts_with('|') && s.ends_with('|') {
            current.push(s.to_string());
        } else {
            if current.len() >= 3 {
                tables.push(std::mem::take(&mut current));
            }
            current.clear();
        }
    }
    if current.len() >= 3 {
        tables.push(current);
    }
    let cells = |l: &str| {
        l.trim_matches('|')
            .split('|')
            .map(|c| strip(c).to_string())
            .collect::<Vec<_>>()
    };
    tables
        .iter()
        .map(|tbl| {
            let headers = cells(&tbl[0]);
            tbl[2..]
                .iter()
                .map(|row| {
                    let mut cols = cells(row);
                    cols.resize(cols.len().max(headers.len()), String::new());
                    let mut m = Map::new();
                    for (h, c) in headers.iter().zip(cols) {
                        m.insert(h.clone(), json!(c));
                    }
                    m
                })
                .collect()
        })
        .collect()
}

fn fancy(pat: &str) -> FancyRegex {
    FancyRegex::new(pat).expect("regex")
}

/// `_extract_md_section`.
fn md_section(md: &str, header: &str) -> String {
    let re = fancy(&format!(
        r"(?s)##\s+{}.*?\n(.*?)(?=\n##\s|\z)",
        fancy_regex::escape(header)
    ));
    re.captures(md)
        .ok()
        .flatten()
        .and_then(|c| c.get(1).map(|m| strip(m.as_str()).to_string()))
        .unwrap_or_default()
}

fn dash_list(sec: &str) -> Vec<String> {
    splitlines(sec)
        .into_iter()
        .filter(|l| strip(l).starts_with('-'))
        .map(|l| strip(strip(l).trim_start_matches(['-', ' '])).to_string())
        .collect()
}

const BLOK1: &str = "Blok 1 — Supply–demand global (bukan harga saja)";
const BLOK2: &str = "Blok 2 — Cadangan dan buffer (SPR ≠ stok pasar)";
const BLOK3: &str = "Blok 3 — Transmisi Indonesia → emiten (universe layer disetujui)";

fn first_table(tables: &[Vec<Map<String, Value>>], i: usize) -> Value {
    tables.get(i).map(|t| json!(t)).unwrap_or(json!([]))
}

pub fn parse_vault_thesis(path: &Path) -> Option<Value> {
    let content = read_text(path).ok()??;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut ident: Vec<(String, String)> = Vec::new();
    if let Some(table) = markdown_tables(&md_section(&content, "Identitas")).first() {
        for r in table {
            let k = strip(r.get("Field").and_then(Value::as_str).unwrap_or("")).to_string();
            let v = strip(r.get("Nilai").and_then(Value::as_str).unwrap_or("")).to_string();
            match ident.iter_mut().find(|(key, _)| *key == k) {
                Some(e) => e.1 = v,
                None => ident.push((k, v)),
            }
        }
    }
    let get = |k: &str, default: &str| {
        ident
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| default.to_string())
    };

    let blok3_sec = md_section(&content, BLOK3);
    let blok3 = markdown_tables(&blok3_sec);
    let gap = fancy(r"(?s)Gap berlabel\s+`belum terverifikasi`.*?\n(.*?)(?=\n\n|\nBacaan|\n#|\z)")
        .find(&blok3_sec)
        .ok()
        .flatten()
        .or_else(|| {
            fancy(r"(?s)Gap berlabel\s+`belum terverifikasi`.*?(?=\n\n|\nBacaan|\n#|\z)")
                .find(&blok3_sec)
                .ok()
                .flatten()
        })
        .map(|m| strip(m.as_str()).to_string())
        .unwrap_or_default();
    let bacaan = fancy(r"(?s)Bacaan jujur:\s*(.*?)(?=\n\n|\n##|\z)")
        .captures(&blok3_sec)
        .ok()
        .flatten()
        .and_then(|c| c.get(1).map(|m| strip(m.as_str()).to_string()))
        .unwrap_or_default();
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    Some(json!({
        "id": get("ID", &stem),
        "title": get("Judul", &stem),
        "status_lifecycle": get("Status lifecycle", "Menunggu (HOLD)"),
        "evidence_label": get("Evidence label", "unproven"),
        "relation": get("Relasi ke tesis map", ""),
        "kamus_status": get("Kamus status", "Menunggu / Berjalan / Melemah / Gugur / Tercapai"),
        "tahap": "Menunggu",
        "is_readonly": true,
        "is_active": false,
        "has_exploration_claim": false,
        "thesis_statement": md_section(&content, "Tesis (satu kalimat)"),
        "causal_chain": md_section(&content, "Rantai kausal"),
        "blok_1": {"title": BLOK1, "parameters": first_table(&markdown_tables(&md_section(&content, BLOK1)), 0)},
        "blok_2": {"title": BLOK2, "parameters": first_table(&markdown_tables(&md_section(&content, BLOK2)), 0)},
        "blok_3": {
            "title": BLOK3,
            "layers": first_table(&blok3, 0),
            "observasi": first_table(&blok3, 1),
            "gaps": gap,
            "bacaan_jujur": bacaan,
            "label": "jasa-terbukti / eksplorasi-belum",
        },
        "falsifiers": first_table(&markdown_tables(&md_section(&content, "Falsifier bernomor")), 0),
        "guards": dash_list(&md_section(&content, "Guard")),
        "riwayat": dash_list(&md_section(&content, "Riwayat")),
        "tugas_tertunda": dash_list(&md_section(&content, "Tugas tertunda eksplisit (pending primer)")),
        "file_info": {"name": name, "size_bytes": size, "path": format!("finance/thesis/{name}")},
    }))
}

pub fn vault_theses(thesis_dir: &Path) -> Vec<Value> {
    if !thesis_dir.is_dir() {
        return vec![];
    }
    let mut files: Vec<_> = std::fs::read_dir(thesis_dir)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().ends_with(".md"))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
        .iter()
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .filter_map(|p| parse_vault_thesis(p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_validation_and_normalization() {
        let d = build_draft(
            &json!({"ticker": " bbca ", "title": " T ", "thesis_statement": "S", "thesis_type": "HYBRID", "scenario_initial": "Nope",
                    "falsifiers": [" a ", " "], "is_primary": "yes", "notes": null}),
            1700000000,
            "2026-10-20T00:00:00+00:00",
            "abcdef",
        )
        .unwrap();
        assert_eq!(d["id"], "draft_bbca_1700000000_abcdef");
        assert_eq!(
            (d["ticker"].as_str(), d["title"].as_str()),
            (Some("BBCA"), Some("T"))
        );
        assert_eq!(d["thesis_type"], "hybrid");
        assert_eq!(d["scenario_status"], "Menunggu");
        assert_eq!(d["story_status"], "Belum terkonfirmasi");
        assert_eq!(d["falsifiers"], json!(["a"]));
        assert_eq!(d["is_primary"], true);
        assert_eq!(d["notes"], "");

        let err = |b: Value| match build_draft(&b, 0, "", "x").unwrap_err() {
            ApiError::Http(s, m) => (s.as_u16(), m),
            ApiError::Validation(v) => (422, v[0]["loc"][1].as_str().unwrap().to_string()),
        };
        assert_eq!(
            err(json!({"title": "t", "thesis_statement": "s"})),
            (422, "ticker".into())
        );
        assert_eq!(
            err(json!({"ticker": "B", "title": "t", "thesis_statement": "s"})),
            (
                422,
                "Format ticker emiten tidak valid (2-10 karakter alfanumerik).".into()
            )
        );
        assert_eq!(
            err(json!({"ticker": "BBCA", "title": " ", "thesis_statement": "s"})),
            (422, "Judul tesis wajib diisi.".into())
        );
        assert_eq!(
            err(json!({"ticker": "  ", "title": "t", "thesis_statement": "s"})),
            (422, "Ticker emiten wajib diisi.".into())
        );
    }

    #[test]
    fn save_is_atomic_and_mc_formatted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/thesis_drafts.json");
        let mut store = Map::new();
        store.insert("b".into(), json!({"x": "é", "l": []}));
        store.insert("a".into(), json!({}));
        save(&path, &store).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"b\": {\n    \"x\": \"é\",\n    \"l\": []\n  },\n  \"a\": {}\n}"
        );
        assert!(!path.with_extension("json.tmp").exists());
        assert_eq!(load(&path).unwrap().keys().collect::<Vec<_>>(), ["b", "a"]);
        std::fs::write(&path, "[1]").unwrap();
        assert!(load(&path).is_err());
        assert!(load(&dir.path().join("missing.json")).unwrap().is_empty());
    }

    #[test]
    fn vault_thesis_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let md = "# X\n\n## Identitas\n\n| Field | Nilai |\n|---|---|\n| ID | T-01 |\n| Judul | Minyak |\n\n## Tesis (satu kalimat)\nHarga naik.\n\n## Guard\n- satu\n -- dua\nbukan\n\n## Falsifier bernomor\n| # | Falsifier |\n|---|---|\n| 1 | a |\n";
        let p = dir.path().join("t01.md");
        std::fs::write(&p, md).unwrap();
        let v = parse_vault_thesis(&p).unwrap();
        assert_eq!(v["id"], "T-01");
        assert_eq!(v["title"], "Minyak");
        assert_eq!(v["status_lifecycle"], "Menunggu (HOLD)");
        assert_eq!(v["thesis_statement"], "Harga naik.");
        assert_eq!(v["guards"], json!(["satu", "dua"]));
        assert_eq!(v["falsifiers"], json!([{"#": "1", "Falsifier": "a"}]));
        assert_eq!(v["blok_1"]["parameters"], json!([]));
        assert_eq!(v["file_info"]["path"], "finance/thesis/t01.md");
    }
}
