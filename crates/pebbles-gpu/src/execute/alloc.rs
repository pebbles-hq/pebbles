//! Out-of-memory handling for content-size-driven GPU allocations (11 §2.6, S09): an
//! `OutOfMemory` error scope around the creation, one evict-and-retry, then the caller's
//! degradation. Native only — on wasm an OOM reaches the uncaptured handler and the device-loss
//! path (C18/C20), and a scope must never be pushed there (an un-awaited WebGPU scope swallows
//! the error). Pipelines, samplers and fixed-size uniforms are never wrapped.

/// The outcome of one checked creation.
pub(crate) enum Checked<T> {
    Ok(T),
    OutOfMemory,
}

/// Run `create` inside an `OutOfMemory` scope (native); the scope is popped before returning,
/// so no guard outlives this call.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn checked<T>(device: &wgpu::Device, create: impl FnOnce() -> T) -> Checked<T> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let value = create();
    match poll_ready_once(scope.pop()) {
        Some(Some(_oom)) => Checked::OutOfMemory,
        _ => Checked::Ok(value),
    }
}

/// wasm: no scope (11 §2.6 rule 1).
#[cfg(target_family = "wasm")]
pub(crate) fn checked<T>(_device: &wgpu::Device, create: impl FnOnce() -> T) -> Checked<T> {
    Checked::Ok(create())
}

/// wgpu-core resolves `ErrorScopeGuard::pop()` immediately; poll exactly once (no executor).
#[cfg(not(target_family = "wasm"))]
fn poll_ready_once<F: Future>(fut: F) -> Option<F::Output> {
    let mut fut = std::pin::pin!(fut);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match fut.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => Some(v),
        std::task::Poll::Pending => None,
    }
}

/// One attempt, then `evict()`, then one more attempt; `None` after a second OOM.
pub(crate) fn alloc_with_retry<T>(
    mut attempt: impl FnMut() -> Checked<T>,
    mut evict: impl FnMut(),
) -> Option<T> {
    if let Checked::Ok(v) = attempt() {
        return Some(v);
    }
    evict();
    match attempt() {
        Checked::Ok(v) => Some(v),
        Checked::OutOfMemory => None,
    }
}

/// `alloc_with_retry` of a checked `create` on `device`.
pub(crate) fn create_with_retry<T>(
    device: &wgpu::Device,
    mut create: impl FnMut() -> T,
    evict: impl FnMut(),
) -> Option<T> {
    alloc_with_retry(|| checked(device, &mut create), evict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn scripted(results: &[bool]) -> impl FnMut() -> Checked<u32> + '_ {
        let i = Cell::new(0);
        move || {
            let ok = results.get(i.get()).copied().unwrap_or(true);
            i.set(i.get() + 1);
            if ok { Checked::Ok(7) } else { Checked::OutOfMemory }
        }
    }

    #[test]
    fn retry_after_evict_succeeds() {
        let evicted = Cell::new(0);
        let r = alloc_with_retry(scripted(&[false, true]), || evicted.set(evicted.get() + 1));
        assert_eq!((r, evicted.get()), (Some(7), 1), "evict called exactly once");
    }

    #[test]
    fn second_oom_gives_none() {
        let evicted = Cell::new(0);
        let r = alloc_with_retry(scripted(&[false, false]), || evicted.set(evicted.get() + 1));
        assert_eq!((r, evicted.get()), (None, 1));
    }

    #[test]
    fn no_evict_when_first_attempt_succeeds() {
        let evicted = Cell::new(0);
        let r = alloc_with_retry(scripted(&[true]), || evicted.set(evicted.get() + 1));
        assert_eq!((r, evicted.get()), (Some(7), 0));
    }
}
