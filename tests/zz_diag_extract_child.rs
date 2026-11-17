//! TEMPORARY diagnostics for the red CI on main (research_extract: every extraction child
//! fails). Runs the extraction child on a real PDF from a small parent and again after the
//! parent has touched 800 MiB, then fails on purpose so the output shows in the CI log.
//! Removed again by the fix.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn status_of(pid: &str) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .map(|s| {
            s.lines()
                .filter(|l| {
                    ["Threads", "VmPeak", "VmSize", "VmHWM", "VmRSS"]
                        .iter()
                        .any(|k| l.starts_with(k))
                })
                .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_else(|e| format!("({e})"))
}

fn run_child(pdf: &[u8]) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_investing-app"))
        .args(["__extract-text", "pdf", "--max-rss-mb", "512"])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the child");
    std::thread::sleep(Duration::from_millis(300));
    let early = child.try_wait().ok().flatten();
    let before = status_of(&child.id().to_string());
    let mut stdin = child.stdin.take().unwrap();
    let _ = stdin.write_all(pdf);
    drop(stdin);
    let mut out = String::new();
    let mut err = String::new();
    let _ = child.stdout.take().unwrap().read_to_string(&mut out);
    let _ = child.stderr.take().unwrap().read_to_string(&mut err);
    let status = child.wait().expect("wait");
    format!(
        "exited before stdin: {early:?}\n  child before stdin: {before}\n  exit: {status:?}\n  stdout: {}\n  stderr: {}",
        out.chars().take(160).collect::<String>(),
        err.chars().take(600).collect::<String>()
    )
}

#[test]
#[cfg_attr(not(target_os = "linux"), ignore = "reads /proc")]
fn diag_the_extraction_child_against_the_parents_memory() {
    let pdf = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/vault/finance/stock/holdings/AAAA/2024/ar-2024.pdf"
    ))
    .expect("fixture pdf");
    let small = format!("parent: {}\n  {}", status_of("self"), run_child(&pdf));
    // Touch 800 MiB in the parent, as a busy test harness or server might hold.
    let mut big = vec![0u8; 800 << 20];
    for i in (0..big.len()).step_by(4096) {
        big[i] = 1;
    }
    let after = format!("parent: {}\n  {}", status_of("self"), run_child(&pdf));
    drop(big);
    panic!(
        "\n== DIAG2 ==\nA (small parent) {small}\nB (parent touched 800 MiB) {after}\n== END DIAG2 ==\n"
    );
}
