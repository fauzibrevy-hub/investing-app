//! TEMPORARY diagnostics for the red CI on main (research_extract: every extraction child
//! fails). Prints what the child process looks like on the runner, then fails on purpose so
//! the output shows in the CI log. Removed again by the fix.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn sh(cmd: &str) -> String {
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|e| format!("({e})"))
}

fn proc_lines(pid: u32, file: &str, keys: &[&str]) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/{file}"))
        .map(|s| {
            s.lines()
                .filter(|l| keys.iter().any(|k| l.starts_with(k)))
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_else(|e| format!("({e})"))
}

#[test]
#[cfg_attr(not(target_os = "linux"), ignore = "reads /proc of the child")]
fn diag_the_extraction_child_on_this_runner() {
    let exe = env!("CARGO_BIN_EXE_investing-app");
    let mut child = Command::new(exe)
        .args(["__extract-text", "pdf", "--max-rss-mb", "512"])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the child");
    std::thread::sleep(Duration::from_millis(700));
    let pid = child.id();
    let status_before = proc_lines(pid, "status", &["Threads", "VmPeak", "VmSize", "VmRSS"]);
    let limits = proc_lines(pid, "limits", &["Max address space", "Max core"]);
    let maps = sh(&format!("wc -l < /proc/{pid}/maps"));
    let mut stdin = child.stdin.take().unwrap();
    let _ = stdin.write_all(b"%PDF-1.4 truncated");
    drop(stdin);
    let mut out = String::new();
    let mut err = String::new();
    let _ = child.stdout.take().unwrap().read_to_string(&mut out);
    let _ = child.stderr.take().unwrap().read_to_string(&mut err);
    let status = child.wait().expect("wait");
    let report = format!(
        "\n== DIAG extraction child ==\n\
         available_parallelism: {:?}\nnproc: {} | nproc --all: {}\ncgroup cpu.max: {}\n\
         kernel: {}\nglibc: {}\nulimit -v (test process): {}\n\
         child status before stdin: {status_before}\nchild limits: {limits}\nchild maps lines: {maps}\n\
         child exit: {status:?}\nchild stdout: {}\nchild stderr: {}\n== END DIAG ==\n",
        std::thread::available_parallelism(),
        sh("nproc"),
        sh("nproc --all"),
        sh("cat /sys/fs/cgroup/cpu.max 2>/dev/null"),
        sh("uname -r"),
        sh("ldd --version 2>&1 | head -1"),
        sh("ulimit -v"),
        out.chars().take(300).collect::<String>(),
        err.chars().take(1500).collect::<String>(),
    );
    panic!("{report}");
}
