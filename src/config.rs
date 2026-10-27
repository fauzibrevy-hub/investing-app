//! Process configuration, read once from the environment (prd.md §5, §6).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub const MIN_TOKEN_LEN: usize = 32;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigError {
    #[error("INVESTING_APP_TOKEN is required and must be at least {MIN_TOKEN_LEN} characters")]
    Token,
    #[error("{0} is required")]
    Missing(&'static str),
    #[error("INVESTING_BIND is not a valid socket address: {0}")]
    Bind(String),
}

#[derive(Clone)]
pub struct Config {
    pub token: String,
    pub bind: SocketAddr,
    pub portfolio_spreadsheet_id: String,
    pub google_credentials_path: PathBuf,
    pub data_dir: PathBuf,
    pub web_dist: PathBuf,
    pub vault_root: PathBuf,
    pub thesis_drafts_path: PathBuf,
    pub idx_db_path: PathBuf,
    pub macro_store_path: PathBuf,
    pub macro_disable_fred: bool,
    /// Background macro ingestion every 60 s (MC `run_macro_release_watcher`).
    pub macro_watcher: bool,
    /// App-owned research store (`research.sqlite`).
    pub research_db_path: PathBuf,
    /// Watchlist CSV the scraper job reads, rewritten from the research universe.
    pub research_scraper_watchlist: PathBuf,
    /// Scraper master data (`taxonomy_type` per ticker), read-only.
    pub idx_emitens_csv: PathBuf,
    /// Scraper checkout; the launchd job runs from here.
    pub idx_scraper_dir: PathBuf,
    /// Raw `.xbrl` filings written by the scraper, read-only (research plan §4.2, §9).
    pub idx_xbrl_dir: PathBuf,
    /// Original filings read from a PDF, one JSON file each, beside the XBRL source
    /// (`INVESTING_PDF_FILINGS_DIR`, default `<data dir>/research/pdf-filings`). Optional:
    /// a missing directory is an empty source.
    pub research_pdf_filings_dir: PathBuf,
    /// Pack TOML files (research plan §4.3, §9); the built-in packs are used when missing.
    pub research_packs_dir: PathBuf,
    /// Uploaded PDF and PPTX files, `<sha256>.<ext>` (research plan R8).
    pub research_documents_dir: PathBuf,
    pub research_disable_scheduler: bool,
}

// Never print the token.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("bind", &self.bind)
            .field("data_dir", &self.data_dir)
            .field("web_dist", &self.web_dist)
            .field("vault_root", &self.vault_root)
            .field("thesis_drafts_path", &self.thesis_drafts_path)
            .field("idx_db_path", &self.idx_db_path)
            .field("macro_store_path", &self.macro_store_path)
            .finish_non_exhaustive()
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Testable constructor: `get` returns the raw value of a variable.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        // Resolved before the `var` closure borrows `get`.
        let research_db_path = research_db_path(&get);
        let var = |k: &str| {
            get(k)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };

        let token = var("INVESTING_APP_TOKEN").ok_or(ConfigError::Token)?;
        if token.chars().count() < MIN_TOKEN_LEN {
            return Err(ConfigError::Token);
        }
        let bind_raw = var("INVESTING_BIND").unwrap_or_else(|| "127.0.0.1:8789".into());
        let bind = bind_raw.parse().map_err(|_| ConfigError::Bind(bind_raw))?;

        let home = var("HOME").map(PathBuf::from).unwrap_or_default();
        let path =
            |k: &str, default: PathBuf| var(k).map(|v| expand_home(&v, &home)).unwrap_or(default);

        let data_dir = path("INVESTING_DATA_DIR", home.join("investing-app-data"));
        let idx_scraper_dir = path(
            "INVESTING_IDX_SCRAPER_DIR",
            home.join("tools/idx-financial-scraper"),
        );
        Ok(Self {
            token,
            bind,
            portfolio_spreadsheet_id: var("PORTFOLIO_SPREADSHEET_ID")
                .ok_or(ConfigError::Missing("PORTFOLIO_SPREADSHEET_ID"))?,
            google_credentials_path: path(
                "GOOGLE_APPLICATION_CREDENTIALS",
                home.join("credentials/hermes-finance-service-account.json"),
            ),
            web_dist: path("INVESTING_WEB_DIST", PathBuf::from("web/dist")),
            vault_root: path(
                "INVESTING_VAULT_ROOT",
                home.join("Documents/Hermes_Master_Vault"),
            ),
            thesis_drafts_path: path(
                "INVESTING_THESIS_DRAFTS_PATH",
                data_dir.join("thesis_drafts.json"),
            ),
            idx_db_path: path(
                "INVESTING_IDX_DB_PATH",
                home.join("tools/idx-financial-scraper/data/idx.sqlite"),
            ),
            macro_store_path: path(
                "INVESTING_MACRO_STORE_PATH",
                data_dir.join("macro_store.json"),
            ),
            macro_disable_fred: var("MACRO_DISABLE_FRED").is_some_and(|v| {
                matches!(
                    v.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "isolated" | "offline"
                )
            }),
            macro_watcher: var("INVESTING_MACRO_WATCHER").is_none_or(|v| truthy(&v)),
            research_db_path,
            research_scraper_watchlist: path(
                "INVESTING_RESEARCH_WATCHLIST",
                data_dir.join("research/watchlist.csv"),
            ),
            idx_emitens_csv: path(
                "INVESTING_IDX_EMITENS_CSV",
                home.join("tools/idx-financial-scraper/data/emitens.csv"),
            ),
            idx_xbrl_dir: path("INVESTING_IDX_XBRL_DIR", idx_scraper_dir.join("data/XBRL")),
            research_packs_dir: path(
                "INVESTING_RESEARCH_PACKS_DIR",
                PathBuf::from("research/packs"),
            ),
            research_documents_dir: data_dir.join("research/documents"),
            research_pdf_filings_dir: path(
                "INVESTING_PDF_FILINGS_DIR",
                data_dir.join("research/pdf-filings"),
            ),
            idx_scraper_dir,
            research_disable_scheduler: var("INVESTING_RESEARCH_DISABLE_SCHEDULER")
                .is_some_and(|v| truthy(&v)),
            data_dir,
        })
    }

    /// Config for tests: valid token, throwaway paths, nothing that touches the network.
    pub fn for_tests(dir: &Path) -> Self {
        Self {
            token: "t".repeat(MIN_TOKEN_LEN),
            bind: "127.0.0.1:0".parse().unwrap(),
            portfolio_spreadsheet_id: "test-portfolio".into(),
            google_credentials_path: dir.join("missing-credentials.json"),
            data_dir: dir.to_path_buf(),
            web_dist: dir.join("dist"),
            vault_root: dir.join("vault"),
            thesis_drafts_path: dir.join("thesis_drafts.json"),
            idx_db_path: dir.join("idx.sqlite"),
            macro_store_path: dir.join("macro_store.json"),
            macro_disable_fred: false,
            macro_watcher: false,
            research_db_path: dir.join("research.sqlite"),
            research_scraper_watchlist: dir.join("research/watchlist.csv"),
            idx_emitens_csv: dir.join("emitens.csv"),
            idx_scraper_dir: dir.join("idx-scraper"),
            idx_xbrl_dir: dir.join("idx-scraper/data/XBRL"),
            research_packs_dir: dir.join("packs"),
            research_documents_dir: dir.join("research/documents"),
            research_pdf_filings_dir: dir.join("research/pdf-filings"),
            research_disable_scheduler: true,
        }
    }
}

