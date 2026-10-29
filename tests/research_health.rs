//! R7.1 / R7.2: the weekly scraper job's run status (written by `deploy/scraper.sh`) shows on
//! `GET /api/health?deep=1`, and every research source reports status and age as data (A7.1,
//! A7.2). Synthetic data only, no network, no launchd.

mod common;

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Duration, Local, SecondsFormat, TimeZone, Utc};
use common::{test_app_with, TestApp};
use investing_app::research::health::{last_scheduled_run, scraper_job_entry_at};
use investing_app::research::kpi::KpiValue;
use investing_app::research::store::{SourceRun, Store};
use serde_json::{json, Value};

fn ago(d: Duration) -> String {
    (Utc::now() - d).to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn status_json(started: &str, finished: Option<&str>, code: Option<i64>, step: &str) -> String {
    json!({"started_at": started, "finished_at": finished, "exit_code": code, "step": step})
        .to_string()
}

/// App whose research dir holds a watchlist, an `idx.sqlite` of the given mtime age and,
/// when given, the scraper status file.
fn app_with_status(status: Option<String>) -> TestApp {
    let t = test_app_with(|_| {});
    let research = t.dir.path().join("research");
    std::fs::create_dir_all(&research).unwrap();
    std::fs::write(research.join("watchlist.csv"), "ticker\nAAAA\n").unwrap();
    std::fs::write(&t.cfg.idx_db_path, "x").unwrap();
    if let Some(s) = status {
        std::fs::write(research.join("scraper-status.json"), s).unwrap();
    }
    t
}

/// (started, finished) of a run that ended minutes before `now`: under a day old on any
/// weekday at any hour, and never `missed` (a start before the last Sunday 03:00 slot means
/// the slot is minutes old, well inside the grace period).
fn just_finished(now: DateTime<Utc>) -> (String, String) {
    let at = |d: Duration| (now - d).to_rfc3339_opts(SecondsFormat::Secs, true);
    (at(Duration::minutes(10)), at(Duration::minutes(3)))
}

async fn job(t: &TestApp) -> Value {
    let (_, h) = t.get("/api/health?deep=1").await;
    h["sources"]["research_scraper_job"].clone()
}

#[tokio::test]
async fn a_recent_successful_run_shows_time_exit_status_and_age() {
    let (started, fin) = just_finished(Utc::now());
    let t = app_with_status(Some(status_json(&started, Some(&fin), Some(0), "done")));
    let j = job(&t).await;
    assert_eq!(j["ok"], true, "{j}");
    assert_eq!(j["state"], "ok");
    assert_eq!(j["exit_code"], 0);
    assert_eq!(j["finished_at"], fin);
    assert_eq!(j["last_at"], fin);
    assert_eq!(j["age_days"], 0);
    assert_eq!(j["step"], "done");
    assert!(j["detail"].as_str().unwrap().contains("exit 0"));
}

#[tokio::test]
async fn a_failed_run_is_red_with_its_exit_code_and_step_even_when_idx_sqlite_is_fresh() {
    let t = app_with_status(Some(status_json(
        &ago(Duration::hours(30)),
        Some(&ago(Duration::hours(30))),
        Some(1),
        "download",
    )));
    let j = job(&t).await;
    assert_eq!(j["ok"], false, "{j}");
    assert_eq!(j["state"], "failed");
    assert_eq!(j["exit_code"], 1);
    assert_eq!(j["step"], "download");
    assert!(j["detail"]
        .as_str()
        .unwrap()
        .contains("exit 1 at step download"));

    let (_, h) = t.get("/api/health?deep=1").await;
    assert_eq!(h["status"], "degraded");
}

#[tokio::test]
async fn an_old_success_is_stale_and_a_run_that_never_finished_is_reported() {
    let t = app_with_status(Some(status_json(
        &ago(Duration::days(20)),
        Some(&ago(Duration::days(20))),
        Some(0),
        "done",
    )));
    let j = job(&t).await;
    assert_eq!(j["ok"], false);
    assert_eq!(j["state"], "stale");
    assert_eq!(j["age_days"], 20);

    // Started an hour ago, no finish yet: running, still ok.
    let t = app_with_status(Some(status_json(
        &ago(Duration::hours(1)),
        None,
        None,
        "download",
    )));
    let j = job(&t).await;
    assert_eq!(j["ok"], true, "{j}");
    assert_eq!(j["state"], "running");
    assert_eq!(j["exit_code"], Value::Null);

    // Started two days ago and never finished: killed or crashed.
    let t = app_with_status(Some(status_json(
        &ago(Duration::days(2)),
        None,
        None,
        "download",
    )));
    let j = job(&t).await;
    assert_eq!(j["ok"], false);
    assert_eq!(j["state"], "unfinished");
    assert!(j["detail"].as_str().unwrap().contains("did not finish"));
}

#[tokio::test]
async fn a_corrupt_status_file_is_red_not_ignored() {
    let t = app_with_status(Some("{not json".into()));
    let j = job(&t).await;
    assert_eq!(j["ok"], false);
    assert_eq!(j["state"], "unreadable");
}

#[tokio::test]
async fn without_a_status_file_the_idx_sqlite_age_is_the_fallback() {
    let t = app_with_status(None);
    let j = job(&t).await;
    assert_eq!(j["ok"], true, "{j}");
    assert_eq!(j["state"], "no_status");
    assert_eq!(j["exit_code"], Value::Null);
    assert_eq!(j["age_days"], Value::Null);
    assert_eq!(j["idx_age_days"], 0);
    assert!(j["detail"].as_str().unwrap().contains("no run status file"));

    // No watchlist and no status: the job cannot have run.
    let t = test_app_with(|_| {});
    let j = job(&t).await;
    assert_eq!(j["ok"], false);
    assert!(j["detail"].as_str().unwrap().contains("watchlist"));
}

/// The real `deploy/scraper.sh` against a stub scraper: the file it writes is the one health reads.
fn run_script(data: &Path, scraper: &Path) -> i32 {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/scraper.sh");
    Command::new("bash")
        .arg(script)
        // HOME too: any default the script falls back to stays in the test dir.
        .env("HOME", data)
        .env("INVESTING_DATA_DIR", data)
        .env("INVESTING_IDX_SCRAPER_DIR", scraper)
        .env("PYTHON", "python3")
        .env_remove("INVESTING_RESEARCH_WATCHLIST")
        .output()
        .unwrap()
        .status
        .code()
        .unwrap()
}

#[tokio::test]
async fn scraper_sh_writes_the_status_that_health_reads() {
    let t = test_app_with(|_| {});
    let data = t.dir.path();
    let scr = data.join("scraper");
    std::fs::create_dir_all(scr.join("model")).unwrap();
    std::fs::create_dir_all(data.join("research")).unwrap();
    std::fs::write(data.join("research/watchlist.csv"), "ticker\nAAAA\n").unwrap();
    std::fs::write(
        scr.join("run.py"),
        "import sys\nassert sys.argv[1:3] == ['--skip-master', '--watchlist'], sys.argv\n",
    )
    .unwrap();
    std::fs::write(scr.join("model/sqlite_export.py"), "").unwrap();
    std::fs::write(&t.cfg.idx_db_path, "x").unwrap();

    assert_eq!(run_script(data, &scr), 0);
    let j = job(&t).await;
    assert_eq!(j["ok"], true, "{j}");
    assert_eq!(j["exit_code"], 0);
    assert_eq!(j["step"], "done");
    assert_eq!(j["age_days"], 0);

    // The download step fails: exit code and step reach health; nothing is hidden.
    std::fs::write(scr.join("run.py"), "import sys\nsys.exit(3)\n").unwrap();
    assert_eq!(run_script(data, &scr), 3);
    let j = job(&t).await;
    assert_eq!(j["ok"], false, "{j}");
    assert_eq!(j["exit_code"], 3);
    assert_eq!(j["step"], "download");

    // Preflight failure (watchlist gone) is a recorded run too.
    std::fs::remove_file(data.join("research/watchlist.csv")).unwrap();
    assert_eq!(run_script(data, &scr), 78);
    let j = job(&t).await;
    assert_eq!(j["exit_code"], 78);
    assert_eq!(j["step"], "preflight");
    assert_eq!(j["ok"], false);
}

#[tokio::test]
async fn every_research_source_reports_status_and_age_as_data() {
    let t = test_app_with(|_| {});
    {
        let s = Store::open(&t.cfg.research_db_path).unwrap();
        for (source, days) in [("yahoo", 1), ("xbrl_dir", 3), ("drivers", 2)] {
            let at = ago(Duration::days(days));
            s.record_source_run(&SourceRun {
                source: source.into(),
                ticker: None,
                started_at: at.clone(),
                finished_at: Some(at),
                ok: true,
                rows: Some(1),
                error: None,
            })
            .unwrap();
        }
        s.set_kpi(&KpiValue {
            ticker: "AAAA".into(),
            metric: "units".into(),
            fy: 2026,
            q: 1,
            value: Some(10.0),
            unknown: false,
            unit: "unit".into(),
            document: "Laporan sintetis".into(),
            document_date: "2026-04-28".into(),
            entered_at: ago(Duration::days(4)),
        })
        .unwrap();
    }
    let (_, h) = t.get("/api/health?deep=1").await;
    let src = &h["sources"];
    // A7.2: filings, facts, prices, drivers, KPIs, scraper job, research store.
    for (key, age) in [
        ("research_prices", 1),
        ("research_filings", 3),
        ("research_facts", 3),
        ("research_xbrl", 3),
        ("research_drivers", 2),
        ("research_kpi", 4),
        ("research_store", 1),
    ] {
        let e = &src[key];
        assert!(e["ok"].is_boolean(), "{key}: {e}");
        assert!(e["detail"].is_string(), "{key}: {e}");
        assert_eq!(e["age_days"], age, "{key}: {e}");
        assert!(e["last_at"].is_string(), "{key}: {e}");
    }
    let j = &src["research_scraper_job"];
    assert!(j["ok"].is_boolean() && j["state"].is_string(), "{j}");
    assert!(
        j.get("age_days").is_some() && j.get("exit_code").is_some(),
        "{j}"
    );
}

#[test]
fn without_a_store_the_age_fields_exist_and_are_null() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = investing_app::config::Config::for_tests(dir.path());
    cfg.research_db_path = dir.path().join("missing.sqlite");
    let src = investing_app::research::health::sources(&cfg);
    for key in [
        "research_store",
        "research_prices",
        "research_filings",
        "research_facts",
        "research_drivers",
        "research_kpi",
    ] {
        let e = &src[key];
        assert_eq!(e["ok"], false, "{key}");
        assert_eq!(e["age_days"], Value::Null, "{key}");
        assert_eq!(e["last_at"], Value::Null, "{key}");
    }
}

