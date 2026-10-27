//! Research universe (research plan §3.1, §4.1, D14; prd.md A6.3): held tickers from the
//! portfolio sheet, manual additions, and up to three peers per ticker, each with an
//! optional pack override and business note. Held tickers cannot be removed; a held ticker
//! that leaves the portfolio becomes `manual` so the owner's research on it survives
//! (ASUMSI R2.2). The scraper watchlist is written from every row, peers included (A2.6).
//!
//! Pack selection (A2.4, A2.5): the per-ticker override wins; else the pack whose
//! `industry_codes` claim the latest filing's subindustry code; else "no pack", with the
//! filing-derived default always reported next to any override.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use super::packs::PackSet;
use super::store::{Store, StoreError};

/// Maximum peers per ticker (D14).
pub const MAX_PEERS: usize = 3;
/// `business_note` length limit (design.md §4.2 Ringkasan).
pub const MAX_NOTE_CHARS: usize = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Held,
    Manual,
    Peer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Manual => "manual",
            Self::Peer => "peer",
        }
    }

    /// `watch` is the v4 spelling of `manual`, accepted so a log line or a store written before migration 005 still reads.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "held" => Some(Self::Held),
            "manual" | "watch" => Some(Self::Manual),
            "peer" => Some(Self::Peer),
            _ => None,
        }
    }
}

/// One `universe` row.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UniverseEntry {
    pub ticker: String,
    pub role: Role,
    /// The ticker this one is a peer of (any role may carry it).
    pub peer_of: Option<String>,
    pub pack_override: Option<String>,
    pub business_note: Option<String>,
    pub added_at: String,
    pub synced_at: String,
}

/// A portfolio position with shares > 0; `avg_price` is info only.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldPosition {
    pub ticker: String,
    pub avg_price: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum UniverseError {
    /// Not a four-letter IDX code (HTTP 422).
    #[error("ticker must be four letters A-Z")]
    Invalid,
    /// Held positions cannot be removed (HTTP 409).
    #[error("{0} is held in the portfolio and cannot be removed")]
    Held(String),
    /// Not in the universe (HTTP 404).
    #[error("{0} is not in the universe")]
    NotFound(String),
    /// More than `MAX_PEERS` peers for one ticker (HTTP 422).
    #[error("at most {MAX_PEERS} peers per ticker")]
    TooManyPeers,
    /// A peer must be a different ticker, and not the ticker's own parent (HTTP 422).
    #[error("a ticker cannot be its own peer or its parent's peer")]
    SelfPeer,
    /// `business_note` over the limit (HTTP 422).
    #[error("business note must be at most {MAX_NOTE_CHARS} characters")]
    NoteTooLong,
    /// Unknown pack id for the override (HTTP 422).
    #[error("unknown pack `{0}`")]
    UnknownPack(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

pub fn is_valid_ticker(t: &str) -> bool {
    t.len() == 4 && t.bytes().all(|b| b.is_ascii_uppercase())
}

/// Trims and upper-cases user input; `None` when it is not a four-letter code.
pub fn normalize_ticker(raw: &str) -> Option<String> {
    let t = raw.trim().to_ascii_uppercase();
    is_valid_ticker(&t).then_some(t)
}

/// Positions with shares > 0 from the `parse_portfolio` JSON, skipping codes that are not a
/// four-letter ticker (cash lines, funds).
pub fn held_from_portfolio(portfolio: &Value) -> Vec<HeldPosition> {
    let mut out: Vec<HeldPosition> = Vec::new();
    let positions = portfolio.get("positions").and_then(Value::as_array);
    for p in positions.into_iter().flatten() {
        let shares = p.get("shares").and_then(Value::as_f64).unwrap_or(0.0);
        let Some(ticker) = p
            .get("ticker")
            .and_then(Value::as_str)
            .and_then(normalize_ticker)
        else {
            continue;
        };
        if shares > 0.0 && !out.iter().any(|h| h.ticker == ticker) {
            out.push(HeldPosition {
                ticker,
                avg_price: p.get("avg_price").and_then(Value::as_f64).unwrap_or(0.0),
            });
        }
    }
    out.sort_by(|a, b| a.ticker.cmp(&b.ticker));
    out
}

/// Portfolio sync: held rows upserted, sold ones demoted to `manual`, average costs stored.
/// Owner-authored columns (`peer_of`, `pack_override`, `business_note`, `added_at`) and
/// manual or peer rows are never touched by the sync.
pub fn sync(
    store: &Store,
    held: &[HeldPosition],
    now: &str,
) -> Result<Vec<UniverseEntry>, StoreError> {
    let tickers: Vec<String> = held
        .iter()
        .map(|h| h.ticker.clone())
        .filter(|t| is_valid_ticker(t))
        .collect();
    store.sync_held(&tickers, now)?;
    let costs: Vec<(String, f64)> = held
        .iter()
        .map(|h| (h.ticker.clone(), h.avg_price))
        .collect();
    store.replace_held(&costs)?;
    store.universe()
}

/// What `POST /universe` carries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AddRequest {
    pub ticker: String,
    /// `Manual` (default) or `Peer` (then `peer_of` is required).
    pub role: Option<Role>,
    pub peer_of: Option<String>,
    pub pack_override: Option<String>,
    pub business_note: Option<String>,
}

fn clean_note(note: Option<&str>) -> Result<Option<String>, UniverseError> {
    let Some(n) = note.map(str::trim).filter(|n| !n.is_empty()) else {
        return Ok(None);
    };
    if n.chars().count() > MAX_NOTE_CHARS {
        return Err(UniverseError::NoteTooLong);
    }
    Ok(Some(n.to_string()))
}

fn check_pack(packs: &PackSet, id: Option<&str>) -> Result<Option<String>, UniverseError> {
    match id.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(id) if packs.get(id).is_some() && id != super::packs::CORE_ID => {
            Ok(Some(id.to_string()))
        }
        Some(id) => Err(UniverseError::UnknownPack(id.to_string())),
    }
}

