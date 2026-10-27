//! Repository for `structure_maps` (Riset redesign G1): one JSON document per ticker holding
//! the nodes of the operational structure map. The store keeps whatever the API validated;
//! every write replaces the document and goes through the JSONL log like the other owner
//! tables, and `apply` is shared with `rebuild`.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{LogEntry, Result, Store};

/// Where a drafted node comes from: a page of an uploaded document and a verbatim snippet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeSource {
    pub document: String,
    pub page: u32,
    pub snippet: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructureNode {
    pub id: String,
    /// `None` for a child of the (implicit) root.
    pub parent: Option<String>,
    pub label: String,
    /// `segmen`, `produk`, `pelanggan`, `pendorong_harga`, `biaya`, `kepemilikan`, `lainnya`.
    pub kind: String,
    /// A pack or core metric id whose latest value the node shows.
    pub metric: Option<String>,
    pub note: Option<String>,
    /// `proposed`, `accepted` or `rejected`.
    pub state: String,
    /// Added by the owner (accepted from the start, no source needed).
    #[serde(default)]
    pub owner: bool,
    pub source: Option<NodeSource>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructureMap {
    pub nodes: Vec<StructureNode>,
    pub updated_at: String,
}

impl Store {
    pub fn structure(&self, ticker: &str) -> Result<Option<StructureMap>> {
        let row: Option<(String, String)> = self
            .conn()
            .query_row(
                "SELECT doc, updated_at FROM structure_maps WHERE ticker = ?1",
                [ticker],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(doc, updated_at)| {
            Ok(StructureMap {
                nodes: serde_json::from_str(&doc)?,
                updated_at,
            })
        })
        .transpose()
    }

    /// Replaces the ticker's whole document. The caller has validated `nodes`.
    pub fn set_structure(&self, ticker: &str, nodes: &[StructureNode], at: &str) -> Result<()> {
        self.write(&LogEntry::StructureSet {
            ticker: ticker.into(),
            nodes: nodes.to_vec(),
            at: at.into(),
        })?;
        Ok(())
    }
}

pub(super) fn apply(conn: &Connection, entry: &LogEntry) -> rusqlite::Result<usize> {
    match entry {
        LogEntry::StructureSet { ticker, nodes, at } => {
            let doc = serde_json::to_string(nodes)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            conn.execute(
                "INSERT INTO structure_maps (ticker, doc, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(ticker) DO UPDATE SET doc = excluded.doc, updated_at = excluded.updated_at",
                params![ticker, doc, at],
            )
        }
        _ => Ok(0),
    }
}
