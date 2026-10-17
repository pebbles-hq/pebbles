//! Unit tests for `init` (08 §2.3, §3.9).

use std::collections::HashMap;

use super::*;

fn get<'a>(map: &'a HashMap<&'static str, String>) -> impl Fn(&str) -> Option<String> + 'a {
    move |k| map.get(k).cloned()
}

fn empty() -> HashMap<&'static str, String> {
    HashMap::new()
}

fn env(entries: &[(&'static str, &str)]) -> HashMap<&'static str, String> {
    entries.iter().map(|(k, v)| (*k, v.to_string())).collect()
}

fn cand(name: &str, backend: wgpu::Backend, device_type: wgpu::DeviceType) -> AdapterCandidate {
    AdapterCandidate { name: name.to_string(), backend, device_type }
}

#[test]
fn defaults_without_env() {
    let init = GpuInit::from_env_with(HostOs::Linux, get(&empty()));
    assert_eq!(init.power, GpuPower::LowPower);
    assert_eq!(init.present_mode, wgpu::PresentMode::AutoVsync);
    assert_eq!(init.max_frame_latency, 2);
    assert!(matches!(init.dx12_compiler, wgpu::Dx12Compiler::Fxc));
    assert!(!init.force_software);
    assert!(init.adapter_name.is_none());
}

#[test]
fn parses_every_pebbles_key() {
    let m = env(&[
        ("PEBBLES_CPU_RENDER", "1"),
        ("PEBBLES_PRESENT_MODE", "mailbox"),
        ("PEBBLES_FRAME_LATENCY", "7"), // out of range → 2
        ("PEBBLES_GPU_INFO", "true"),
        ("PEBBLES_GPU_STRICT", "0"),
    ]);
    let init = GpuInit::from_env_with(HostOs::Linux, get(&m));
    assert!(init.force_software);
    assert_eq!(init.present_mode, wgpu::PresentMode::Mailbox);
    assert_eq!(init.max_frame_latency, 2);
    assert!(init.print_info);
    assert!(!init.strict_validation);
}

#[test]
fn power_none_is_low_power() {
    let low = GpuInit::from_env_with(HostOs::Linux, get(&env(&[("WGPU_POWER_PREF", "none")])));
    assert_eq!(low.power, GpuPower::LowPower);
    let high = GpuInit::from_env_with(HostOs::Linux, get(&env(&[("WGPU_POWER_PREF", "high")])));
    assert_eq!(high.power, GpuPower::HighPerformance);
}

#[test]
fn backend_env_intersects_compiled() {
    let (b, ignored) =
        resolve_backends(HostOs::Linux, Some("webgpu"), wgpu::Backends::VULKAN | wgpu::Backends::GL);
    assert_eq!(
        b,
        (wgpu::Backends::VULKAN | wgpu::Backends::GL) & (wgpu::Backends::VULKAN | wgpu::Backends::GL)
    );
    assert!(ignored);
}

#[test]
fn backend_env_selects_subset() {
    let (b, ignored) =
        resolve_backends(HostOs::Linux, Some("gl"), wgpu::Backends::VULKAN | wgpu::Backends::GL);
    assert_eq!(b, wgpu::Backends::GL);
    assert!(!ignored);
}

#[test]
fn per_os_default_backends() {
    assert_eq!(default_backends(HostOs::Windows), wgpu::Backends::DX12);
    assert_eq!(default_backends(HostOs::MacOs), wgpu::Backends::METAL);
    assert_eq!(default_backends(HostOs::Linux), wgpu::Backends::VULKAN | wgpu::Backends::GL);
    assert_eq!(default_backends(HostOs::Web), wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL);
    assert_eq!(default_backends(HostOs::Other), wgpu::Backends::all());
}

#[test]
fn dx12_compiler_env() {
    let dxc = GpuInit::from_env_with(HostOs::Windows, get(&env(&[("WGPU_DX12_COMPILER", "dxc")])));
    assert!(matches!(dxc.dx12_compiler, wgpu::Dx12Compiler::DynamicDxc { .. }));
    let bogus = GpuInit::from_env_with(HostOs::Windows, get(&env(&[("WGPU_DX12_COMPILER", "bogus")])));
    assert!(matches!(bogus.dx12_compiler, wgpu::Dx12Compiler::Fxc));
}

#[test]
fn rank_windows_low_power_prefers_dx12_integrated() {
    let cands = [
        cand("v", wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu),
        cand("i", wgpu::Backend::Dx12, wgpu::DeviceType::IntegratedGpu),
        cand("d", wgpu::Backend::Dx12, wgpu::DeviceType::DiscreteGpu),
        cand("c", wgpu::Backend::Dx12, wgpu::DeviceType::Cpu),
    ];
    let init = GpuInit { os: HostOs::Windows, ..GpuInit::from_env_with(HostOs::Windows, get(&empty())) };
    let ranked = rank_adapters(&cands, &init);
    assert_eq!(ranked.order.first(), Some(&1));
    assert!(!ranked.name_filter_ignored);
}

#[test]
fn rank_windows_high_performance() {
    let cands = [
        cand("v", wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu),
        cand("i", wgpu::Backend::Dx12, wgpu::DeviceType::IntegratedGpu),
        cand("d", wgpu::Backend::Dx12, wgpu::DeviceType::DiscreteGpu),
        cand("c", wgpu::Backend::Dx12, wgpu::DeviceType::Cpu),
    ];
    let mut init = GpuInit::from_env_with(HostOs::Windows, get(&empty()));
    init.power = GpuPower::HighPerformance;
    let ranked = rank_adapters(&cands, &init);
    assert_eq!(ranked.order.first(), Some(&2));
}

#[test]
fn rank_force_software_keeps_only_cpu() {
    let cands = [
        cand("v", wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu),
        cand("i", wgpu::Backend::Dx12, wgpu::DeviceType::IntegratedGpu),
        cand("d", wgpu::Backend::Dx12, wgpu::DeviceType::DiscreteGpu),
        cand("c", wgpu::Backend::Dx12, wgpu::DeviceType::Cpu),
    ];
    let mut init = GpuInit::from_env_with(HostOs::Windows, get(&empty()));
    init.force_software = true;
    let ranked = rank_adapters(&cands, &init);
    assert_eq!(ranked.order, vec![3]);
}

#[test]
fn rank_hardware_gl_beats_lavapipe() {
    let cands = [
        cand("llvmpipe (lavapipe)", wgpu::Backend::Vulkan, wgpu::DeviceType::Cpu),
        cand("gl", wgpu::Backend::Gl, wgpu::DeviceType::IntegratedGpu),
    ];
    let init = GpuInit::from_env_with(HostOs::Linux, get(&empty()));
    let ranked = rank_adapters(&cands, &init);
    assert_eq!(ranked.order.first(), Some(&1));
}

#[test]
fn rank_linux_vulkan_before_gl() {
    let cands = [
        cand("gl", wgpu::Backend::Gl, wgpu::DeviceType::IntegratedGpu),
        cand("vk", wgpu::Backend::Vulkan, wgpu::DeviceType::IntegratedGpu),
    ];
    let init = GpuInit::from_env_with(HostOs::Linux, get(&empty()));
    let ranked = rank_adapters(&cands, &init);
    assert_eq!(ranked.order.first(), Some(&1));
}

#[test]
fn rank_name_filter_no_match_is_ignored() {
    let cands = [
        cand("amd", wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu),
        cand("intel", wgpu::Backend::Gl, wgpu::DeviceType::IntegratedGpu),
    ];
    let init = GpuInit::from_env_with(HostOs::Linux, get(&empty()));
    let no_filter = rank_adapters(&cands, &init);
    let mut filtered = GpuInit::from_env_with(HostOs::Linux, get(&empty()));
    filtered.adapter_name = Some("zzz".to_string());
    let r = rank_adapters(&cands, &filtered);
    assert_eq!(r.order, no_filter.order);
    assert!(r.name_filter_ignored);
}

#[test]
fn rank_name_filter_narrows() {
    let cands = [
        cand("amd radeon", wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu),
        cand("intel uhd", wgpu::Backend::Vulkan, wgpu::DeviceType::IntegratedGpu),
        cand("nvidia", wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu),
    ];
    let mut init = GpuInit::from_env_with(HostOs::Linux, get(&empty()));
    init.adapter_name = Some("intel".to_string());
    let ranked = rank_adapters(&cands, &init);
    assert_eq!(ranked.order, vec![1]);
}

#[test]
fn rank_macos_software_is_empty() {
    let cands = [cand("apple", wgpu::Backend::Metal, wgpu::DeviceType::IntegratedGpu)];
    let mut init = GpuInit::from_env_with(HostOs::MacOs, get(&empty()));
    init.force_software = true;
    let ranked = rank_adapters(&cands, &init);
    assert!(ranked.order.is_empty());
}