fn peers_of(entries: &[UniverseEntry], ticker: &str) -> Vec<String> {
    entries
        .iter()
        .filter(|e| e.peer_of.as_deref() == Some(ticker))
        .map(|e| e.ticker.clone())
        .collect()
}

/// Adds a manual ticker or a peer (idempotent: an existing row keeps its role, held stays
/// held, and only the fields given are updated).
pub fn add(
    store: &Store,
    packs: &PackSet,
    req: &AddRequest,
    now: &str,
) -> Result<UniverseEntry, UniverseError> {
    let ticker = normalize_ticker(&req.ticker).ok_or(UniverseError::Invalid)?;
    let role = req.role.unwrap_or(Role::Manual);
    let peer_of = match (role, &req.peer_of) {
        (Role::Peer, None) => return Err(UniverseError::Invalid),
        (_, Some(p)) => Some(normalize_ticker(p).ok_or(UniverseError::Invalid)?),
        (_, None) => None,
    };
    if peer_of.as_deref() == Some(ticker.as_str()) {
        return Err(UniverseError::SelfPeer);
    }
    let entries = store.universe()?;
    if let Some(p) = &peer_of {
        let parent = entries
            .iter()
            .find(|e| &e.ticker == p)
            .ok_or_else(|| UniverseError::NotFound(p.clone()))?;
        // `ticker` becomes a child of `p`: refuse when `p` already descends from `ticker`.
        if reaches(&entries, &parent.ticker, &ticker) {
            return Err(UniverseError::SelfPeer);
        }
        let current = peers_of(&entries, p);
        if !current.contains(&ticker) && current.len() >= MAX_PEERS {
            return Err(UniverseError::TooManyPeers);
        }
    }
    let existing = entries.iter().find(|e| e.ticker == ticker);
    let entry = UniverseEntry {
        ticker: ticker.clone(),
        role: match existing.map(|e| e.role) {
            Some(Role::Held) => Role::Held,
            Some(Role::Manual) => Role::Manual,
            Some(Role::Peer) | None => role,
        },
        peer_of: peer_of.or_else(|| existing.and_then(|e| e.peer_of.clone())),
        pack_override: match req.pack_override.as_deref() {
            Some(id) => check_pack(packs, Some(id))?,
            None => existing.and_then(|e| e.pack_override.clone()),
        },
        business_note: match req.business_note.as_deref() {
            Some(n) => clean_note(Some(n))?,
            None => existing.and_then(|e| e.business_note.clone()),
        },
        added_at: existing.map_or_else(|| now.to_string(), |e| e.added_at.clone()),
        synced_at: existing.map_or_else(|| now.to_string(), |e| e.synced_at.clone()),
    };
    if existing != Some(&entry) {
        store.upsert_universe(&entry)?;
    }
    Ok(entry)
}

