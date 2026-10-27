use investing_app::config::{self, Config};
use investing_app::research::store::Store;
use investing_app::research::xbrl;

/// `investing research-ingest --xbrl-dir DIR [--pdf-dir DIR] [--db PATH] [--reingest]` (research
/// plan §4.2, R1 exit check). Ingests every new or changed `.xbrl` file under DIR, and every
/// original-filing `.json` file under `--pdf-dir` (default: `INVESTING_PDF_FILINGS_DIR`, else
/// `<data dir>/research/pdf-filings`; a missing directory is skipped), into the research store
/// at PATH (default: the configured store) and prints what happened: filings on file, newly
/// ingested, unreadable files with their error, and every reconciliation failure with a
/// one-line explanation. `--reingest` re-parses files whose hash is already stored, to rebuild
/// facts after a parser change. Never touches DIR. Use `--db` on a copy or a temp path to
/// inspect a directory without writing to the live store.
fn research_ingest(args: &[String]) -> i32 {
    let mut reingest = false;
    let mut dir: Option<std::path::PathBuf> = None;
    let mut db: Option<std::path::PathBuf> = None;
    let mut pdf_dir: Option<std::path::PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--xbrl-dir" => {
                i += 1;
                match args.get(i) {
                    Some(p) => dir = Some(p.into()),
                    None => {
                        eprintln!("investing research-ingest: --xbrl-dir needs a path");
                        return 2;
                    }
                }
            }
            "--pdf-dir" => {
                i += 1;
                match args.get(i) {
                    Some(p) => pdf_dir = Some(p.into()),
                    None => {
                        eprintln!("investing research-ingest: --pdf-dir needs a path");
                        return 2;
                    }
                }
            }
            "--db" => {
                i += 1;
                match args.get(i) {
                    Some(p) => db = Some(p.into()),
                    None => {
                        eprintln!("investing research-ingest: --db needs a path");
                        return 2;
                    }
                }
            }
            "--reingest" => reingest = true,
            other => {
                eprintln!("investing research-ingest: unknown argument: {other}");
                return 2;
            }
        }
        i += 1;
    }
    let Some(dir) = dir else {
        eprintln!("investing research-ingest: --xbrl-dir is required");
        return 2;
    };
    if !dir.is_dir() {
        eprintln!(
            "investing research-ingest: {} is not a directory",
            dir.display()
        );
        return 2;
    }
    let db = db.unwrap_or_else(|| config::research_db_path(|k| std::env::var(k).ok()));
    let store = match Store::open(&db) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("investing research-ingest: cannot open {}: {e}", db.display());
            return 1;
        }
    };
    let now = chrono::Utc::now();
    // A directory the owner named (flag or variable) must exist: a typo would otherwise read
    // as an empty source. Only the default location is optional.
    let named = pdf_dir.is_some()
        || std::env::var("INVESTING_PDF_FILINGS_DIR").is_ok_and(|v| !v.trim().is_empty());
    let pdf_dir = pdf_dir.unwrap_or_else(|| config::pdf_filings_dir(|k| std::env::var(k).ok()));
    if named && !pdf_dir.is_dir() {
        eprintln!(
            "investing research-ingest: {} is not a directory (--pdf-dir or INVESTING_PDF_FILINGS_DIR)",
            pdf_dir.display()
        );
        return 2;
    }
    let ingested = if reingest {
        xbrl::reingest_all_with_pdf(&store, &dir, Some(&pdf_dir), now)
    } else {
        xbrl::ingest_new_with_pdf(&store, &dir, Some(&pdf_dir), now, None)
    };
    let report = match ingested {
        Ok(r) => r,
        Err(e) => {
            eprintln!("investing research-ingest: {e}");
            return 1;
        }
    };
    // A rules bump never reaches filings whose hash is known: re-derive those tickers too.
    // A failure here is reported after the ingest summary, not instead of it.
    let rederived = match xbrl::rederive_stale(&store, &chrono::Utc::now().to_rfc3339()) {
        Ok(r) => r,
        Err(e) => xbrl::RederiveReport {
            failed: vec![("(stale-ticker query)".into(), e.to_string())],
            ..Default::default()
        },
    };
    let filings = store.filing_count().unwrap_or(0);
    let facts = store.fact_count().unwrap_or(0);
    println!(
        "store {}: {filings} filings, {facts} facts; this pass scanned {}, ingested {}, unchanged {}, unreadable {}",
        db.display(),
        report.scanned,
        report.ingested.len(),
        report.unchanged,
        report.unparsed.len()
    );
    println!(
        "pdf filings {}: {} in store, {} replaced or kept out by an XBRL filing of the same id",
        pdf_dir.display(),
        store
            .pdf_filing_count()
            .map_or_else(|e| format!("unreadable ({e})"), |n| n.to_string()),
        report.pdf_superseded.len()
    );
    for id in &report.pdf_superseded {
        println!("SUPERSEDED {id}: an XBRL filing of the same id wins");
    }
    println!(
        "rules {}: re-derived {} stale ticker(s), {} reconciliation failure(s) among them",
        investing_app::research::derive::RULES_VERSION,
        rederived.tickers.len(),
        rederived.reconciliation_failures.len()
    );
    for t in &rederived.tickers {
        println!("REDERIVED {t}");
    }
    for (t, e) in &rederived.failed {
        println!("REDERIVE FAILED {t}: {e}");
    }
    for (path, err) in &report.unparsed {
        println!("UNREADABLE {path}: {err}");
    }
    // A rejected file that has not changed since an earlier pass is not read again; it stays
    // on record, so list it here too instead of letting a second run look clean.
    for e in store.ingest_errors().unwrap_or_default() {
        if !report.unparsed.iter().any(|(p, _)| *p == e.source_path) {
            println!("UNREADABLE (on record) {}: {}", e.source_path, e.error);
        }
    }
    let stored_failures = store.reconciliation_failures().unwrap_or_default();
    println!(
        "reconciliation failures in store: {} ({} new this pass)",
        stored_failures.len(),
        report.reconciliation_failures.len()
    );
    let restated_years = store.restated_years().unwrap_or_else(|e| {
        println!("WARNING restated years unreadable: {e}");
        Vec::new()
    });
    println!(
        "restated years in store: {} (ticker-years explained by the restatements table, not failures)",
        restated_years.len()
    );
    for (ticker, fy) in &restated_years {
        println!("RESTATED {ticker}: FY{fy}");
    }
    for (ticker, f) in &report.reconciliation_failures {
        println!("RECONCILE {ticker}: {}", f.explain());
    }
    for (ticker, element, fy) in stored_failures.iter().filter(|(t, e, fy)| {
        !report
            .reconciliation_failures
            .iter()
            .any(|(rt, rf)| rt == t && rf.element == *e && rf.fy == *fy)
    }) {
        println!("RECONCILE {ticker}: {element} FY{fy} (from an earlier pass)");
    }
    for (id, w) in &report.warnings {
        println!("WARNING {id}: {w}");
    }
    // Unreadable files and re-derive failures are a non-zero exit so the check can be scripted.
    if !report.unparsed.is_empty() {
        3
    } else if !rederived.failed.is_empty() {
        4
    } else {
        0
    }
}

#[tokio::main]
async fn main() {
    let argv: Vec<String> = std::env::args().collect();
    // The text extractor child (see `research::extract::isolated`): first thing, before `.env`
    // and the server config, so a hostile upload is parsed by a process with an empty
    // environment.
    if argv.len() >= 2 && argv[1] == investing_app::research::extract::isolated::SUBCOMMAND {
        std::process::exit(investing_app::research::extract::isolated::child_main(
            &argv[2..],
        ));
    }
    let _ = dotenvy::dotenv();
    // CLI subcommands dispatch before Config::from_env so they run without the server env.
    if argv.len() >= 2 && argv[1] == "research-ingest" {
        std::process::exit(research_ingest(&argv[2..]));
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "investing_app=info,tower_http=info".into()),
        )
        .init();

    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("investing-app: refusing to start: {e}");
            std::process::exit(2);
        }
    };
    let bind = cfg.bind;
    let app = investing_app::build_app(cfg);
    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("investing-app: cannot bind {bind}: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!("listening on http://{bind}");
    axum::serve(listener, app).await.expect("server error");
}
