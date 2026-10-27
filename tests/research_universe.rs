//! Universe (research plan §3.1, D14; prd.md A2.4, A2.5, A2.6, A6.3): held from the
//! portfolio, manual adds, peers up to three per ticker, pack override next to the
//! filing-derived default, held cannot be removed, watchlist.csv with peers, log replay.

use finance_app::research::packs::Packs;
use finance_app::research::store::Store;
use finance_app::research::universe::{
    add, held_from_portfolio, is_valid_ticker, normalize_ticker, pack_choice, peer_map, remove,
    sync, update, write_watchlist, AddRequest, HeldPosition, Patch, Role, UniverseEntry,
    UniverseError,
};
use serde_json::json;

const AT: &str = "2026-10-22T09:00:00+07:00";

fn held(tickers: &[&str]) -> Vec<HeldPosition> {
    tickers
        .iter()
        .map(|t| HeldPosition {
            ticker: (*t).into(),
            avg_price: 1_000.0,
        })
        .collect()
}

fn roles(store: &Store) -> Vec<(String, Role, Option<String>)> {
    store
        .universe()
        .unwrap()
        .into_iter()
        .map(|e| (e.ticker, e.role, e.peer_of))
        .collect()
}

fn manual(ticker: &str) -> AddRequest {
    AddRequest {
        ticker: ticker.into(),
        ..Default::default()
    }
}

fn peer(ticker: &str, of: &str) -> AddRequest {
    AddRequest {
        ticker: ticker.into(),
        role: Some(Role::Peer),
        peer_of: Some(of.into()),
        ..Default::default()
    }
}

fn packs() -> finance_app::research::packs::PackSet {
    (*Packs::embedded().set()).clone()
}

#[test]
fn held_comes_from_positions_with_shares() {
    let portfolio = json!({"positions": [
        {"ticker": "CCCC", "shares": 300.0, "avg_price": 50.0},
        {"ticker": "AAAA", "shares": 100.0, "avg_price": 1200.5},
        {"ticker": "BBBB", "shares": 0.0, "avg_price": 10.0},
        {"ticker": "KAS", "shares": 5.0, "avg_price": 1.0},
        {"ticker": "aaaa", "shares": 9.0, "avg_price": 1.0},
        {"ticker": "DDDD", "avg_price": 1.0},
    ]});
    let positions = held_from_portfolio(&portfolio);
    assert_eq!(
        positions,
        vec![
            HeldPosition {
                ticker: "AAAA".into(),
                avg_price: 1200.5
            },
            HeldPosition {
                ticker: "CCCC".into(),
                avg_price: 50.0
            },
        ]
    );
    assert!(held_from_portfolio(&json!({})).is_empty());
}

#[test]
fn ticker_validation_and_role_spellings() {
    assert!(is_valid_ticker("AAAA"));
    for bad in ["AAA", "AAAAA", "aaaa", "AA1A", "", "AA A"] {
        assert!(!is_valid_ticker(bad), "{bad}");
    }
    assert_eq!(normalize_ticker(" bbbb "), Some("BBBB".into()));
    assert_eq!(normalize_ticker("b1bb"), None);
    assert_eq!(Role::parse("watch"), Some(Role::Manual), "v4 spelling");
    assert_eq!(Role::parse("peer"), Some(Role::Peer));
    assert_eq!(Role::Manual.as_str(), "manual");
}

#[test]
fn sync_adds_held_demotes_sold_and_keeps_owner_columns() {
    let store = Store::open_memory().unwrap();
    let packs = packs();
    sync(&store, &held(&["AAAA", "CCCC"]), AT).unwrap();
    assert_eq!(
        roles(&store),
        vec![
            ("AAAA".into(), Role::Held, None),
            ("CCCC".into(), Role::Held, None)
        ]
    );
    update(
        &store,
        &packs,
        "AAAA",
        &Patch {
            pack_override: Some(Some("shipping".into())),
            business_note: Some(Some("Kapal sintetis".into())),
            peers: Some(vec!["BBBB".into()]),
        },
        AT,
    )
    .unwrap();
    // A later sync: AAAA sold, CCCC still held. AAAA becomes manual, keeps its columns.
    sync(&store, &held(&["CCCC"]), "2026-10-23T09:00:00+07:00").unwrap();
    let aaaa = store.universe_entry("AAAA").unwrap().unwrap();
    assert_eq!(aaaa.role, Role::Manual);
    assert_eq!(aaaa.pack_override.as_deref(), Some("shipping"));
    assert_eq!(aaaa.business_note.as_deref(), Some("Kapal sintetis"));
    assert_eq!(aaaa.added_at, AT);
    assert_eq!(
        roles(&store),
        vec![
            ("AAAA".into(), Role::Manual, None),
            ("BBBB".into(), Role::Peer, Some("AAAA".into())),
            ("CCCC".into(), Role::Held, None)
        ]
    );
    // Held again: back to held, columns intact.
    sync(&store, &held(&["AAAA", "CCCC"]), AT).unwrap();
    let aaaa = store.universe_entry("AAAA").unwrap().unwrap();
    assert_eq!(aaaa.role, Role::Held);
    assert_eq!(aaaa.pack_override.as_deref(), Some("shipping"));
    assert_eq!(store.avg_price("AAAA").unwrap(), Some(1_000.0));
}

