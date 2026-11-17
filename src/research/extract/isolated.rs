//! Text extraction in a child process (QA H1, H2: a hostile upload must cost one upload,
//! never the server).
//!
//! `pdf-extract` recurses without a bound (a Form XObject that draws itself overflows the
//! stack, which `catch_unwind` cannot catch and which aborts the whole process) and `lopdf`
//! inflates a stream without a limit (1 MB of deflated zeros became 1.5 GB of memory). So the
//! server never parses an upload itself: it starts its own binary again with a hidden
//! subcommand, `__extract-text <pdf|pptx>`, feeds the file on stdin and reads JSON back on
//! stdout. Whatever goes wrong there stays there:
//!
//! - a crash (stack overflow, abort, panic outside the guards) is a non-zero exit;
//! - a wall-clock budget kills a child that runs too long;
//! - the output is read up to a cap and the child is killed when it writes more;
//! - the child's heap is capped: the server binary's allocator (`CountingAlloc`) counts the
//!   bytes it hands out and, in the child, refuses an allocation past the limit, which aborts
//!   the child. That is exact and independent of how the OS counts memory: resident memory
//!   (RSS) is not, because macOS compresses idle pages, and under memory pressure a 600 MiB
//!   flate bomb of spaces peaked at 1.28 GB of memory with an RSS under 512 MiB;
//! - the child also watches its own peak RSS (memory outside the heap) and exits past the same
//!   limit, and on Linux gets `RLIMIT_AS`. Best effort: checked every few milliseconds.
//!
//! Every such failure is `DocError::Unreadable`, which the API answers as 422
//! `file_unreadable`. The child gets an empty environment (no secrets, no `.env`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use super::doc::{self, DocError, Format};

/// The hidden subcommand of the server binary.
pub const SUBCOMMAND: &str = "__extract-text";

/// What a child may use before it is stopped.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Wall-clock time for the whole extraction.
    pub timeout: Duration,
    /// Bytes of JSON read back; more and the child is killed.
    pub max_output: usize,
    /// Peak resident memory of the child, in MiB.
    pub max_rss_mb: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(25),
            max_output: 32 * 1024 * 1024,
            max_rss_mb: 512,
        }
    }
}

/// What the child writes: the text of each page, or why it could not.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reply {
    Pages(Vec<String>),
    Encrypted,
    TooManyPages,
    /// `DocError::Unreadable`, by its message.
    Unreadable(String),
}

const FAILED: DocError = DocError::Unreadable("The file could not be read");
const LIMIT: DocError = DocError::Unreadable(
    "The file could not be read safely (it crashed the reader or needed too much time or memory)",
);

/// The binary to start: `INVESTING_EXTRACT_EXE` when set, else this very executable. Under
/// `cargo test` the current executable is a test harness in `target/*/deps/`; the server
/// binary sits one directory up.
fn extractor_exe() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("INVESTING_EXTRACT_EXE").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    let me = std::env::current_exe().ok()?;
    let dir = me.parent()?;
    if dir.file_name().is_some_and(|n| n == "deps") {
        let sibling = dir
            .parent()?
            .join(format!("investing-app{}", std::env::consts::EXE_SUFFIX));
        return sibling.is_file().then_some(sibling);
    }
    Some(me)
}

