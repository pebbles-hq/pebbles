//! `render-check`: the pixel render-check harness of the wgpu migration
//! (documentations/wgpu-migration/05-testing-and-ci.md §6). Dev tool; never shipped.

use std::fmt;

pub mod args;
pub mod bench;
pub mod candidate;
pub mod case;
pub mod corpus;
pub mod golden;
pub mod metric;
pub mod png_io;
pub mod probe;
pub mod readback;
pub mod reference;
pub mod refs;
pub mod report;
pub mod rng;
pub mod run;
pub mod scene;
pub mod status;
pub mod thresholds;

pub use status::{Status, exit_code};

/// Every harness error. Hand-written `Display` + `Error` (no `thiserror`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckError {
    /// A render size of 0 or beyond the renderer's limit.
    Size { width: u32, height: u32 },
    /// No GPU adapter / device (exit 3).
    NoAdapter(String),
    /// The smoke reference captured a GPU error.
    ReferenceGpu(String),
    /// The candidate returned `Err` or captured a GPU error.
    CandidateGpu(String),
    /// Mapping / copying a texture failed.
    Readback(String),
    /// Filesystem error.
    Io(String),
    /// PNG encode/decode error.
    Png(String),
    /// The golden model does not implement this op yet (V45/V46); reported as PROBE_FAIL.
    GoldenUnsupported(&'static str),
    /// `--reference dir` has no committed self-golden for the case (MISSING_REF).
    MissingRef(String),
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size { width, height } => write!(f, "invalid render size {width}x{height}"),
            Self::NoAdapter(e) => write!(f, "no GPU adapter: {e}"),
            Self::ReferenceGpu(e) => write!(f, "smoke reference GPU error: {e}"),
            Self::CandidateGpu(e) => write!(f, "candidate GPU error: {e}"),
            Self::Readback(e) => write!(f, "readback: {e}"),
            Self::Io(e) => write!(f, "I/O: {e}"),
            Self::Png(e) => write!(f, "PNG: {e}"),
            Self::GoldenUnsupported(op) => write!(f, "golden model does not support {op}"),
            Self::MissingRef(path) => write!(f, "missing {path}"),
        }
    }
}

impl std::error::Error for CheckError {}