#[test]
fn add_remove_and_held_is_protected() {
    let store = Store::open_memory().unwrap();
    let packs = packs();
    sync(&store, &held(&["AAAA"]), AT).unwrap();

    let e = add(&store, &packs, &manual("bbbb"), AT).unwrap();
    assert_eq!((e.ticker.as_str(), e.role), ("BBBB", Role::Manual));
    add(&store, &packs, &manual("BBBB"), AT).unwrap(); // idempotent
    assert_eq!(roles(&store).len(), 2);

    // Held → 409, invalid → 422, unknown → 404.
    assert!(matches!(remove(&store, "AAAA", AT), Err(UniverseError::Held(t)) if t == "AAAA"));
    assert!(matches!(
        add(&store, &packs, &manual("AB1"), AT),
        Err(UniverseError::Invalid)
    ));
    assert!(matches!(
        remove(&store, "ZZZZ", AT),
        Err(UniverseError::NotFound(_))
    ));

    // Manually adding a held ticker keeps it held and still unremovable; the note lands.
    let e = add(
        &store,
        &packs,
        &AddRequest {
            ticker: "AAAA".into(),
            business_note: Some("  Emiten sintetis  ".into()),
            ..Default::default()
        },
        AT,
    )
    .unwrap();
    assert_eq!(e.role, Role::Held);
    assert_eq!(e.business_note.as_deref(), Some("Emiten sintetis"));
    assert!(matches!(
        remove(&store, "AAAA", AT),
        Err(UniverseError::Held(_))
    ));

    remove(&store, "BBBB", AT).unwrap();
    assert_eq!(roles(&store).len(), 1);
    assert!(matches!(
        remove(&store, "BBBB", AT),
        Err(UniverseError::NotFound(_))
    ));

    // A sync keeps the removal.
    sync(&store, &held(&["AAAA"]), AT).unwrap();
    assert_eq!(roles(&store).len(), 1);
}

#[test]
fn peers_are_capped_at_three_and_go_with_their_ticker() {
    let store = Store::open_memory().unwrap();
    let packs = packs();
    sync(&store, &held(&["AAAA"]), AT).unwrap();
    for p in ["BBBB", "CCCC", "DDDD"] {
        add(&store, &packs, &peer(p, "AAAA"), AT).unwrap();
    }
    assert!(matches!(
        add(&store, &packs, &peer("EEEE", "AAAA"), AT),
        Err(UniverseError::TooManyPeers)
    ));
    assert!(matches!(
        add(&store, &packs, &peer("AAAA", "AAAA"), AT),
        Err(UniverseError::SelfPeer)
    ));
    assert!(matches!(
        add(&store, &packs, &peer("EEEE", "ZZZZ"), AT),
        Err(UniverseError::NotFound(t)) if t == "ZZZZ"
    ));
    assert!(
        matches!(
            add(
                &store,
                &packs,
                &AddRequest {
                    ticker: "EEEE".into(),
                    role: Some(Role::Peer),
                    ..Default::default()
                },
                AT
            ),
            Err(UniverseError::Invalid)
        ),
        "a peer needs peer_of"
    );
    let entries = store.universe().unwrap();
    assert_eq!(
        peer_map(&entries)["AAAA"],
        vec!["BBBB".to_string(), "CCCC".into(), "DDDD".into()]
    );

    // Replacing the list: one stays, one new, the dropped peer row disappears.
    update(
        &store,
        &packs,
        "AAAA",
        &Patch {
            peers: Some(vec!["cccc".into(), "FFFF".into()]),
            ..Default::default()
        },
        AT,
    )
    .unwrap();
    assert_eq!(
        roles(&store),
        vec![
            ("AAAA".into(), Role::Held, None),
            ("CCCC".into(), Role::Peer, Some("AAAA".into())),
            ("FFFF".into(), Role::Peer, Some("AAAA".into())),
        ]
    );
    // A manual ticker used as a peer keeps its role and only loses `peer_of` when dropped.
    add(&store, &packs, &manual("GGGG"), AT).unwrap();
    update(
        &store,
        &packs,
        "AAAA",
        &Patch {
            peers: Some(vec!["GGGG".into()]),
            ..Default::default()
        },
        AT,
    )
    .unwrap();
    assert_eq!(
        roles(&store),
        vec![
            ("AAAA".into(), Role::Held, None),
            ("GGGG".into(), Role::Manual, Some("AAAA".into())),
        ]
    );
    update(
        &store,
        &packs,
        "AAAA",
        &Patch {
            peers: Some(vec![]),
            ..Default::default()
        },
        AT,
    )
    .unwrap();
    assert_eq!(roles(&store)[1], ("GGGG".into(), Role::Manual, None));

    // Removing a manual ticker takes its peer-only rows with it.
    add(&store, &packs, &manual("HHHH"), AT).unwrap();
    add(&store, &packs, &peer("IIII", "HHHH"), AT).unwrap();
    remove(&store, "HHHH", AT).unwrap();
    assert!(store.universe_entry("IIII").unwrap().is_none());
    assert!(matches!(
        update(
            &store,
            &packs,
            "AAAA",
            &Patch {
                peers: Some(vec!["B".into()]),
                ..Default::default()
            },
            AT
        ),
        Err(UniverseError::Invalid)
    ));
}