/// Text of each page or slide of `bytes`, read by a child process under `limits`.
pub async fn pages(
    format: Format,
    bytes: Vec<u8>,
    limits: &Limits,
) -> Result<Vec<String>, DocError> {
    let Some(exe) = extractor_exe() else {
        return Err(DocError::Unavailable(
            "The text extractor (the server binary) cannot be found",
        ));
    };
    let mut child = Command::new(exe)
        .arg(SUBCOMMAND)
        .arg(format.ext())
        .arg("--max-rss-mb")
        .arg(limits.max_rss_mb.to_string())
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| DocError::Unavailable("The text extractor could not be started"))?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(FAILED);
    };
    // The child may die before it has read everything (a broken pipe): not an error here,
    // its exit status says what happened.
    let feed = tokio::spawn(async move {
        let _ = stdin.write_all(&bytes).await;
        let _ = stdin.shutdown().await;
    });
    let cap = limits.max_output;
    let run = async {
        let mut out = Vec::new();
        let mut capped = stdout.take(cap as u64 + 1);
        capped.read_to_end(&mut out).await.map_err(|_| FAILED)?;
        if out.len() > cap {
            return Err(LIMIT);
        }
        let status = child.wait().await.map_err(|_| FAILED)?;
        Ok((out, status))
    };
    let result = tokio::time::timeout(limits.timeout, run).await;
    feed.abort();
    let (out, status) = match result {
        Ok(Ok(done)) => done,
        Ok(Err(e)) => {
            let _ = child.kill().await;
            tracing::warn!("extraction child stopped: output over the cap or unreadable");
            return Err(e);
        }
        Err(_) => {
            let _ = child.kill().await;
            tracing::warn!("extraction child stopped: over {:?}", limits.timeout);
            return Err(LIMIT);
        }
    };
    if !status.success() {
        // A crash, an abort or the memory watchdog.
        tracing::warn!("extraction child failed: {status}");
        return Err(LIMIT);
    }
    match serde_json::from_slice::<Reply>(&out).map_err(|_| FAILED)? {
        Reply::Pages(p) => Ok(p),
        Reply::Encrypted => Err(DocError::Encrypted),
        Reply::TooManyPages => Err(DocError::TooManyPages),
        Reply::Unreadable(m) => Err(DocError::Unreadable(static_message(&m))),
    }
}

/// The messages `doc` uses, as `&'static str`; anything else is the generic one.
fn static_message(m: &str) -> &'static str {
    doc::UNREADABLE_MESSAGES
        .iter()
        .find(|known| **known == m)
        .copied()
        .unwrap_or("The file could not be read")
}

/// Entry point of `investing-app __extract-text <pdf|pptx> [--max-rss-mb N]`: reads the file on
/// stdin, writes one JSON line on stdout. Returns the process exit code.
pub fn child_main(args: &[String]) -> i32 {
    use std::io::{Read, Write};
    let format = match args.first().map(String::as_str) {
        Some("pdf") => Format::Pdf,
        Some("pptx") => Format::Pptx,
        _ => return 2,
    };
    let max_rss_mb = match (args.get(1).map(String::as_str), args.get(2)) {
        (Some("--max-rss-mb"), Some(n)) => n.parse::<u64>().unwrap_or(512),
        _ => 512,
    };
    guard(max_rss_mb);
    cap_heap(max_rss_mb.saturating_mul(1024 * 1024));
    let mut bytes = Vec::new();
    if std::io::stdin().read_to_end(&mut bytes).is_err() {
        return 2;
    }
    let reply = match doc::pages(format, &bytes) {
        Ok(p) => Reply::Pages(p),
        Err(DocError::Encrypted) => Reply::Encrypted,
        Err(DocError::TooManyPages) => Reply::TooManyPages,
        Err(DocError::Unreadable(m)) => Reply::Unreadable(m.to_string()),
        Err(DocError::Unavailable(m)) => Reply::Unreadable(m.to_string()),
    };
    drop(bytes);
    let Ok(json) = serde_json::to_vec(&reply) else {
        return 1;
    };
    let mut out = std::io::stdout().lock();
    if out.write_all(&json).and_then(|()| out.flush()).is_err() {
        return 1;
    }
    0
}

/// Limits of the child itself: no core dump of a crash, an address-space limit where the OS
/// honours one, and a watchdog on peak resident memory everywhere.
#[allow(unsafe_code)]
fn guard(max_rss_mb: u64) {
    let limit_bytes = max_rss_mb.saturating_mul(1024 * 1024);
    // SAFETY: plain libc calls with valid, initialised arguments; no pointers escape.
    unsafe {
        let none = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::setrlimit(libc::RLIMIT_CORE, &none);
        #[cfg(target_os = "linux")]
        {
            // Address space is larger than resident memory (thread stacks, mappings).
            let as_limit = limit_bytes.saturating_mul(4).max(2 << 30);
            let lim = libc::rlimit {
                rlim_cur: as_limit as libc::rlim_t,
                rlim_max: as_limit as libc::rlim_t,
            };
            libc::setrlimit(libc::RLIMIT_AS, &lim);
        }
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(10));
        // SAFETY: `getrusage` fills the zeroed struct it is given.
        let peak = unsafe {
            let mut ru: libc::rusage = std::mem::zeroed();
            if libc::getrusage(libc::RUSAGE_SELF, &mut ru) != 0 {
                continue;
            }
            // Bytes on macOS, kilobytes elsewhere.
            if cfg!(target_os = "macos") {
                ru.ru_maxrss as u64
            } else {
                (ru.ru_maxrss as u64).saturating_mul(1024)
            }
        };
        if peak > limit_bytes {
            // SAFETY: ends the process at once, as the memory is already gone.
            unsafe { libc::_exit(4) };
        }
    });
}

