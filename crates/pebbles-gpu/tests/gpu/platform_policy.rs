//! P18: the per-OS backend policy (08 §2.4, §3.1, §3.3) picks the expected backend.

use super::*;

fn device(init: &GpuInit) -> Option<Result<wgpu::AdapterInfo, pebbles_gpu::GpuError>> {
    let mut ctx = pollster::block_on(GpuContext::new(init));
    Some(
        pollster::block_on(ctx.request_device(None))
            .map(|id| ctx.device(id).expect("device").caps().info.clone()),
    )
}

#[test]
#[ignore = "gpu"]
fn default_policy_picks_expected_backend() {
    let init = GpuInit::headless();
    let expected = pebbles_gpu::backend_rank(init.os)
        .iter()
        .copied()
        .find(|b| init.backends.contains(wgpu::Backends::from(*b)));
    let Some(Ok(info)) = device(&init) else {
        println!("skipped: no GPU adapter");
        return;
    };
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!(Some(info.backend), expected, "{info:?} with {:?}", init.backends);
}

#[test]
#[ignore = "gpu"]
#[cfg(not(target_os = "macos"))]
fn forced_software_picks_cpu_adapter() {
    if std::env::var("PEBBLES_CPU_RENDER").is_err() {
        println!("skipped: PEBBLES_CPU_RENDER not set");
        return;
    }
    let Some(Ok(info)) = device(&GpuInit::headless()) else {
        println!("skipped: no software adapter");
        return;
    };
    assert_eq!(info.device_type, wgpu::DeviceType::Cpu, "{info:?}");
}

#[test]
#[ignore = "gpu"]
#[cfg(target_os = "macos")]
fn macos_has_no_software_adapter() {
    // The noop validation lane enables wgpu's noop adapter, which reports itself as `Cpu`.
    if support::noop_backend() {
        println!("skipped: noop backend");
        return;
    }
    let mut init = GpuInit::headless();
    init.force_software = true;
    let r = device(&init).expect("result");
    assert!(matches!(r, Err(pebbles_gpu::GpuError::NoAdapter { software_only: true, .. })), "{r:?}");
}