// ---- QA P4 fixes: signals, lock, unwritable status, files fetched, missed, clock skew ----

use std::process::{Child, Stdio};
use std::time::Instant;

/// Data dir, stub scraper dir and the research dir of a script run.
struct Rig {
    t: TestApp,
    scr: std::path::PathBuf,
}

impl Rig {
    fn new(run_py: &str) -> Rig {
        let t = test_app_with(|_| {});
        let data = t.dir.path().to_path_buf();
        let scr = data.join("scraper");
        std::fs::create_dir_all(scr.join("model")).unwrap();
        std::fs::create_dir_all(data.join("research")).unwrap();
        std::fs::write(data.join("research/watchlist.csv"), "ticker\nAAAA\n").unwrap();
        std::fs::write(scr.join("run.py"), run_py).unwrap();
        std::fs::write(scr.join("model/sqlite_export.py"), "").unwrap();
        std::fs::write(&t.cfg.idx_db_path, "x").unwrap();
        Rig { t, scr }
    }
    fn research(&self) -> std::path::PathBuf {
        self.t.dir.path().join("research")
    }
    fn status_path(&self) -> std::path::PathBuf {
        self.research().join("scraper-status.json")
    }
    fn status(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.status_path()).unwrap()).unwrap()
    }
    fn command(&self) -> Command {
        let mut c = Command::new("bash");
        c.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/scraper.sh"))
            .env("HOME", self.t.dir.path())
            .env("INVESTING_DATA_DIR", self.t.dir.path())
            .env("INVESTING_IDX_SCRAPER_DIR", &self.scr)
            .env("PYTHON", "python3")
            .env("PIDFILE", self.t.dir.path().join("stub.pid"))
            .env("GATE", self.t.dir.path().join("gate"))
            .env_remove("INVESTING_RESEARCH_WATCHLIST")
            .env_remove("INVESTING_IDX_XBRL_DIR")
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        c
    }
    fn spawn(&self) -> Child {
        self.command().spawn().unwrap()
    }
    fn wait_status(&self, f: impl Fn(&Value) -> bool) -> Value {
        let end = Instant::now() + std::time::Duration::from_secs(20);
        loop {
            if let Ok(t) = std::fs::read_to_string(self.status_path()) {
                if let Ok(v) = serde_json::from_str::<Value>(&t) {
                    if f(&v) {
                        return v;
                    }
                }
            }
            assert!(
                Instant::now() < end,
                "status never reached the wanted state"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

const SLEEPER: &str =
    "import os, time\nopen(os.environ['PIDFILE'], 'w').write(str(os.getpid()))\ntime.sleep(60)\n";
const GATED: &str =
    "import os, time\nwhile not os.path.exists(os.environ['GATE']):\n    time.sleep(0.05)\n";

fn pid_alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[tokio::test]
async fn term_hup_and_int_record_a_failure_the_step_and_stop_the_scrape() {
    for (sig, code) in [("TERM", 143), ("HUP", 129), ("INT", 130)] {
        let r = Rig::new(SLEEPER);
        let child = r.spawn();
        r.wait_status(|v| v["step"] == "download" && r.t.dir.path().join("stub.pid").exists());
        let pid = std::fs::read_to_string(r.t.dir.path().join("stub.pid")).unwrap();
        Command::new("kill")
            .args([format!("-{sig}"), child.id().to_string()])
            .status()
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(code), "SIG{sig}");
        let st = r.status();
        assert_eq!(st["exit_code"], code, "SIG{sig}: {st}");
        assert_eq!(st["step"], "download", "SIG{sig}: {st}");
        assert!(st["finished_at"].is_string(), "SIG{sig}: {st}");
        assert!(
            !pid_alive(pid.trim()),
            "SIG{sig}: the scraper child outlived the job"
        );
        assert!(
            !r.research().join("scraper.lock").exists(),
            "lock left behind"
        );
        let j = job(&r.t).await;
        assert_eq!(j["state"], "failed", "SIG{sig}: {j}");
        assert_eq!(j["ok"], false);
        assert_eq!(j["exit_code"], code);
    }
}

#[tokio::test]
async fn a_killed_run_shows_the_step_it_died_in() {
    let r = Rig::new(SLEEPER);
    let mut child = r.spawn();
    r.wait_status(|v| v["step"] == "download");
    child.kill().unwrap(); // SIGKILL: no trap can run
    let _ = child.wait();
    let pid = std::fs::read_to_string(r.t.dir.path().join("stub.pid")).unwrap_or_default();
    if !pid.trim().is_empty() {
        let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
    }
    let st = r.status();
    assert_eq!(st["step"], "download");
    assert_eq!(st["finished_at"], Value::Null);
}

#[tokio::test]
async fn an_overlapping_run_exits_early_and_leaves_the_status_alone() {
    let r = Rig::new(GATED);
    let a = r.spawn();
    let before = r.wait_status(|v| v["step"] == "download");
    let raw_before = std::fs::read_to_string(r.status_path()).unwrap();

    let b = r.command().output().unwrap();
    assert_eq!(
        b.status.code(),
        Some(75),
        "{}",
        String::from_utf8_lossy(&b.stderr)
    );
    assert!(String::from_utf8_lossy(&b.stderr).contains("another run"));
    assert_eq!(
        std::fs::read_to_string(r.status_path()).unwrap(),
        raw_before
    );
    assert_eq!(before["finished_at"], Value::Null);

    std::fs::write(r.t.dir.path().join("gate"), "").unwrap();
    let a = a.wait_with_output().unwrap();
    assert_eq!(a.status.code(), Some(0));
    let st = r.status();
    assert_eq!(
        (st["exit_code"].as_i64(), st["step"].as_str()),
        (Some(0), Some("done"))
    );
    assert!(!r.research().join("scraper.lock").exists());
}

#[tokio::test]
async fn a_stale_lock_is_replaced() {
    let r = Rig::new("pass\n");
    // A lock held by a process that no longer exists.
    let mut dead = Command::new("true").spawn().unwrap();
    let dead_pid = dead.id();
    dead.wait().unwrap();
    std::fs::create_dir(r.research().join("scraper.lock")).unwrap();
    std::fs::write(r.research().join("scraper.lock/pid"), dead_pid.to_string()).unwrap();
    let out = r.command().output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(r.status()["step"], "done");
}

#[tokio::test]
async fn a_status_that_cannot_be_written_fails_the_run_and_says_so() {
    use std::os::unix::fs::PermissionsExt;
    let ro = |p: &Path, mode| {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap()
    };

    // Unwritable from the start: nothing runs, exit 74, stderr says why.
    let r = Rig::new("raise SystemExit('must not run')\n");
    ro(&r.research(), 0o555);
    let out = r.command().output().unwrap();
    ro(&r.research(), 0o755);
    assert_eq!(out.status.code(), Some(74));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not recorded"));
    assert!(!r.status_path().exists());

    // The disk fills (here: the dir turns read-only) during a scrape that itself succeeds.
    let stub = format!(
        "import os\nos.chmod({:?}, 0o555)\n",
        r.research().to_string_lossy()
    );
    std::fs::write(r.scr.join("run.py"), stub).unwrap();
    let out = r.command().output().unwrap();
    ro(&r.research(), 0o755);
    assert_eq!(
        out.status.code(),
        Some(74),
        "success must not hide an unrecorded run"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("NOT recorded"));

    // ...and a real failure keeps its own exit code.
    let stub = format!(
        "import os, sys\nos.chmod({:?}, 0o555)\nsys.exit(3)\n",
        r.research().to_string_lossy()
    );
    std::fs::write(r.scr.join("run.py"), stub).unwrap();
    let out = r.command().output().unwrap();
    ro(&r.research(), 0o755);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("NOT recorded"));
}

#[tokio::test]
async fn the_run_records_how_many_new_files_it_fetched_and_zero_is_not_a_failure() {
    let r = Rig::new(
        "import os\nd = os.path.join(os.getcwd(), 'data/XBRL/AAAA')\nos.makedirs(d, exist_ok=True)\nfor n in ('a.xbrl', 'b.xbrl'):\n    open(os.path.join(d, n), 'w').write('x')\n",
    );
    assert_eq!(r.command().output().unwrap().status.code(), Some(0));
    assert_eq!(r.status()["new_files"], 2);
    let j = job(&r.t).await;
    assert_eq!(
        (j["ok"].clone(), j["new_files"].clone()),
        (json!(true), json!(2)),
        "{j}"
    );
    assert!(j["detail"].as_str().unwrap().contains("2 new files"), "{j}");

    // Next week nothing new came out: still exit 0, still ok, and the 0 is shown.
    std::fs::write(r.scr.join("run.py"), "pass\n").unwrap();
    assert_eq!(r.command().output().unwrap().status.code(), Some(0));
    assert_eq!(r.status()["new_files"], 0);
    let j = job(&r.t).await;
    assert_eq!(j["ok"], true, "{j}");
    assert_eq!(j["state"], "ok");
    assert_eq!(j["new_files"], 0);
    assert!(j["detail"].as_str().unwrap().contains("0 new files"), "{j}");
}

fn local(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(y, m, d, h, mi, 0)
        .earliest()
        .unwrap()
}
fn utc(t: DateTime<Local>) -> String {
    t.with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// The health entry for a status file at a pinned clock.
fn entry_at(status: String, now: DateTime<Local>) -> Value {
    let t = app_with_status(Some(status));
    scraper_job_entry_at(&t.cfg, now)
}

fn ok_run(at: DateTime<Local>) -> String {
    status_json(
        &utc(at),
        Some(&utc(at + Duration::minutes(7))),
        Some(0),
        "done",
    )
}

#[test]
fn the_schedule_constants_match_the_plist_and_the_slot_is_sunday_0300_local() {
    let plist = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/com.menganto.investing.scraper.plist"),
    )
    .unwrap();
    let squashed: String = plist.split_whitespace().collect();
    assert!(squashed.contains("<key>Weekday</key><integer>0</integer>"));
    assert!(squashed.contains("<key>Hour</key><integer>3</integer>"));
    assert!(squashed.contains("<key>Minute</key><integer>0</integer>"));
    // 2026-10-25 is a Sunday.
    assert_eq!(
        last_scheduled_run(local(2026, 10, 25, 3, 0)),
        local(2026, 10, 25, 3, 0)
    );
    assert_eq!(
        last_scheduled_run(local(2026, 10, 25, 2, 59)),
        local(2026, 10, 18, 3, 0)
    );
    assert_eq!(
        last_scheduled_run(local(2026, 10, 28, 12, 0)),
        local(2026, 10, 25, 3, 0)
    );
}

#[test]
fn a_status_older_than_the_last_slot_is_missed_after_the_grace_period() {
    let last_week = ok_run(local(2026, 10, 18, 3, 0));
    // Wednesday: the Sunday run never recorded anything.
    let j = entry_at(last_week.clone(), local(2026, 10, 28, 12, 0));
    assert_eq!(
        (j["state"].as_str(), j["ok"].as_bool()),
        (Some("missed"), Some(false)),
        "{j}"
    );
    assert_eq!(j["exit_code"], 0);
    // Sunday 05:00: the slot just passed, the run may still be coming (grace).
    let j = entry_at(last_week.clone(), local(2026, 10, 25, 5, 0));
    assert_eq!(j["state"], "ok", "{j}");
    // Sunday 10:00: past the grace period.
    let j = entry_at(last_week, local(2026, 10, 25, 10, 0));
    assert_eq!(j["state"], "missed", "{j}");
    // A run that did record itself this week is fine, also late in the week.
    let j = entry_at(
        ok_run(local(2026, 10, 25, 3, 0)),
        local(2026, 10, 31, 12, 0),
    );
    assert_eq!(j["state"], "ok", "{j}");
    // A late start after wake counts for the slot.
    let j = entry_at(
        ok_run(local(2026, 10, 25, 7, 30)),
        local(2026, 10, 28, 12, 0),
    );
    assert_eq!(j["state"], "ok", "{j}");
}

#[test]
fn a_recent_successful_run_is_ok_with_age_zero_on_every_weekday_and_hour() {
    let t = app_with_status(None);
    let status_file = t.dir.path().join("research/scraper-status.json");
    // Monday 2026-10-26 to Sunday 2026-11-01; the hours straddle the Sunday 03:00 slot and the
    // end of its grace period, the minutes the hour boundaries.
    let mut checked = 0;
    for day in 0..7 {
        for hour in [0, 2, 3, 8, 9, 12, 23] {
            for minute in [0, 5, 59] {
                let now = local(2026, 10, 26, hour, minute) + Duration::days(day);
                let (started, fin) = just_finished(now.with_timezone(&Utc));
                std::fs::write(
                    &status_file,
                    status_json(&started, Some(&fin), Some(0), "done"),
                )
                .unwrap();
                let j = scraper_job_entry_at(&t.cfg, now);
                assert_eq!(
                    (j["state"].as_str(), j["ok"].as_bool()),
                    (Some("ok"), Some(true)),
                    "{now}: {j}"
                );
                assert_eq!(j["exit_code"], 0, "{now}: {j}");
                assert_eq!(j["finished_at"], fin, "{now}: {j}");
                assert_eq!(j["last_at"], fin, "{now}: {j}");
                assert_eq!(j["age_days"], 0, "{now}: {j}");
                assert_eq!(j["age_hours"], 0, "{now}: {j}");
                assert!(
                    j["detail"].as_str().unwrap().contains("exit 0"),
                    "{now}: {j}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 7 * 7 * 3);
}

#[test]
fn the_stale_line_is_exactly_fourteen_days() {
    let now = local(2026, 10, 28, 12, 0);
    let fin = |d: Duration| {
        let at = now - d;
        status_json(&utc(at), Some(&utc(at)), Some(0), "done")
    };
    let j = entry_at(fin(Duration::days(14) - Duration::minutes(1)), now);
    assert_ne!(j["state"], "stale", "{j}");
    let j = entry_at(fin(Duration::days(14) + Duration::minutes(1)), now);
    assert_eq!(
        (j["state"].as_str(), j["ok"].as_bool()),
        (Some("stale"), Some(false)),
        "{j}"
    );
    // 14 d 23 h used to pass as "14 days"; it is stale.
    let j = entry_at(fin(Duration::days(14) + Duration::hours(23)), now);
    assert_eq!(j["state"], "stale", "{j}");
}

#[tokio::test]
async fn timestamps_from_the_future_are_clock_skew_with_no_negative_ages() {
    for status in [
        status_json(
            "2099-01-01T00:00:00Z",
            Some("2099-01-01T00:05:00Z"),
            Some(0),
            "done",
        ),
        status_json("2099-01-01T00:00:00Z", None, None, "download"),
        status_json(
            &ago(Duration::hours(1)),
            Some("2099-01-01T00:05:00Z"),
            Some(0),
            "done",
        ),
    ] {
        let j = job(&app_with_status(Some(status.clone()))).await;
        assert_eq!(j["state"], "clock_skew", "{status}: {j}");
        assert_eq!(j["ok"], false);
        assert_eq!(j["age_days"], Value::Null);
        assert_eq!(j["age_hours"], Value::Null);
        assert!(!j["detail"].as_str().unwrap().contains(" ago"), "{j}");
    }
    // A minute ahead is within tolerance: a state, never a negative age.
    let ahead = (Utc::now() + Duration::minutes(1)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let j = job(&app_with_status(Some(status_json(
        &ahead,
        Some(&ahead),
        Some(0),
        "done",
    ))))
    .await;
    assert_ne!(j["state"], "clock_skew", "{j}");
    assert!(j["age_days"].as_i64().unwrap() >= 0 && j["age_hours"].as_i64().unwrap() >= 0);
}

#[tokio::test]
async fn an_empty_research_store_has_a_null_age_not_zero() {
    let t = test_app_with(|_| {});
    drop(Store::open(&t.cfg.research_db_path).unwrap());
    let (_, h) = t.get("/api/health?deep=1").await;
    let s = &h["sources"]["research_store"];
    assert_eq!(s["age_days"], Value::Null, "{s}");
    assert_eq!(s["last_at"], Value::Null, "{s}");
}

#[tokio::test]
async fn r8_documents_and_proposals_report_folder_counts_and_last_extraction() {
    let t = test_app_with(|_| {});
    let store = Store::open(&t.cfg.research_db_path).unwrap();
    drop(store);
    let (_, h) = t.get("/api/health?deep=1").await;
    let d = &h["sources"]["research_documents"];
    assert_eq!(d["ok"], true, "{d}");
    assert_eq!(d["documents"], 0, "{d}");
    let p = &h["sources"]["research_proposals"];
    assert_eq!(p["ok"], true, "{p}");
    assert_eq!(p["pending"], 0, "{p}");
    assert_eq!(p["last_at"], Value::Null, "{p}");

    std::fs::create_dir_all(&t.cfg.research_documents_dir).unwrap();
    std::fs::write(t.cfg.research_documents_dir.join("abc.pdf"), "x").unwrap();
    let c = rusqlite::Connection::open(&t.cfg.research_db_path).unwrap();
    let at = ago(Duration::days(3));
    c.execute(
        "INSERT INTO documents (id, ticker, kind, added_at) VALUES ('d1', 'AAAA', 'pdf', ?1)",
        [&at],
    )
    .unwrap();
    for (id, status) in [("p1", "pending"), ("p2", "accepted")] {
        c.execute(
            "INSERT INTO extraction_proposals (id, document_id, ticker, kpi_key, value, unit, page, snippet, rank, status, created_at)
             VALUES (?1, 'd1', 'AAAA', 'k:2025Q1', 1.0, 'x', 1, 's', 1, ?2, ?3)",
            rusqlite::params![id, status, at],
        )
        .unwrap();
    }
    drop(c);
    let (_, h) = t.get("/api/health?deep=1").await;
    let d = &h["sources"]["research_documents"];
    assert_eq!(
        (d["ok"].clone(), d["files"].clone(), d["documents"].clone()),
        (json!(true), json!(1), json!(1)),
        "{d}"
    );
    let p = &h["sources"]["research_proposals"];
    assert_eq!(
        (
            p["pending"].clone(),
            p["total"].clone(),
            p["age_days"].clone()
        ),
        (json!(1), json!(2), json!(3)),
        "{p}"
    );
}

#[tokio::test]
async fn a_documents_path_that_is_a_file_is_not_ok() {
    let t = test_app_with(|_| {});
    drop(Store::open(&t.cfg.research_db_path).unwrap());
    std::fs::create_dir_all(t.cfg.research_documents_dir.parent().unwrap()).unwrap();
    std::fs::write(&t.cfg.research_documents_dir, "x").unwrap();
    let (_, h) = t.get("/api/health?deep=1").await;
    assert_eq!(h["sources"]["research_documents"]["ok"], false);
}