#[test]
fn override_and_note_are_validated_and_pack_choice_shows_the_filing_default() {
    let store = Store::open_memory().unwrap();
    let packs = packs();
    sync(&store, &held(&["AAAA"]), AT).unwrap();
    assert!(matches!(
        update(
            &store,
            &packs,
            "AAAA",
            &Patch {
                pack_override: Some(Some("nope".into())),
                ..Default::default()
            },
            AT
        ),
        Err(UniverseError::UnknownPack(p)) if p == "nope"
    ));
    assert!(
        matches!(
            update(
                &store,
                &packs,
                "AAAA",
                &Patch {
                    pack_override: Some(Some("core".into())),
                    ..Default::default()
                },
                AT
            ),
            Err(UniverseError::UnknownPack(_))
        ),
        "core is always applied, never an override"
    );
    assert!(matches!(
        update(
            &store,
            &packs,
            "AAAA",
            &Patch {
                business_note: Some(Some("x".repeat(601))),
                ..Default::default()
            },
            AT
        ),
        Err(UniverseError::NoteTooLong)
    ));
    let e = update(
        &store,
        &packs,
        "AAAA",
        &Patch {
            pack_override: Some(Some("shipping".into())),
            ..Default::default()
        },
        AT,
    )
    .unwrap();
    // Filing says coal (A121); override says shipping; both are reported (A2.4).
    let c = pack_choice(&packs, &e, Some("A121"));
    assert_eq!(c.id.as_deref(), Some("shipping"));
    assert_eq!(c.source, "override");
    assert_eq!(c.filing_default.as_deref(), Some("coal_mining"));
    assert_eq!(c.override_.as_deref(), Some("shipping"));
    // Clearing the override: the filing default rules; A112 maps to shipping by code.
    let e = update(
        &store,
        &packs,
        "AAAA",
        &Patch {
            pack_override: Some(None),
            ..Default::default()
        },
        AT,
    )
    .unwrap();
    let c = pack_choice(&packs, &e, Some("A112"));
    assert_eq!((c.id.as_deref(), c.source), (Some("shipping"), "filing"));
    // Unknown code and no override: "no pack" (A2.5).
    let c = pack_choice(&packs, &e, Some("Z999"));
    assert_eq!((c.id, c.source), (None, "none"));
    assert_eq!(pack_choice(&packs, &e, None).source, "none");
}

#[test]
fn universe_rows_survive_losing_the_database_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research.sqlite");
    let packs = packs();
    let store = Store::open(&path).unwrap();
    sync(&store, &held(&["AAAA"]), AT).unwrap();
    add(
        &store,
        &packs,
        &AddRequest {
            ticker: "BBBB".into(),
            pack_override: Some("it_distribution".into()),
            business_note: Some("Distributor sintetis".into()),
            ..Default::default()
        },
        AT,
    )
    .unwrap();
    add(&store, &packs, &peer("CCCC", "BBBB"), AT).unwrap();
    add(&store, &packs, &manual("DDDD"), AT).unwrap();
    remove(&store, "DDDD", AT).unwrap();
    drop(store);
    std::fs::remove_file(&path).unwrap();

    let store = Store::open(&path).unwrap();
    let entries = sync(&store, &held(&["AAAA"]), AT).unwrap();
    let view: Vec<(&str, Role, Option<&str>)> = entries
        .iter()
        .map(|e| (e.ticker.as_str(), e.role, e.peer_of.as_deref()))
        .collect();
    assert_eq!(
        view,
        [
            ("AAAA", Role::Held, None),
            ("BBBB", Role::Manual, None),
            ("CCCC", Role::Peer, Some("BBBB")),
        ]
    );
    let b = store.universe_entry("BBBB").unwrap().unwrap();
    assert_eq!(b.pack_override.as_deref(), Some("it_distribution"));
    assert_eq!(b.business_note.as_deref(), Some("Distributor sintetis"));
    assert_eq!(b.added_at, AT);
    assert_eq!(
        store.rebuild().unwrap(),
        4,
        "three upserts and one delete replay"
    );
}

