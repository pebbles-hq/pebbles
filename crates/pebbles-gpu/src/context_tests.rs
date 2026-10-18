//! Unit tests for `context` and the device hooks (08 §6.10).

use super::*;
use crate::health::route_uncaptured;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn msaa_is_four_or_one() {
    assert_eq!(pick_msaa(true), 4);
    assert_eq!(pick_msaa(false), 1);
}

#[test]
#[ignore = "gpu"]
fn limits_use_adapter_resolution() {
    let init = GpuInit::headless();
    let mut ctx = pollster::block_on(GpuContext::new(&init));
    let dev_id = match pollster::block_on(ctx.request_device(None)) {
        Ok(id) => id,
        Err(_) => {
            if std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1") {
                panic!("PEBBLES_REQUIRE_GPU=1 but no GPU adapter");
            }
            return;
        }
    };
    let dev = ctx.device(dev_id).expect("device");
    assert_eq!(
        dev.device().limits().max_texture_dimension_2d,
        dev.adapter().limits().max_texture_dimension_2d
    );
    assert_eq!(dev.device().limits().max_storage_buffers_per_shader_stage, 0);
}

/// Build a headless context + device, or `None` without an adapter (panics under
/// `PEBBLES_REQUIRE_GPU=1`).
pub(crate) fn gpu_or_skip(init: &GpuInit) -> Option<(GpuContext, DeviceId)> {
    let mut ctx = pollster::block_on(GpuContext::new(init));
    match pollster::block_on(ctx.request_device(None)) {
        Ok(id) => Some((ctx, id)),
        Err(_) => {
            assert!(
                !std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1"),
                "PEBBLES_REQUIRE_GPU=1 but no GPU adapter"
            );
            None
        }
    }
}

fn validation(description: &str) -> wgpu::Error {
    wgpu::Error::Validation { source: Box::new(std::fmt::Error), description: description.to_owned() }
}

#[test]
fn health_mark_lost() {
    let h = DeviceHealth::default();
    assert!(!h.is_poisoned());
    h.mark_lost(wgpu::DeviceLostReason::Unknown);
    assert!(h.is_poisoned() && h.is_lost());
    assert_eq!(h.lost_reason(), Some(wgpu::DeviceLostReason::Unknown));
}

#[test]
fn routing_classifies_errors() {
    let h = DeviceHealth::default();
    assert!(route_uncaptured(&h, false, false, validation("bad view")));
    assert!(!route_uncaptured(&h, false, false, validation("bad view")), "same description logs once");
    assert_eq!(h.validation_errors(), 2);
    assert!(!h.is_poisoned(), "validation errors never poison");
    route_uncaptured(&h, false, false, wgpu::Error::OutOfMemory { source: Box::new(std::fmt::Error) });
    assert!(h.is_poisoned());
    assert_eq!(h.fatal_errors(), 1);
}

#[test]
#[ignore = "gpu"]
fn device_destroy_fires_lost_callback() {
    let woke = Arc::new(AtomicU64::new(0));
    let mut init = GpuInit::headless();
    let w = Arc::clone(&woke);
    init.on_device_lost = Some(Arc::new(move || {
        w.fetch_add(1, Relaxed);
    }));
    let Some((ctx, id)) = gpu_or_skip(&init) else { return };
    let dev = ctx.device(id).expect("device");
    dev.device().destroy();
    let _ = dev.device().poll(wgpu::PollType::Poll);
    assert_eq!(dev.health().lost_reason(), Some(wgpu::DeviceLostReason::Destroyed));
    assert!(ctx.any_device_poisoned());
    assert_eq!(ctx.lost_devices().len(), 1);
    assert_eq!(woke.load(Relaxed), 1);
}

#[test]
#[ignore = "gpu"]
fn strict_mode_panics_on_validation_error() {
    let mut init = GpuInit::headless();
    init.strict_validation = true;
    let Some((ctx, id)) = gpu_or_skip(&init) else { return };
    let device = ctx.device(id).expect("device").device().clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // A zero-size texture is a validation error.
        let _texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.test.context"),
            size: wgpu::Extent3d { width: 0, height: 0, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
    }));
    let payload = result.expect_err("strict mode must panic");
    let msg = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default();
    assert!(msg.contains("wgpu error (PEBBLES_GPU_STRICT)"), "{msg}");
}