fn truthy(v: &str) -> bool {
    matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn expand_home(v: &str, home: &Path) -> PathBuf {
    match v.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if v == "~" => home.to_path_buf(),
        None => PathBuf::from(v),
    }
}

/// The directory of original-filing JSON files (`INVESTING_PDF_FILINGS_DIR`), for the CLI.
pub fn pdf_filings_dir(get: impl Fn(&str) -> Option<String>) -> PathBuf {
    let var = |k: &str| {
        get(k)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let home = var("HOME").map(PathBuf::from).unwrap_or_default();
    let path =
        |k: &str, default: PathBuf| var(k).map(|v| expand_home(&v, &home)).unwrap_or(default);
    let data_dir = path("INVESTING_DATA_DIR", home.join("investing-app-data"));
    path(
        "INVESTING_PDF_FILINGS_DIR",
        data_dir.join("research/pdf-filings"),
    )
}

/// Path to the app-owned research store (`research.sqlite`), honouring
/// `INVESTING_RESEARCH_DB_PATH` / `INVESTING_DATA_DIR` with the same defaults as `Config`.
/// Standalone so CLI subcommands can find the database without the full server env.
pub fn research_db_path(get: impl Fn(&str) -> Option<String>) -> PathBuf {
    let var = |k: &str| {
        get(k)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let home = var("HOME").map(PathBuf::from).unwrap_or_default();
    let path =
        |k: &str, default: PathBuf| var(k).map(|v| expand_home(&v, &home)).unwrap_or(default);
    let data_dir = path("INVESTING_DATA_DIR", home.join("investing-app-data"));
    path("INVESTING_RESEARCH_DB_PATH", data_dir.join("research.sqlite"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn rejects_missing_or_short_token() {
        assert_eq!(
            Config::from_lookup(env(&[])).unwrap_err(),
            ConfigError::Token
        );
        assert_eq!(
            Config::from_lookup(env(&[("INVESTING_APP_TOKEN", "short")])).unwrap_err(),
            ConfigError::Token
        );
    }

    #[test]
    fn portfolio_spreadsheet_id_is_required() {
        let err = Config::from_lookup(env(&[("INVESTING_APP_TOKEN", TOKEN)])).unwrap_err();
        assert_eq!(err, ConfigError::Missing("PORTFOLIO_SPREADSHEET_ID"));
    }

    #[test]
    fn defaults_and_home_expansion() {
        let cfg = Config::from_lookup(env(&[
            ("INVESTING_APP_TOKEN", TOKEN),
            ("PORTFOLIO_SPREADSHEET_ID", "b"),
            ("HOME", "/h"),
            ("INVESTING_IDX_DB_PATH", "~/x.sqlite"),
        ]))
        .unwrap();
        assert_eq!(cfg.bind.to_string(), "127.0.0.1:8789");
        assert_eq!(cfg.data_dir, PathBuf::from("/h/investing-app-data"));
        assert_eq!(cfg.idx_db_path, PathBuf::from("/h/x.sqlite"));
        assert_eq!(
            cfg.macro_store_path,
            PathBuf::from("/h/investing-app-data/macro_store.json")
        );
        assert!(!format!("{cfg:?}").contains(TOKEN));
    }

    #[test]
    fn research_paths_default_under_data_dir() {
        let cfg = Config::from_lookup(env(&[
            ("INVESTING_APP_TOKEN", TOKEN),
            ("PORTFOLIO_SPREADSHEET_ID", "b"),
            ("HOME", "/h"),
            ("INVESTING_IDX_SCRAPER_DIR", "~/scraper"),
            ("INVESTING_RESEARCH_DISABLE_SCHEDULER", "1"),
        ]))
        .unwrap();
        assert_eq!(
            cfg.research_db_path,
            PathBuf::from("/h/investing-app-data/research.sqlite")
        );
        assert_eq!(
            cfg.research_scraper_watchlist,
            PathBuf::from("/h/investing-app-data/research/watchlist.csv")
        );
        assert_eq!(
            cfg.idx_emitens_csv,
            PathBuf::from("/h/tools/idx-financial-scraper/data/emitens.csv")
        );
        assert_eq!(cfg.idx_scraper_dir, PathBuf::from("/h/scraper"));
        assert_eq!(cfg.idx_xbrl_dir, PathBuf::from("/h/scraper/data/XBRL"));
        assert_eq!(cfg.research_packs_dir, PathBuf::from("research/packs"));
        assert_eq!(
            cfg.research_documents_dir,
            PathBuf::from("/h/investing-app-data/research/documents")
        );
        assert_eq!(
            cfg.research_pdf_filings_dir,
            PathBuf::from("/h/investing-app-data/research/pdf-filings")
        );
        assert!(cfg.research_disable_scheduler);
    }

    #[test]
    fn the_config_and_the_cli_resolve_the_pdf_dir_alike() {
        for pairs in [
            vec![("HOME", "/h")],
            vec![("HOME", "/h"), ("INVESTING_DATA_DIR", "/d")],
            vec![("HOME", "/h"), ("INVESTING_PDF_FILINGS_DIR", "~/pdf")],
            vec![("HOME", "/h"), ("INVESTING_PDF_FILINGS_DIR", "  ")],
        ] {
            let lookup = |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            };
            let cfg = Config::from_lookup(env(&[
                ("INVESTING_APP_TOKEN", TOKEN),
                ("PORTFOLIO_SPREADSHEET_ID", "b"),
                ("HOME", pairs[0].1),
                (
                    "INVESTING_DATA_DIR",
                    pairs
                        .iter()
                        .find(|(n, _)| *n == "INVESTING_DATA_DIR")
                        .map_or("/h/investing-app-data", |p| p.1),
                ),
                (
                    "INVESTING_PDF_FILINGS_DIR",
                    pairs
                        .iter()
                        .find(|(n, _)| *n == "INVESTING_PDF_FILINGS_DIR")
                        .map_or("", |p| p.1),
                ),
            ]))
            .unwrap();
            assert_eq!(
                pdf_filings_dir(lookup),
                cfg.research_pdf_filings_dir,
                "{pairs:?}"
            );
        }
    }

    #[test]
    fn pdf_filings_dir_can_be_set_apart_from_the_data_dir() {
        let get = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            pdf_filings_dir(get(&[("HOME", "/h"), ("INVESTING_DATA_DIR", "/d")])),
            PathBuf::from("/d/research/pdf-filings")
        );
        assert_eq!(
            pdf_filings_dir(get(&[("HOME", "/h"), ("INVESTING_PDF_FILINGS_DIR", "~/pdf")])),
            PathBuf::from("/h/pdf")
        );
    }

    #[test]
    fn xbrl_dir_can_be_set_apart_from_the_scraper_dir() {
        let cfg = Config::from_lookup(env(&[
            ("INVESTING_APP_TOKEN", TOKEN),
            ("PORTFOLIO_SPREADSHEET_ID", "b"),
            ("HOME", "/h"),
            ("INVESTING_IDX_XBRL_DIR", "~/filings"),
        ]))
        .unwrap();
        assert_eq!(cfg.idx_xbrl_dir, PathBuf::from("/h/filings"));
        assert_eq!(
            cfg.idx_scraper_dir,
            PathBuf::from("/h/tools/idx-financial-scraper")
        );
    }
}