/// Heap bytes this process holds, and the most it may hold (no cap outside the child).
static HEAP_HELD: AtomicUsize = AtomicUsize::new(0);
static HEAP_CAP: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Caps the heap of this process at `bytes`; past it an allocation fails, and Rust aborts the
/// process on a failed allocation (a non-zero exit, answered as 422). The child only.
fn cap_heap(bytes: u64) {
    HEAP_CAP.store(
        usize::try_from(bytes).unwrap_or(usize::MAX),
        Ordering::Relaxed,
    );
}

/// The server binary's global allocator: the system one, counting what it hands out so the
/// extraction child's heap can be capped exactly (`cap_heap`). Without a cap it only counts.
pub struct CountingAlloc;

impl CountingAlloc {
    /// Books `n` more bytes, or refuses when that would pass the cap.
    fn take(n: usize) -> bool {
        let held = HEAP_HELD.fetch_add(n, Ordering::Relaxed);
        if held.saturating_add(n) > HEAP_CAP.load(Ordering::Relaxed) {
            HEAP_HELD.fetch_sub(n, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn give(n: usize) {
        HEAP_HELD.fetch_sub(n, Ordering::Relaxed);
    }
}

// SAFETY: every call goes to `System` with the caller's arguments unchanged; the counter only
// decides whether an allocation is attempted at all, and a refused one returns null, which
// the `GlobalAlloc` contract allows.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !Self::take(layout.size()) {
            return std::ptr::null_mut();
        }
        let p = System.alloc(layout);
        if p.is_null() {
            Self::give(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !Self::take(layout.size()) {
            return std::ptr::null_mut();
        }
        let p = System.alloc_zeroed(layout);
        if p.is_null() {
            Self::give(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        Self::give(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = layout.size();
        if new_size > old {
            if !Self::take(new_size - old) {
                return std::ptr::null_mut();
            }
            let p = System.realloc(ptr, layout, new_size);
            if p.is_null() {
                Self::give(new_size - old);
            }
            p
        } else {
            let p = System.realloc(ptr, layout, new_size);
            if !p.is_null() {
                Self::give(old - new_size);
            }
            p
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counter is the process-wide one, so this is the only test that touches it.
    #[test]
    fn the_counting_allocator_books_every_byte_and_refuses_past_the_cap() {
        let a = CountingAlloc;
        let small = Layout::from_size_align(1024, 8).unwrap();
        let held = HEAP_HELD.load(Ordering::Relaxed);
        cap_heap((held + 4096) as u64);
        // SAFETY: each pointer is freed once, with the layout it was allocated with.
        unsafe {
            let p = a.alloc(small);
            assert!(!p.is_null());
            assert_eq!(HEAP_HELD.load(Ordering::Relaxed), held + 1024);
            let p = a.realloc(p, small, 3072);
            assert!(!p.is_null());
            assert_eq!(HEAP_HELD.load(Ordering::Relaxed), held + 3072);
            // 3072 + 2048 is past the cap: refused, nothing booked.
            let big = Layout::from_size_align(2048, 8).unwrap();
            assert!(a.alloc(big).is_null());
            assert!(a.alloc_zeroed(big).is_null());
            assert!(a
                .realloc(p, Layout::from_size_align(3072, 8).unwrap(), 8192)
                .is_null());
            assert_eq!(HEAP_HELD.load(Ordering::Relaxed), held + 3072);
            let p = a.realloc(p, Layout::from_size_align(3072, 8).unwrap(), 512);
            assert_eq!(HEAP_HELD.load(Ordering::Relaxed), held + 512);
            a.dealloc(p, Layout::from_size_align(512, 8).unwrap());
        }
        assert_eq!(HEAP_HELD.load(Ordering::Relaxed), held);
        cap_heap(u64::MAX);
    }
}
