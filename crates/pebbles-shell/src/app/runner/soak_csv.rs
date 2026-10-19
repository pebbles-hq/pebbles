//! `PEBBLES_SOAK_CSV=<path>` (11 §2.9, S18): one CSV row every 600 rendered frames and on exit —
//! RSS, the driver's allocator report, the UI census and the renderer census — for long soaks
//! (`--profile soak` keeps the debug-only census accessors in an optimized build). Never per frame.

use std::io::Write as _;

use pebbles_gpu::GpuCensus;

/// Rendered frames between rows.
pub(super) const EVERY_FRAMES: u64 = 600;

const FIXED: [&str; 10] = [
    "unix_ms",
    "frame",
    "uptime_s",
    "rss_kib",
    "gpu_alloc_bytes",
    "gpu_reserved_bytes",
    "elements",
    "render_nodes",
    "signals",
    "subscriptions",
];

/// One sample; `None` cells are written empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Sample {
    pub unix_ms: u128,
    pub frame: u64,
    pub uptime_s: f64,
    pub rss_kib: Option<u64>,
    /// (allocated, reserved) bytes from `Device::generate_allocator_report` (Vulkan/DX12).
    pub gpu: Option<(u64, u64)>,
    /// (elements, render nodes, signals, subscriptions) — debug-only census.
    pub ui: Option<[usize; 4]>,
    /// The renderer census (wgpu + debug assertions only).
    pub census: Option<[u64; 23]>,
}

pub(super) fn header() -> String {
    let mut cols: Vec<&str> = FIXED.to_vec();
    cols.extend(GpuCensus::COLUMNS);
    cols.join(",") + "\n"
}

pub(super) fn row(s: &Sample) -> String {
    let opt = |v: Option<u64>| v.map(|v| v.to_string()).unwrap_or_default();
    let mut cells =
        vec![s.unix_ms.to_string(), s.frame.to_string(), format!("{:.1}", s.uptime_s), opt(s.rss_kib)];
    cells.push(opt(s.gpu.map(|g| g.0)));
    cells.push(opt(s.gpu.map(|g| g.1)));
    for i in 0..4 {
        cells.push(s.ui.map(|u| u[i].to_string()).unwrap_or_default());
    }
    for i in 0..GpuCensus::COLUMNS.len() {
        cells.push(s.census.map(|c| c[i].to_string()).unwrap_or_default());
    }
    cells.join(",") + "\n"
}

/// `VmRSS` of this process in KiB (Linux); `None` elsewhere.
pub(super) fn rss_kib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = s.lines().find(|l| l.starts_with("VmRSS:"))?;
        line.split_whitespace().nth(1)?.parse().ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// The open CSV file.
pub(super) struct SoakCsv {
    file: std::fs::File,
}

impl SoakCsv {
    /// Open `PEBBLES_SOAK_CSV` in append mode (header once when empty).
    pub(super) fn from_env() -> Option<Self> {
        let path = std::env::var("PEBBLES_SOAK_CSV").ok().filter(|p| !p.is_empty())?;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).ok()?;
        if file.metadata().ok()?.len() == 0 {
            file.write_all(header().as_bytes()).ok()?;
        }
        Some(Self { file })
    }

    pub(super) fn write(&mut self, s: &Sample) {
        let _ = self.file.write_all(row(s).as_bytes());
        let _ = self.file.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_has_header_column_count() {
        let cols = header().trim_end().split(',').count();
        let full = Sample {
            unix_ms: 1,
            frame: 600,
            uptime_s: 10.0,
            rss_kib: Some(1024),
            gpu: Some((2, 3)),
            ui: Some([4, 5, 6, 7]),
            census: Some([9; 23]),
        };
        assert_eq!(row(&full).trim_end().split(',').count(), cols);
        let empty = Sample::default();
        assert_eq!(row(&empty).trim_end().split(',').count(), cols, "empty cells keep the shape");
        assert!(row(&full).starts_with("1,600,10.0,1024,2,3,4,5,6,7,9,"));
    }
}