#[test]
fn watchlist_csv_includes_peers_is_sorted_and_only_rewritten_on_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research/watchlist.csv");
    let store = Store::open_memory().unwrap();
    let packs = packs();
    sync(&store, &held(&["CCCC", "AAAA"]), AT).unwrap();
    add(&store, &packs, &manual("BBBB"), AT).unwrap();
    add(&store, &packs, &peer("DDDD", "AAAA"), AT).unwrap();
    let entries = store.universe().unwrap();

    assert!(write_watchlist(&path, &entries).unwrap());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "ticker\nAAAA\nBBBB\nCCCC\nDDDD\n",
        "peers are downloaded too (A2.6)"
    );
    assert!(
        !write_watchlist(&path, &entries).unwrap(),
        "unchanged → untouched"
    );
    assert!(!dir.path().join("research/watchlist.csv.tmp").exists());

    remove(&store, "BBBB", AT).unwrap();
    let entries = store.universe().unwrap();
    assert!(write_watchlist(&path, &entries).unwrap());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "ticker\nAAAA\nCCCC\nDDDD\n"
    );

    assert!(write_watchlist(&path, &[]).unwrap());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "ticker\n");
    let _ = UniverseEntry {
        ticker: "ZZZZ".into(),
        role: Role::Peer,
        peer_of: None,
        pack_override: None,
        business_note: None,
        added_at: AT.into(),
        synced_at: AT.into(),
    };
}

#[test]
fn rebuild_never_resurrects_a_sold_holding_as_held() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research.sqlite");
    let packs = packs();
    let store = Store::open(&path).unwrap();
    sync(&store, &held(&["AAAA"]), AT).unwrap();
    // A note written while held logs the row with role `held`.
    update(
        &store,
        &packs,
        "AAAA",
        &Patch {
            business_note: Some(Some("Catatan".into())),
            ..Default::default()
        },
        AT,
    )
    .unwrap();
    // Sold: the sync demotes it (not logged), then the log replays.
    sync(&store, &[], "2026-10-23T09:00:00+07:00").unwrap();
    assert_eq!(
        store.universe_entry("AAAA").unwrap().unwrap().role,
        Role::Manual
    );
    store.rebuild().unwrap();
    let a = store.universe_entry("AAAA").unwrap().unwrap();
    assert_eq!(a.role, Role::Manual, "held is the sync's to grant");
    assert_eq!(a.business_note.as_deref(), Some("Catatan"));
    // A stale `held` line cannot promote a manual row either.
    store
        .upsert_universe(&UniverseEntry {
            role: Role::Held,
            ..a.clone()
        })
        .unwrap();
    assert_eq!(
        store.universe_entry("AAAA").unwrap().unwrap().role,
        Role::Manual
    );
    // And an unchanged PUT writes nothing to the log.
    let before = std::fs::read_to_string(dir.path().join("research-log.jsonl")).unwrap();
    update(&store, &packs, "AAAA", &Patch::default(), AT).unwrap();
    let after = std::fs::read_to_string(dir.path().join("research-log.jsonl")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn peer_removal_cascades_and_direct_cycles_are_refused() {
    let store = Store::open_memory().unwrap();
    let packs = packs();
    sync(&store, &held(&["AAAA"]), AT).unwrap();
    add(&store, &packs, &manual("BBBB"), AT).unwrap();
    add(&store, &packs, &peer("CCCC", "BBBB"), AT).unwrap();
    add(&store, &packs, &peer("DDDD", "CCCC"), AT).unwrap();
    // BBBB is a peer of CCCC? CCCC is already BBBB's peer: a direct cycle.
    assert!(matches!(
        update(
            &store,
            &packs,
            "CCCC",
            &Patch {
                peers: Some(vec!["BBBB".into()]),
                ..Default::default()
            },
            AT
        ),
        Err(UniverseError::SelfPeer)
    ));
    assert!(matches!(
        add(&store, &packs, &peer("BBBB", "CCCC"), AT),
        Err(UniverseError::SelfPeer)
    ));
    // Removing BBBB takes CCCC (peer-only) and DDDD (peer-only of CCCC) with it.
    remove(&store, "BBBB", AT).unwrap();
    assert_eq!(roles(&store), vec![("AAAA".into(), Role::Held, None)]);
}
