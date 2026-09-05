//! Compile-time platform detection — branch behavior *before* calling a
//! platform-specific API (secondary windows, native menus, network images, …), so
//! you never trip a capability that isn't available on the current target.
//!
//! The platform is fixed at **compile time** by the build target, so every check
//! is a `const` — zero runtime cost, and the dead branch is optimized out.
//!
//! ```
//! use pebbles_foundation::platform;
//!
//! // Only open a secondary window where it's supported.
//! if platform::is_desktop() {
//!     // open_settings_window();
//! }
//!
//! // Or match the exact platform.
//! use pebbles_foundation::platform::Platform;
//! let hint = match platform::current() {
//!     Platform::Web => "press Ctrl+S",
//!     Platform::MacOS => "press ⌘S",
//!     _ => "save",
//! };
//! # let _ = hint;
//! ```
//!
//! See `PLATFORMS.md` for which capabilities each platform supports.

/// The platform an app is running on. Determined at compile time from the build
/// target (`target_family = "wasm"` → [`Web`](Platform::Web), then `target_os`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Platform {
    Linux,
    MacOS,
    Windows,
    /// A WebGPU browser (the `wasm32` build).
    Web,
    Android,
    Ios,
}

impl Platform {
    /// A desktop OS (Linux, macOS, or Windows).
    #[must_use]
    pub const fn is_desktop(self) -> bool {
        matches!(self, Platform::Linux | Platform::MacOS | Platform::Windows)
    }

    /// A mobile OS (Android or iOS).
    #[must_use]
    pub const fn is_mobile(self) -> bool {
        matches!(self, Platform::Android | Platform::Ios)
    }

    /// The web (wasm) platform.
    #[must_use]
    pub const fn is_web(self) -> bool {
        matches!(self, Platform::Web)
    }

    /// A lowercase name (`"linux"`, `"macos"`, `"windows"`, `"web"`, `"android"`,
    /// `"ios"`) — handy for logs and diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::MacOS => "macos",
            Platform::Windows => "windows",
            Platform::Web => "web",
            Platform::Android => "android",
            Platform::Ios => "ios",
        }
    }
}

/// The platform this build targets (compile-time constant). Unknown Unix targets
/// (e.g. BSD) map to [`Linux`](Platform::Linux), the closest desktop behavior.
#[must_use]
pub const fn current() -> Platform {
    #[cfg(target_family = "wasm")]
    {
        Platform::Web
    }
    #[cfg(all(not(target_family = "wasm"), target_os = "android"))]
    {
        Platform::Android
    }
    #[cfg(all(not(target_family = "wasm"), target_os = "ios"))]
    {
        Platform::Ios
    }
    #[cfg(all(not(target_family = "wasm"), target_os = "macos"))]
    {
        Platform::MacOS
    }
    #[cfg(all(not(target_family = "wasm"), target_os = "windows"))]
    {
        Platform::Windows
    }
    #[cfg(all(
        not(target_family = "wasm"),
        not(any(target_os = "android", target_os = "ios", target_os = "macos", target_os = "windows"))
    ))]
    {
        Platform::Linux
    }
}

/// Whether this build targets the web (wasm).
#[must_use]
pub const fn is_web() -> bool {
    current().is_web()
}

/// Whether this build targets a desktop OS (Linux/macOS/Windows).
#[must_use]
pub const fn is_desktop() -> bool {
    current().is_desktop()
}

/// Whether this build targets a mobile OS (Android/iOS).
#[must_use]
pub const fn is_mobile() -> bool {
    current().is_mobile()
}

/// Whether this build targets Android.
#[must_use]
pub const fn is_android() -> bool {
    matches!(current(), Platform::Android)
}

/// Whether this build targets iOS.
#[must_use]
pub const fn is_ios() -> bool {
    matches!(current(), Platform::Ios)
}

/// Whether this build targets Linux.
#[must_use]
pub const fn is_linux() -> bool {
    matches!(current(), Platform::Linux)
}

/// Whether this build targets macOS.
#[must_use]
pub const fn is_macos() -> bool {
    matches!(current(), Platform::MacOS)
}

/// Whether this build targets Windows.
#[must_use]
pub const fn is_windows() -> bool {
    matches!(current(), Platform::Windows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_is_internally_consistent() {
        let p = current();
        // Exactly one family is true.
        assert_eq!(p.is_web() as u8 + p.is_desktop() as u8 + p.is_mobile() as u8, 1);
        // The free helpers agree with the enum.
        assert_eq!(is_web(), p.is_web());
        assert_eq!(is_desktop(), p.is_desktop());
        assert_eq!(is_mobile(), p.is_mobile());
        assert!(!p.name().is_empty());
    }

    #[test]
    fn desktop_host_reports_desktop() {
        // These tests run on the host (a desktop CI runner), so the target must be
        // a desktop platform and never web/mobile.
        assert!(is_desktop());
        assert!(!is_web());
        assert!(!is_mobile());
    }
}
