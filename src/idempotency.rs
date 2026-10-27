//! Durable `Idempotency-Key` store (prd.md §6.1, plan.md R2): key → response
//! for 24 hours in `FINANCE_DATA_DIR/idempotency.jsonl`, surviving restarts.
//!
//! Each write is journaled twice: `pending` (with the target sheet row)
//! before touching Sheets, then `done` (with the response) or `failed`.
//! A key left `pending` by a crash is resolved by the caller by inspecting
//! the recorded row, so a retry never appends a second row.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Entry {
    Pending {
        fingerprint: String,
        row: usize,
    },
    Done {
        fingerprint: String,
        status: u16,
        body: Value,
    },
    Failed,
}

#[derive(Serialize, Deserialize)]
struct Line {
    key: String,
    at: u64,
    #[serde(flatten)]
    entry: Entry,
}

pub struct IdempotencyStore {
    path: PathBuf,
    entries: HashMap<String, (u64, Entry)>,
}

impl IdempotencyStore {
    /// Loads the journal, drops expired/failed keys and compacts the file.
    pub fn open(path: &Path, now: u64) -> std::io::Result<Self> {
        let mut entries = HashMap::new();
        match std::fs::read_to_string(path) {
            Ok(raw) => {
                for line in raw.lines().filter(|l| !l.trim().is_empty()) {
                    match serde_json::from_str::<Line>(line) {
                        Ok(l) => {
                            entries.insert(l.key, (l.at, l.entry));
                        }
                        Err(_) => tracing::warn!("skipping malformed idempotency journal line"),
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        entries
            .retain(|_, (at, e)| !matches!(e, Entry::Failed) && now.saturating_sub(*at) < TTL_SECS);
        let store = Self {
            path: path.to_path_buf(),
            entries,
        };
        store.compact()?;
        Ok(store)
    }

    fn compact(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            let mut lines: Vec<_> = self.entries.iter().collect();
            lines.sort_by_key(|(_, (at, _))| *at);
            for (key, (at, entry)) in lines {
                let line = Line {
                    key: key.clone(),
                    at: *at,
                    entry: entry.clone(),
                };
                writeln!(f, "{}", serde_json::to_string(&line).expect("serializable"))?;
            }
            f.sync_all()?;
        }
        std::fs::rename(tmp, &self.path)
    }

    pub fn get(&self, key: &str, now: u64) -> Option<&Entry> {
        self.entries
            .get(key)
            .filter(|(at, _)| now.saturating_sub(*at) < TTL_SECS)
            .map(|(_, e)| e)
    }

    /// Appends to the journal (fsync'd) and updates memory.
    pub fn record(&mut self, key: &str, entry: Entry, now: u64) -> std::io::Result<()> {
        let line = Line {
            key: key.to_string(),
            at: now,
            entry: entry.clone(),
        };
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(&line).expect("serializable"))?;
        f.sync_data()?;
        if matches!(entry, Entry::Failed) {
            self.entries.remove(key);
        } else {
            self.entries.insert(key.to_string(), (now, entry));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn survives_reopen_and_expires() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("idempotency.jsonl");
        let mut s = IdempotencyStore::open(&path, 1_000).unwrap();
        s.record(
            "a",
            Entry::Pending {
                fingerprint: "f".into(),
                row: 20,
            },
            1_000,
        )
        .unwrap();
        let done = Entry::Done {
            fingerprint: "f".into(),
            status: 201,
            body: json!({"ok": true}),
        };
        s.record("a", done.clone(), 1_001).unwrap();
        s.record(
            "b",
            Entry::Pending {
                fingerprint: "g".into(),
                row: 21,
            },
            1_002,
        )
        .unwrap();
        s.record("b", Entry::Failed, 1_003).unwrap();

        let s = IdempotencyStore::open(&path, 2_000).unwrap();
        assert_eq!(s.get("a", 2_000), Some(&done));
        assert_eq!(s.get("b", 2_000), None, "failed keys are retryable");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().lines().count(),
            1,
            "compacted"
        );

        let s = IdempotencyStore::open(&path, 1_001 + TTL_SECS).unwrap();
        assert_eq!(s.get("a", 1_001 + TTL_SECS), None, "expired after 24h");
    }

    #[test]
    fn tolerates_malformed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("i.jsonl");
        std::fs::write(&path, "not json\n{\"key\":\"k\",\"at\":5,\"state\":\"pending\",\"fingerprint\":\"f\",\"row\":3}\n")
            .unwrap();
        let s = IdempotencyStore::open(&path, 10).unwrap();
        assert_eq!(
            s.get("k", 10),
            Some(&Entry::Pending {
                fingerprint: "f".into(),
                row: 3
            })
        );
    }
}
