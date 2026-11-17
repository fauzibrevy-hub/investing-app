//! The extraction child's memory watchdog measures the child, not its parent.
//!
//! On Linux `getrusage`'s `ru_maxrss` of a freshly started program begins at the peak of the
//! process before `exec`: for a child started with posix_spawn (vfork), the parent's. Measured
//! on the CI runner: from a parent that had touched 800 MiB, the child exited 4 (its watchdog)
//! before it read a byte, so a server that once held more than the 512 MiB limit would answer
//! every upload with 422 `file_unreadable`.

#[cfg(target_os = "linux")]
#[test]
fn the_child_answers_whatever_its_parent_has_used() {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    // The parent (this test process) holds 800 MiB, every page touched.
    let mut big = vec![0u8; 800 << 20];
    big.iter_mut().step_by(4096).for_each(|b| *b = 1);
    let mut child = Command::new(env!("CARGO_BIN_EXE_investing-app"))
        .args(["__extract-text", "pdf", "--max-rss-mb", "512"])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the extraction child");
    std::thread::sleep(std::time::Duration::from_millis(300)); // its watchdog looks every 10 ms
    let mut stdin = child.stdin.take().expect("stdin");
    let _ = stdin.write_all(b"%PDF-1.4 truncated");
    drop(stdin);
    let mut out = String::new();
    child
        .stdout
        .take()
        .expect("stdout")
        .read_to_string(&mut out)
        .expect("read the reply");
    let status = child.wait().expect("wait for the child");
    assert!(std::hint::black_box(&big)[0] == 1);
    assert!(
        status.success(),
        "the child must answer, not stop: {status:?}"
    );
    assert!(
        out.contains("unreadable"),
        "a truncated PDF is unreadable: {out}"
    );
}