/// Drops the peer link of every row whose `peer_of` is `parent` and is not in `keep`: a
/// peer-only row is deleted (and its own peer-only rows with it), any other row just loses
/// `peer_of`.
fn detach_peers(
    store: &Store,
    entries: &[UniverseEntry],
    parent: &str,
    keep: &[String],
    now: &str,
) -> Result<(), UniverseError> {
    // Iterative with a visited set: a stored peer cycle must not loop or overflow the stack.
    let mut visited: Vec<&str> = vec![parent];
    let mut queue: Vec<(&str, &[String])> = vec![(parent, keep)];
    while let Some((owner, keep)) = queue.pop() {
        for e in entries
            .iter()
            .filter(|e| e.peer_of.as_deref() == Some(owner))
        {
            if keep.contains(&e.ticker) || visited.contains(&e.ticker.as_str()) {
                continue;
            }
            visited.push(&e.ticker);
            if e.role == Role::Peer {
                queue.push((&e.ticker, &[]));
                store.delete_universe(&e.ticker, now)?;
            } else {
                store.upsert_universe(&UniverseEntry {
                    peer_of: None,
                    ..e.clone()
                })?;
            }
        }
    }
    Ok(())
}

/// True when `candidate` is `ticker` or one of its ancestors through `peer_of` (so making
/// `ticker` a peer of `candidate`, or `candidate` a peer of `ticker`, would close a loop).
fn reaches(entries: &[UniverseEntry], from: &str, target: &str) -> bool {
    let mut seen: Vec<&str> = Vec::new();
    let mut cur = from;
    loop {
        if cur == target {
            return true;
        }
        if seen.contains(&cur) {
            return false;
        }
        seen.push(cur);
        match entries
            .iter()
            .find(|e| e.ticker == cur)
            .and_then(|e| e.peer_of.as_deref())
        {
            Some(next) => cur = next,
            None => return false,
        }
    }
}

/// Removes a manual ticker or a peer. Held positions are refused. Peers that exist only for
/// the removed ticker go with it; other rows just lose their `peer_of`.
pub fn remove(store: &Store, raw: &str, now: &str) -> Result<(), UniverseError> {
    let ticker = normalize_ticker(raw).ok_or(UniverseError::Invalid)?;
    let entries = store.universe()?;
    match entries.iter().find(|e| e.ticker == ticker) {
        None => return Err(UniverseError::NotFound(ticker)),
        Some(e) if e.role == Role::Held => return Err(UniverseError::Held(ticker)),
        Some(_) => {}
    }
    detach_peers(store, &entries, &ticker, &[], now)?;
    store.delete_universe(&ticker, now)?;
    Ok(())
}

/// What `PUT /universe/{ticker}` carries; `Some(None)` clears a field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Patch {
    pub pack_override: Option<Option<String>>,
    pub business_note: Option<Option<String>>,
    /// Replaces the peer list (at most `MAX_PEERS`).
    pub peers: Option<Vec<String>>,
}

