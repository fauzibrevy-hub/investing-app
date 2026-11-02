//! Pack loading (research plan §4.3, §9): `research/packs/*.toml` from
//! `INVESTING_RESEARCH_PACKS_DIR` at start, hot reload on request, and the canon packs
//! embedded at build time as the fallback for a missing directory or a broken file.
//!
//! A directory file that fails validation is reported (health, `GET /packs`) and the
//! embedded pack of the same id serves in its place, so one typo never empties the Riset tab.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::Serialize;

use super::def::{Pack, PackError, PackSet};

/// The canon v1 packs, embedded so the binary serves them without the repo checkout.
pub const BUILTIN: &[(&str, &str)] = &[
    (
        "core.toml",
        include_str!("../../../research/packs/core.toml"),
    ),
    (
        "bank_sharia.toml",
        include_str!("../../../research/packs/bank_sharia.toml"),
    ),
    (
        "coal_mining.toml",
        include_str!("../../../research/packs/coal_mining.toml"),
    ),
    (
        "heavy_equipment_contracting.toml",
        include_str!("../../../research/packs/heavy_equipment_contracting.toml"),
    ),
    (
        "shipping.toml",
        include_str!("../../../research/packs/shipping.toml"),
    ),
    (
        "shipping_tanker.toml",
        include_str!("../../../research/packs/shipping_tanker.toml"),
    ),
    (
        "it_distribution.toml",
        include_str!("../../../research/packs/it_distribution.toml"),
    ),
    (
        "media_scma.toml",
        include_str!("../../../research/packs/media_scma.toml"),
    ),
];

/// What the last load produced.
#[derive(Clone, Debug, Serialize)]
pub struct Loaded {
    #[serde(skip)]
    pub set: Arc<PackSet>,
    /// `embedded`, or the directory read.
    pub source: String,
    /// Files read from the directory.
    pub files: usize,
    /// `(file, error)` for every directory file that did not validate.
    pub invalid: Vec<(String, String)>,
    /// Pack ids served from the embedded copy because the directory had none or a broken one.
    pub embedded_ids: Vec<String>,
    pub loaded_at: String,
}

impl Loaded {
    pub fn ok(&self) -> bool {
        self.invalid.is_empty()
    }
}

/// Parses the embedded packs; a failure here is a build error, not a runtime one.
pub fn embedded_packs() -> Vec<Pack> {
    BUILTIN
        .iter()
        .map(|(name, text)| {
            Pack::parse(text).unwrap_or_else(|e| panic!("embedded pack {name} invalid: {e}"))
        })
        .collect()
}

/// `(file name, parse result)` per directory file.
type ReadFile = (String, Result<Pack, PackError>);

fn read_dir(dir: &Path) -> (Vec<ReadFile>, usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (Vec::new(), 0);
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    let n = files.len();
    let parsed = files
        .into_iter()
        .map(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let result = std::fs::read_to_string(&p)
                .map_err(|e| {
                    PackError::Invalid(super::def::ValidationError(vec![format!("read: {e}")]))
                })
                .and_then(|t| Pack::parse(&t));
            (name, result)
        })
        .collect();
    (parsed, n)
}

fn load_from(dir: &Path, now: &str) -> Loaded {
    let embedded = embedded_packs();
    let (parsed, files) = read_dir(dir);
    let mut invalid = Vec::new();
    let mut packs: Vec<Pack> = Vec::new();
    for (name, result) in parsed {
        match result {
            Ok(p) => packs.push(p),
            Err(e) => invalid.push((name, e.to_string())),
        }
    }
    let mut embedded_ids = Vec::new();
    for e in &embedded {
        if !packs.iter().any(|p| p.id == e.id) {
            embedded_ids.push(e.id.clone());
            packs.push(e.clone());
        }
    }
    let set = match PackSet::new(packs) {
        Ok(s) => s,
        Err(e) => {
            invalid.push(("<set>".into(), e.to_string()));
            embedded_ids = embedded.iter().map(|p| p.id.clone()).collect();
            PackSet::new(embedded).expect("embedded packs form a valid set")
        }
    };
    let source = if files == 0 {
        "embedded".to_string()
    } else {
        dir.display().to_string()
    };
    Loaded {
        set: Arc::new(set),
        source,
        files,
        invalid,
        embedded_ids,
        loaded_at: now.to_string(),
    }
}

/// The loaded packs, shared by the API and health; `reload` re-reads the directory.
pub struct Packs {
    dir: PathBuf,
    state: RwLock<Loaded>,
}

impl Packs {
    pub fn load(dir: &Path, now: &str) -> Self {
        Self {
            dir: dir.to_path_buf(),
            state: RwLock::new(load_from(dir, now)),
        }
    }

    /// Only the embedded packs (tests, tooling).
    pub fn embedded() -> Self {
        Self::load(Path::new(""), "")
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn reload(&self, now: &str) -> Loaded {
        let loaded = load_from(&self.dir, now);
        *self.state.write().unwrap_or_else(|p| p.into_inner()) = loaded.clone();
        loaded
    }

    pub fn set(&self) -> Arc<PackSet> {
        self.state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .set
            .clone()
    }

    pub fn status(&self) -> Loaded {
        self.state.read().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_packs_are_valid_and_cover_the_canon_industries() {
        let p = Packs::embedded();
        let set = p.set();
        let mut ids: Vec<&str> = set.ids().collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            [
                "bank_sharia",
                "coal_mining",
                "core",
                "heavy_equipment_contracting",
                "it_distribution",
                "media_scma",
                "shipping",
                "shipping_tanker"
            ]
        );
        assert!(set.get("media_scma").unwrap().industry_codes.is_empty());
        assert_eq!(set.for_industry_code("G111").unwrap().id, "bank_sharia");
        assert_eq!(set.for_industry_code("A121").unwrap().id, "coal_mining");
        assert_eq!(
            set.for_industry_code("C141").unwrap().id,
            "heavy_equipment_contracting"
        );
        assert_eq!(set.for_industry_code("A112").unwrap().id, "shipping");
        assert_eq!(set.for_industry_code("I211").unwrap().id, "it_distribution");
        assert!(set.for_industry_code("Z999").is_none());
        assert_eq!(p.status().source, "embedded");
    }

    #[test]
    fn a_broken_directory_file_is_reported_and_replaced_by_the_embedded_pack() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shipping.toml"), "id = \"shipping\"\n").unwrap();
        std::fs::write(
            dir.path().join("core.toml"),
            BUILTIN.iter().find(|(n, _)| *n == "core.toml").unwrap().1,
        )
        .unwrap();
        let p = Packs::load(dir.path(), "2026-10-24T00:00:00Z");
        let s = p.status();
        assert_eq!(s.files, 2);
        assert_eq!(s.invalid.len(), 1);
        assert_eq!(s.invalid[0].0, "shipping.toml");
        assert!(s.embedded_ids.contains(&"shipping".to_string()));
        assert!(!s.embedded_ids.contains(&"core".to_string()));
        assert!(s.set.get("shipping").is_some());
        // Fixing the file and reloading clears the report without a restart.
        std::fs::write(
            dir.path().join("shipping.toml"),
            BUILTIN
                .iter()
                .find(|(n, _)| *n == "shipping.toml")
                .unwrap()
                .1,
        )
        .unwrap();
        let s = p.reload("2026-10-24T00:01:00Z");
        assert!(s.invalid.is_empty());
        assert!(!s.embedded_ids.contains(&"shipping".to_string()));
    }
}
