//! Device health (08 §6.2–§6.3): the lost callback and uncaptured-error routing installed on
//! every device pebbles-gpu creates.

use std::collections::HashSet;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64};
use std::sync::{Arc, Mutex};

use pebbles_foundation::log::{Cat, Level, logf};

use super::init::GpuInit;

/// Per-device health, written by wgpu callbacks (any thread), read by the frame loop (08 §6.2).
#[derive(Debug, Default)]
pub struct DeviceHealth {
    lost: AtomicBool,
    /// 0 = not lost, 1 = Unknown (real loss), 2 = Destroyed.
    lost_reason: AtomicU8,
    fatal_errors: AtomicU64,
    validation_errors: AtomicU64,
    /// Hashes of validation messages already logged (dedup).
    seen: Mutex<HashSet<u64>>,
    /// The driver's message from the lost callback; the frame loop logs it.
    lost_message: Mutex<Option<String>>,
}

impl DeviceHealth {
    /// Lost, or hit an out-of-memory / internal error: the device must be rebuilt.
    pub fn is_poisoned(&self) -> bool {
        self.lost.load(Relaxed) || self.fatal_errors.load(Relaxed) > 0
    }

    /// The lost callback fired.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Relaxed)
    }

    /// The driver's message from the lost callback.
    pub fn lost_message(&self) -> Option<String> {
        self.lost_message.lock().ok().and_then(|m| m.clone())
    }

    /// Why the device was lost, if it was.
    pub fn lost_reason(&self) -> Option<wgpu::DeviceLostReason> {
        match self.lost_reason.load(Relaxed) {
            1 => Some(wgpu::DeviceLostReason::Unknown),
            2 => Some(wgpu::DeviceLostReason::Destroyed),
            _ => None,
        }
    }

    /// Out-of-memory and internal errors seen.
    pub fn fatal_errors(&self) -> u64 {
        self.fatal_errors.load(Relaxed)
    }

    /// Validation errors seen.
    pub fn validation_errors(&self) -> u64 {
        self.validation_errors.load(Relaxed)
    }

    pub(crate) fn mark_lost(&self, reason: wgpu::DeviceLostReason) {
        let code = match reason {
            wgpu::DeviceLostReason::Unknown => 1,
            wgpu::DeviceLostReason::Destroyed => 2,
        };
        self.lost_reason.store(code, Relaxed);
        self.lost.store(true, Relaxed);
    }
}

/// Install the lost callback and the uncaptured-error handler on a device pebbles-gpu created.
pub(crate) fn install_device_hooks(device: &wgpu::Device, health: &Arc<DeviceHealth>, init: &GpuInit) {
    let h = Arc::clone(health);
    let wake = init.on_device_lost.clone();
    device.set_device_lost_callback(move |reason, message| {
        // ANY thread (inside poll/submit, or a driver thread). Atomics + the stored message ONLY.
        h.mark_lost(reason);
        if let Ok(mut m) = h.lost_message.lock() {
            *m = Some(message);
        }
        if let Some(wake) = &wake {
            wake();
        }
    });
    let h = Arc::clone(health);
    let (strict, trace) = (init.strict_validation, init.gpu_trace);
    device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
        route_uncaptured(&h, strict, trace, e);
    }));
}

/// Classify one uncaptured error (08 §6.3). Returns whether it was logged.
#[allow(clippy::panic)] // strict mode (PEBBLES_GPU_STRICT) panics by contract (11 §4.6)
pub(crate) fn route_uncaptured(h: &DeviceHealth, strict: bool, trace: bool, e: wgpu::Error) -> bool {
    let logged = match &e {
        wgpu::Error::OutOfMemory { .. } | wgpu::Error::Internal { .. } => {
            let n = h.fatal_errors.fetch_add(1, Relaxed) + 1;
            logf(
                Level::Error,
                Cat::Gpu,
                format_args!("fatal wgpu error #{n} (device poisoned — scheduling reset): {e}"),
            );
            true
        }
        wgpu::Error::Validation { description, source } => {
            let n = h.validation_errors.fetch_add(1, Relaxed) + 1;
            let key = {
                use std::hash::{Hash, Hasher};
                let mut s = std::hash::DefaultHasher::new();
                description.hash(&mut s);
                s.finish()
            };
            // Remember at most 64 distinct descriptions (11 §4.6); after that only the counter moves.
            let first = h.seen.lock().map(|mut set| set.len() < 64 && set.insert(key)).unwrap_or(true);
            if first {
                // `description` is often just "Validation Error"; the detail is in the source chain.
                let mut detail = String::new();
                let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(source.as_ref());
                while let Some(c) = cause {
                    detail.push_str(": ");
                    detail.push_str(&c.to_string());
                    cause = c.source();
                }
                logf(
                    Level::Error,
                    Cat::Gpu,
                    format_args!("wgpu validation error #{n} (first occurrence): {description}{detail}"),
                );
            }
            first
        }
    };
    if trace {
        logf(Level::Error, Cat::Gpu, format_args!("{}", std::backtrace::Backtrace::force_capture()));
    }
    if strict {
        panic!("wgpu error (PEBBLES_GPU_STRICT): {e}");
    }
    logged
}
