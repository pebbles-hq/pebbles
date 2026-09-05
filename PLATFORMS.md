# Pebbles platform support

> Per-capability platform compatibility — so you know *before* you ship what runs
> where. Like a package's platform badges: the **core is universal**, and the
> handful of platform-specific capabilities are listed explicitly below.
> Cross-checked against the code on 2026-09-05.

Legend: **✅ supported** · **🔶 partial / degraded** (works, with a documented
limitation) · **❌ not available** · **⏳ planned**.

Platform runtime status (see [README](README.md#platform-support) for detail):
**Linux · macOS · Windows** run fully today; **Web** runs (`pebbles run -d web`,
WebGPU browsers); **Android · iOS** compile and are gated in CI but are not yet
one-command runnable — their marks below describe the *coded* support, verified at
runtime only on desktop + web so far.

---

## The core is universal

Everything in the widget catalog paints and responds on **all six** platforms —
there is no per-platform widget set. That includes:

- All layout & primitives, every input/display/feedback/navigation component, the
  full theme + styling system.
- The reactive runtime (signals, effects, memos, resources), animations, timers.
- GPU rendering (Vello), scrolling & fling physics, gestures.
- **Pointer *and* touch input** — the same widgets work under mouse and finger.
- Text shaping, rich text, bidi/RTL, and in-field IME/composition.

If a widget isn't in the table below, it works everywhere. Only the capabilities
that touch the OS in a platform-specific way have restrictions:

## Capability matrix

| Capability | API / feature | Linux | macOS | Windows | Web | Android | iOS |
|---|---|:--:|:--:|:--:|:--:|:--:|:--:|
| Secondary / multiple windows | `window()`, `monitors()` | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ |
| Native OS menu bar | `App::menu` (`native-menus`) | 🔶¹ | ✅ | ✅ | ❌ | ❌ | ❌ |
| System-wide global hotkeys | `register_global_hotkey` (`global-hotkeys`) | 🔶² | ✅ | ✅ | ❌ | ❌ | ❌ |
| Native folder picker | `pick_folder` (`file-dialogs`) | ✅ | ✅ | ✅ | ❌ | 🔶³ | 🔶³ |
| Network images (remote URL) | `ImageView::network` (`image-view`) | ✅ | ✅ | ✅ | 🔶⁴ | ✅ | ✅ |
| System clipboard (cross-app) | copy/cut/paste | ✅ | ✅ | ✅ | 🔶⁵ | 🔶⁵ | ✅ |
| Screen-reader accessibility | `Semantics` (published tree) | ✅ | ✅ | ✅ | ❌⁶ | ✅ | ❌⁶ |
| Soft keyboard / IME | text fields | ✅ | ✅ | ✅ | 🔶⁷ | 🔶⁷ | 🔶⁷ |
| System font discovery | (bundled fonts always work) | ✅ | ✅ | ✅ | ❌⁸ | 🔶⁸ | 🔶⁸ |
| `tokio` async | `create_resource_future` (`tokio`) | ✅ | ✅ | ✅ | ❌⁹ | ✅ | ✅ |
| Background work | `spawn`, `create_resource` | ✅ | ✅ | ✅ | 🔶⁹ | ✅ | ✅ |

**Notes**

1. **Linux native menu** → Pebbles uses the cross-platform in-window `menubar`
   component instead of an OS menu bar (`native-menus` is a macOS/Windows feature).
   The in-window `menubar` works on every platform.
2. **Wayland** returns a graceful error for global hotkeys (X11 works); the app
   keeps running.
3. **Mobile folder picker** — coded via `rfd`, but not yet runtime-verified on a
   device. On **web** `pick_folder` resolves as *cancelled* (a `<input type=file>`
   backend is a follow-up).
4. **Web network images** — asset, in-memory, base64, and `data:` URIs work fully.
   A remote `http(s)://` URL shows the view's `error` widget until the browser
   `fetch` backend lands (a native blocking HTTP client can't run in a browser).
5. **Web / Android clipboard** falls back to an *in-app* clipboard (copy/paste
   works within the app) until the platform backend (`navigator.clipboard` / JNI)
   is wired. iOS uses the system pasteboard.
6. **Web / iOS accessibility** — the semantics tree is still built in memory, just
   not published: there is no AccessKit adapter for those platforms yet (upstream).
7. **Soft keyboard / IME** is full on desktop; on mobile/web it is wired but not
   yet runtime-hardened.
8. **Fonts** — the bundled families (Inter, JetBrains Mono, Space Grotesk, Lora,
   plus any you register with `App::font`) are guaranteed on every platform. Only
   *system-font discovery* is desktop-only for now.
9. **Web async** has no OS threads: `spawn`/`create_resource` run the work inline
   and still deliver the result through the per-frame pump (the reactive contract
   holds). `tokio` (`create_resource_future`) is native-only; on web use `spawn`.

## Detecting the platform in code

Branch on the target before calling a platform-specific capability — the checks
are compile-time constants (zero cost, dead branch removed):

```rust
use pebbles::prelude::*; // brings `platform` + `Platform` into scope

if platform::is_desktop() {
    open_settings_in_a_new_window();   // window() is desktop-only
}

// Platform-specific copy, the exact match form:
let save_hint = match platform::current() {
    Platform::MacOS => "⌘S to save",
    Platform::Web   => "Ctrl+S to save",
    _               => "save",
};
```

Available in `pebbles_foundation::platform` (re-exported in the prelude):

- `current() -> Platform` — the enum `Linux · MacOS · Windows · Web · Android · Ios`.
- Family checks: `is_desktop()`, `is_mobile()`, `is_web()`.
- Exact checks: `is_linux()`, `is_macos()`, `is_windows()`, `is_android()`, `is_ios()`.
- `Platform::name()` — a lowercase name for logs.

This is the "check first, don't trip it" path. Even if you *don't* check, Pebbles
still won't crash — it degrades as described next.

## What happens if you use an unavailable capability

Pebbles **degrades gracefully — it does not crash.** Calling a capability on a
platform that lacks it does the sensible thing and logs a one-line warning:

- `window()` on web/mobile → ignored (single surface), warning logged.
- `ImageView::network` on web → the `error` widget renders.
- `pick_folder` on web → resolves as cancelled.
- `register_global_hotkey` on Wayland → returns an error you can handle.

So an app written for desktop still *runs* on web; the platform-specific bits are
simply inert. Where a capability is behind a Cargo feature (`native-menus`,
`global-hotkeys`, `image-view`, `file-dialogs`, `tokio`), enabling that feature is
always safe on every target — the crate manifests target-gate the native-only
dependencies, so a wasm/mobile build never pulls a dependency it can't compile.

## Per-widget notes

The affected widgets and APIs carry a **`# Platform support`** section in their
rustdoc, so the restriction is visible at the call site too:
`window()`, `App::menu`, `register_global_hotkey`, `pick_folder`,
`ImageView::network`. Run `cargo doc --open` to browse them.

See also: [`WIDGETS.md`](WIDGETS.md) (the full catalog) and the platform runtime
matrix in [`README.md`](README.md#platform-support).
