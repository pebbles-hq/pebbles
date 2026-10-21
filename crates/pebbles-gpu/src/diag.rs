//! The adapter and surface diagnostic lines (08 §14.1, P09): pure formatters, plus the one
//! sanctioned stderr print (`PEBBLES_GPU_INFO=1`). On wasm32 the lines go to the browser
//! console instead (the web has no env; testers read them there, 08 §15).

use crate::init::GpuPower;
use crate::surface::OutputEncoding;

/// What the Device line reports.
pub(crate) struct GpuSummary<'a> {
    pub name: &'a str,
    pub backend: wgpu::Backend,
    pub device_type: wgpu::DeviceType,
    pub driver: &'a str,
    pub driver_info: &'a str,
    pub max_texture_dim: u32,
    pub power: GpuPower,
}

/// `GPU: {name} | backend={backend} type={type:?} | driver="{driver} {info}" | max_tex={n} | power={power:?}[ | software]`
pub(crate) fn adapter_line(s: &GpuSummary<'_>) -> String {
    let soft = if s.device_type == wgpu::DeviceType::Cpu { " | software" } else { "" };
    format!(
        "GPU: {} | backend={} type={:?} | driver=\"{} {}\" | max_tex={} | power={:?}{soft}",
        s.name,
        s.backend.to_str(),
        s.device_type,
        s.driver,
        s.driver_info,
        s.max_texture_dim,
        s.power
    )
}

/// What the Surface line reports.
pub(crate) struct SurfaceSummary {
    pub format: wgpu::TextureFormat,
    pub view_format: wgpu::TextureFormat,
    pub encoding: OutputEncoding,
    pub present: wgpu::PresentMode,
    pub latency: u32,
    pub alpha: wgpu::CompositeAlphaMode,
    pub width: u32,
    pub height: u32,
}

/// `surface: format={f:?} view={v:?} encoding={e:?} present={p:?} latency={n} alpha={a:?} size={w}x{h}`
pub(crate) fn surface_line(s: &SurfaceSummary) -> String {
    format!(
        "surface: format={:?} view={:?} encoding={:?} present={:?} latency={} alpha={:?} size={}x{}",
        s.format, s.view_format, s.encoding, s.present, s.latency, s.alpha, s.width, s.height
    )
}

/// `pebbles: {line}` on stderr (native) or the browser console (wasm32).
#[allow(clippy::print_stderr)] // PEBBLES_GPU_INFO, 08 §14.1
pub(crate) fn stderr_line(line: &str) {
    #[cfg(not(target_family = "wasm"))]
    eprintln!("pebbles: {line}");
    #[cfg(target_family = "wasm")]
    web_sys::console::info_1(&format!("pebbles: {line}").into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_line_format() {
        let s = GpuSummary {
            name: "llvmpipe (LLVM 17.0.6, 256 bits)",
            backend: wgpu::Backend::Vulkan,
            device_type: wgpu::DeviceType::Cpu,
            driver: "llvmpipe",
            driver_info: "Mesa 24.0",
            max_texture_dim: 16384,
            power: GpuPower::LowPower,
        };
        assert_eq!(
            adapter_line(&s),
            "GPU: llvmpipe (LLVM 17.0.6, 256 bits) | backend=vulkan type=Cpu | driver=\"llvmpipe Mesa 24.0\" | max_tex=16384 | power=LowPower | software"
        );
    }

    #[test]
    fn surface_line_format() {
        let s = SurfaceSummary {
            format: wgpu::TextureFormat::Bgra8Unorm,
            view_format: wgpu::TextureFormat::Bgra8Unorm,
            encoding: OutputEncoding::Gamma,
            present: wgpu::PresentMode::AutoVsync,
            latency: 2,
            alpha: wgpu::CompositeAlphaMode::Opaque,
            width: 800,
            height: 600,
        };
        assert_eq!(
            surface_line(&s),
            "surface: format=Bgra8Unorm view=Bgra8Unorm encoding=Gamma present=AutoVsync latency=2 alpha=Opaque size=800x600"
        );
    }
}
