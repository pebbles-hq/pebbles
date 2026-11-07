//! Memory diagnostics: who holds the process's memory.
//!
//! * [`CountingAlloc`] — an opt-in global allocator wrapper (`#[global_allocator] static A:
//!   CountingAlloc = CountingAlloc;` in the app) that counts live Rust heap bytes: two
//!   relaxed atomics per allocation.
//! * [`snapshot`] — Rust heap (when counted), glibc's whole heap (Rust AND C libraries such
//!   as the GPU driver), free-but-retained heap, and the process RSS split by mapping kind
//!   (heap, anonymous, the executable, GPU driver libraries, other files).
//! * [`trim`] / [`start_trimmer`] — give glibc's retained free memory back to the OS.
//! * `PEBBLES_MEM_TRACE=1` + [`probe`] — log a snapshot with a label (startup milestones).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

/// Counts live Rust heap bytes; forwards to the system allocator.
pub struct CountingAlloc;

static LIVE: AtomicIsize = AtomicIsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static COUNTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size() as isize, Ordering::Relaxed);
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        COUNTING.store(true, Ordering::Relaxed);
        // SAFETY: forwarded verbatim to the system allocator.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as isize, Ordering::Relaxed);
        // SAFETY: as above.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        LIVE.fetch_add(n as isize - l.size() as isize, Ordering::Relaxed);
        // SAFETY: as above.
        unsafe { System.realloc(p, l, n) }
    }
}

/// One memory snapshot, in bytes (`None` = not available on this platform/setup).
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Live Rust heap ([`CountingAlloc`] installed), and the allocation count so far.
    pub rust_heap: Option<usize>,
    pub rust_allocs: usize,
    /// glibc: bytes in use by every malloc user, and free bytes it retains.
    pub malloc_in_use: Option<usize>,
    pub malloc_free_retained: Option<usize>,
    /// Resident set by mapping: `[heap]`, anonymous (other arenas, thread stacks, GPU
    /// staging), the executable, GPU/driver libraries (LLVM, Mesa, Vulkan ICDs), other
    /// files (shared libraries, fonts), and the total.
    pub rss_heap: usize,
    pub rss_anon: usize,
    pub rss_exe: usize,
    pub rss_driver: usize,
    pub rss_files: usize,
    pub rss_total: usize,
}

impl Snapshot {
    /// One line, MB.
    pub fn line(&self) -> String {
        let mb = |b: usize| b as f64 / 1_048_576.0;
        let opt = |o: Option<usize>| o.map_or("n/a".to_string(), |b| format!("{:.1}", mb(b)));
        format!(
            "rss {:.1} MB = heap {:.1} + anon {:.1} + exe {:.1} + driver {:.1} + files {:.1} | rust heap {} MB | malloc in-use {} MB, free-retained {} MB",
            mb(self.rss_total),
            mb(self.rss_heap),
            mb(self.rss_anon),
            mb(self.rss_exe),
            mb(self.rss_driver),
            mb(self.rss_files),
            opt(self.rust_heap),
            opt(self.malloc_in_use),
            opt(self.malloc_free_retained),
        )
    }
}

#[cfg(target_os = "linux")]
#[repr(C)]
struct MallInfo2 {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
unsafe extern "C" {
    fn mallinfo2() -> MallInfo2;
    fn malloc_trim(pad: usize) -> i32;
}


/// Take a [`Snapshot`] now.
pub fn snapshot() -> Snapshot {
    let mut s = Snapshot {
        rust_heap: COUNTING.load(Ordering::Relaxed).then(|| LIVE.load(Ordering::Relaxed).max(0) as usize),
        rust_allocs: ALLOCS.load(Ordering::Relaxed),
        ..Default::default()
    };
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: reads glibc's allocator statistics.
        let m = unsafe { mallinfo2() };
        s.malloc_in_use = Some(m.uordblks + m.hblkhd);
        s.malloc_free_retained = Some(m.fordblks);
    }
    #[cfg(target_os = "linux")]
    if let Ok(text) = std::fs::read_to_string("/proc/self/smaps") {
        let exe = std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned());
        let mut kind = 0u8;
        for l in text.lines() {
            let first = l.split_whitespace().next().unwrap_or("");
            if first.contains('-') && first.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                let path = l.split_whitespace().nth(5).unwrap_or("");
                kind = if path == "[heap]" {
                    0
                } else if path.is_empty() || path.starts_with('[') {
                    1
                } else if exe.as_deref() == Some(path) {
                    2
                } else if ["libLLVM", "libvulkan_", "libgallium", "dri/", "mesa", "libnvidia", "libdrm", "iris", "/memfd:"]
                    .iter()
                    .any(|k| path.contains(k))
                {
                    3
                } else {
                    4
                };
            } else if let Some(v) = l.strip_prefix("Rss:") {
                let kb: usize = v.trim().trim_end_matches(" kB").trim().parse().unwrap_or(0);
                let b = kb * 1024;
                s.rss_total += b;
                match kind {
                    0 => s.rss_heap += b,
                    1 => s.rss_anon += b,
                    2 => s.rss_exe += b,
                    3 => s.rss_driver += b,
                    _ => s.rss_files += b,
                }
            }
        }
    }
    s
}

/// Return glibc's free-but-retained heap to the OS (a no-op elsewhere).
pub fn trim() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: glibc's own, thread-safe.
    unsafe {
        malloc_trim(0);
    }
}

/// A background thread trimming 2 s after start (the GPU driver's startup shader compiles
/// leave ~tens of MB free but retained), again at 4, 8 and 16 s (an app's own start-up
/// work — folder walks, indexes, git — finishes after the first frame and frees its
/// scratch), then each minute; each time only when > 4 MB sits free.
pub fn start_trimmer() {
    let _ = std::thread::Builder::new().name("pebbles-trim".into()).spawn(|| {
        let mut wait = 2;
        std::thread::sleep(std::time::Duration::from_secs(wait));
        loop {
            if snapshot().malloc_free_retained.is_some_and(|f| f > 4 << 20) {
                trim();
            }
            probe("after trim");
            wait = if wait < 16 { wait * 2 } else { 60 };
            std::thread::sleep(std::time::Duration::from_secs(wait));
        }
    });
}

/// `PEBBLES_MEM_TRACE=1`: log a snapshot labelled `label` to stderr.
pub fn probe(label: &str) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var("PEBBLES_MEM_TRACE").is_ok_and(|v| v == "1")) {
        eprintln!("[pebbles mem] {label:<20} {}", snapshot().line());
    }
}