/// Updates the override, note and peers of a universe ticker.
pub fn update(
    store: &Store,
    packs: &PackSet,
    raw: &str,
    patch: &Patch,
    now: &str,
) -> Result<UniverseEntry, UniverseError> {
    let ticker = normalize_ticker(raw).ok_or(UniverseError::Invalid)?;
    let entries = store.universe()?;
    let mut entry = entries
        .iter()
        .find(|e| e.ticker == ticker)
        .cloned()
        .ok_or_else(|| UniverseError::NotFound(ticker.clone()))?;
    if let Some(p) = &patch.pack_override {
        entry.pack_override = check_pack(packs, p.as_deref())?;
    }
    if let Some(n) = &patch.business_note {
        entry.business_note = clean_note(n.as_deref())?;
    }
    if let Some(list) = &patch.peers {
        let mut wanted: Vec<String> = Vec::new();
        for raw in list {
            let p = normalize_ticker(raw).ok_or(UniverseError::Invalid)?;
            if p == ticker {
                return Err(UniverseError::SelfPeer);
            }
            if !wanted.contains(&p) {
                wanted.push(p);
            }
        }
        if wanted.len() > MAX_PEERS {
            return Err(UniverseError::TooManyPeers);
        }
        // Each wanted peer becomes a child of `ticker`: refuse when `ticker` descends from it.
        if wanted.iter().any(|p| reaches(&entries, &ticker, p)) {
            return Err(UniverseError::SelfPeer);
        }
        detach_peers(store, &entries, &ticker, &wanted, now)?;
        for p in &wanted {
            let existing = entries.iter().find(|e| &e.ticker == p);
            let row = match existing {
                Some(e) => UniverseEntry {
                    peer_of: Some(ticker.clone()),
                    ..e.clone()
                },
                None => UniverseEntry {
                    ticker: p.clone(),
                    role: Role::Peer,
                    peer_of: Some(ticker.clone()),
                    pack_override: None,
                    business_note: None,
                    added_at: now.to_string(),
                    synced_at: now.to_string(),
                },
            };
            if existing != Some(&row) {
                store.upsert_universe(&row)?;
            }
        }
    }
    if entries.iter().find(|e| e.ticker == ticker) != Some(&entry) {
        store.upsert_universe(&entry)?;
    }
    Ok(entry)
}

/// Peers of every ticker, from the rows' `peer_of`.
pub fn peer_map(entries: &[UniverseEntry]) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in entries {
        if let Some(p) = &e.peer_of {
            m.entry(p.clone()).or_default().push(e.ticker.clone());
        }
    }
    m
}

/// How a ticker's pack was chosen (design.md §4.2: the filing-derived default is shown next
/// to any override; A2.5: unknown code and no override is a visible "no pack" state).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PackChoice {
    /// The pack in effect, or `None` for "no pack".
    pub id: Option<String>,
    /// `override`, `filing` or `none`.
    pub source: &'static str,
    /// Subindustry code of the latest filing, when any filing exists.
    pub filing_code: Option<String>,
    /// The pack the filing code maps to, whether or not an override replaces it.
    pub filing_default: Option<String>,
    #[serde(rename = "override")]
    pub override_: Option<String>,
}

pub fn pack_choice(
    packs: &PackSet,
    entry: &UniverseEntry,
    filing_code: Option<&str>,
) -> PackChoice {
    let filing_default = filing_code
        .and_then(|c| packs.for_industry_code(c))
        .map(|p| p.id.clone());
    let override_ = entry
        .pack_override
        .as_deref()
        .filter(|id| packs.get(id).is_some())
        .map(str::to_string);
    let (id, source) = match (&override_, &filing_default) {
        (Some(o), _) => (Some(o.clone()), "override"),
        (None, Some(f)) => (Some(f.clone()), "filing"),
        (None, None) => (None, "none"),
    };
    PackChoice {
        id,
        source,
        filing_code: filing_code.map(str::to_string),
        filing_default,
        override_,
    }
}

/// Writes the scraper watchlist (`ticker` header, one code per line, sorted) atomically,
/// every role included. Returns whether the file changed, so an unchanged universe leaves
/// the mtime alone.
pub fn write_watchlist(path: &Path, entries: &[UniverseEntry]) -> std::io::Result<bool> {
    let mut tickers: Vec<&str> = entries.iter().map(|e| e.ticker.as_str()).collect();
    tickers.sort_unstable();
    tickers.dedup();
    let mut body = String::from("ticker\n");
    for t in tickers {
        body.push_str(t);
        body.push('\n');
    }
    if std::fs::read_to_string(path).is_ok_and(|old| old == body) {
        return Ok(false);
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    // A unique temp name: the scheduler's sync and an API write may run at the same time.
    let tmp = path.with_extension(format!("csv.{}.tmp", uuid::Uuid::new_v4()));
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(body.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(true)
}
