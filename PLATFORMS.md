# Pebbles platform support

> Per-capability platform compatibility — so you know *before* you ship what runs
> where. Like a package's platform badges: the **core is universal**, and the
> handful of platform-specific capabilities are listed explicitly below.
> Cross-checked against the code on 2026-09-20.

Legend: **✅ supported** · **🔶 partial / degraded** (works, with a documented
limitation) · **❌ not available** · **⏳ planned**.

Platform runtime status (see [README](README.md#platform-support) for detail):
**Linux** is the primary target — built, tested and **run daily**. **macOS · Windows**
build and pass the **headless** suites in CI on `macos-latest`/`windows-latest`, but
they are **not run in a window** there, so interactive polish is unverified (visual/input
reports welcome). **Web** runs (`pebbles run -d web`, WebGPU browsers). The marks below
describe the *coded* support, runtime-verified on desktop + web.

---

## The core is universal

Everything in the widget catalog paints and responds on **all four** platforms —
there is no per-platform widget set. That includes:

- All layout & primitives, every input/display/feedback/navigation component, the
  full theme + styling system.
- The reactive runtime (signals, effects, memos, resources), animations, timers.
- GPU rendering (Vello), scrolling & fling physics.
- **Pointer input** — mouse and trackpad, including trackpad gestures.
- Text shaping, rich text, bidi/RTL, and in-field IME/composition.

If a widget isn't in the table below, it works everywhere. Only the capabilities
that touch the OS in a platform-specific way have restrictions:

## Capability matrix

| Capability | API / feature | Linux | macOS | Windows | Web |
|---|---|:--:|:--:|:--:|:--:|
| Secondary / multiple windows | `window()`, `monitors()` | ✅ | ✅ | ✅ | ❌ |
| Native OS menu bar | `App::menu` (`native-menus`) | 🔶¹ | ✅ | ✅ | ❌ |
| System-wide global hotkeys | `register_global_hotkey` (`global-hotkeys`) | 🔶² | ✅ | ✅ | ❌ |
| Native folder picker | `pick_folder` (`file-dialogs`) | ✅ | ✅ | ✅ | ❌³ |
| Network images (remote URL) | `ImageView::network` (`image-view`) | ✅ | ✅ | ✅ | ✅⁴ |
| System clipboard (cross-app) | copy/cut/paste | ✅ | ✅ | ✅ | 🔶⁵ |
| Screen-reader accessibility | `Semantics` (published tree) | ✅ | ✅ | ✅ | ❌⁶ |
| Soft keyboard / IME | text fields | ✅ | ✅ | ✅ | 🔶⁷ |
| System font discovery | (bundled fonts + icons always work) | ✅ | ✅ | ✅ | ❌⁸ |
| `tokio` async | `create_resource_future` (`tokio`) | ✅ | ✅ | ✅ | ❌⁹ |
| Background work | `spawn`, `create_resource` | ✅ | ✅ | ✅ | 🔶⁹ |
| Offscreen capture (widget→PNG/RGBA) | `capture_png`/`capture_rgba` | ✅ | ✅ | ✅ | ❌¹⁰ |
| Router / history | `pebbles_core::router`, `route_view`/`link` | 🔶¹¹ | 🔶¹¹ | 🔶¹¹ | ✅¹¹ |
| Reduced-motion query | `prefers_reduced_motion()` | 🔶¹² | ❌¹² | ❌¹² | ✅¹² |

**Notes**

1. **Linux native menu** → Pebbles uses the cross-platform in-window `menubar`
   component instead of an OS menu bar (`native-menus` is a macOS/Windows feature).
   The in-window `menubar` works on every platform.
2. **Wayland** returns a graceful error for global hotkeys (X11 works); the app
   keeps running.
3. **Web folder picker** — on **web** `pick_folder` resolves as *cancelled* (a
   `<input type=file>` backend is a follow-up).
4. **Web network images** — asset, in-memory, base64, and `data:` URIs work fully, and
   a remote `http(s)://` URL now loads via the **browser's own `fetch`** (read as an
   ArrayBuffer, decoded by the same pure-Rust `image` codecs). **CORS caveat:** a
   cross-origin URL must send the usual CORS response headers or the browser blocks the
   fetch — the view's `error` widget then renders. Same-origin URLs and `data:` URIs are
   unaffected.
5. **Web clipboard** — copy-**out** is wired: text copied in Pebbles is pushed to the OS
   clipboard via the async `navigator.clipboard.write_text`, so it pastes into other apps.
   Paste-**in** from another app is still **app-local** — a *synchronous* read of the OS
   clipboard isn't possible on the web (`readText` is async + permission-gated), so
   cross-app paste *into* Pebbles is a follow-up. Intra-app copy/paste works fully.
6. **Web accessibility** — the semantics tree is still built in memory, just
   not published: there is no AccessKit adapter for the web yet (upstream). The
   AccessKit bridge is a **no-op stub**, so screen-reader features that ride on
   a published tree — focus announcements, the modal-barrier "announced-behind-modal"
   behavior, live regions — work on **desktop only**.
7. **Soft keyboard / IME** is full on desktop; on **web** it is wired but not
   yet runtime-hardened — CJK/dead-key composition needs `App::web_ime(true)`
   (see the out-of-scope note below).
8. **Bundled assets always work.** The built-in font families (Inter, JetBrains Mono,
   Space Grotesk, Lora, plus any you register with `App::font`) **and** the bundled
   **Tabler** icon set (~5100 outline `tabler::*` + ~1050 filled `tabler::filled::*`,
   which replaced the previous Lucide set) are compiled in and render on every platform.
   Only *system-font discovery* is desktop-only. **Web text:** the web renders the bundled
   families (plus `App::font`) only; the CSS generics `"monospace"`, `"serif"` and
   `"sans-serif"` (and `system-ui`, `ui-monospace`, …) resolve to JetBrains Mono, Lora and
   Inter on every platform, and scripts the bundled fonts don't cover (Hebrew, Arabic, CJK,
   emoji) need a covering font registered with `App::font`. The text locale comes from
   `navigator.language`. **Icon binary-size footgun:**
   naming a glyph by its `const` (`tabler::CAMERA`) lets the linker drop every icon you
   don't reference; calling `tabler::by_name` or iterating `tabler::ALL` links the whole
   ~6,184-glyph table into the binary (the lookup can reach any of them), so use `by_name`
   only when the name is genuinely dynamic.
9. **Web async** has no OS threads: `spawn`/`create_resource` run the work inline
   and still deliver the result through the per-frame pump (the reactive contract
   holds). `tokio` (`create_resource_future`) is native-only; on web use `spawn`.
10. **Offscreen capture** renders headlessly through an **offscreen GPU device**, so it is **desktop-only**: the web has no headless device.
11. **Router / history** is one API on every platform, but only the **web** target syncs
    it to the browser: deep-linkable URLs, working **Back/Forward** buttons, and
    `popstate` fed back in (the history `index` rides in `history.state`). On **desktop**
    it is pure **in-memory** history — no URL bar, no deep links, and
    **no built-in Back affordance** (there is no browser chrome, so wire your own Back
    button over `router::back()` / `can_back()`). Navigation **guards** are a
    navigation-flow tool, **not** a security boundary: on web Back/Forward (and a typed
    deep link) the browser moves history *before* the router sees it, so the guard runs as
    a **post-move redirect** (it can redirect but cannot hard-**block**). Treat auth as a
    redirect to `/login` and authorize *data* server-side.
12. **Reduced-motion** is sampled **once at startup** (`prefers_reduced_motion()`), so
    widgets can default entry/transition animations off. **Web** reads the real
    `(prefers-reduced-motion: reduce)` media query; **Linux** reads GNOME's
    `gsettings … enable-animations` (other desktops report `false`); **macOS/Windows**
    currently hardcode `false` (native detection is a follow-up).
    The `PEBBLES_REDUCED_MOTION` env var forces it on any platform (tests, kiosks,
    headless captures).

## Out of scope for 0.1.0

Not implemented yet — call sites that expect these will find nothing wired:

- **Fullscreen** — there is no `winit::Fullscreen` path. `App` exposes
  `maximized`/`decorations`, but not a true fullscreen mode.
- **File drag-and-drop** — winit's `DroppedFile`/`HoveredFile` window events are not
  handled, so dropping a file onto the window does nothing.
- **Color emoji** — no emoji font is bundled (the built-ins are Inter, JetBrains Mono,
  Space Grotesk, Lora). On desktop the platform *may* fall back to a system emoji font,
  but that is **unverified**; there is no web fallback.
- **CJK on web needs `App::web_ime(true)`** — winit can't drive IME on a `<canvas>`
  (winit#4424), so CJK/dead-key composition on the web
  requires opting into the hidden-input bridge.
- **Font hinting** — glyphs render **unhinted** for 0.1.0; hinting is unconfigured, so
  there is no grid-fitting at small sizes (subpixel positions ride as-is). Fine on
  HiDPI; slightly softer than a hinted renderer at low DPI. Configurable hinting is a
  follow-up.
- **Form / validation framework** — there is **no** built-in form-state or validation
  layer, and this is a **deliberate** 0.1.0 non-feature (parked, see [`WIDGETS.md`](WIDGETS.md)
  §10), not a gap. Drive validation with plain signals: a `Signal` per field plus a derived
  error `Signal`/memo, and gate submit on it. The input widgets already expose `.error(..)`
  slots to render the result.

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

- `current() -> Platform` — the enum `Linux · MacOS · Windows · Web`.
- Family checks: `is_desktop()`, `is_web()`.
- Exact checks: `is_linux()`, `is_macos()`, `is_windows()`.
- `Platform::name()` — a lowercase name for logs.

This is the "check first, don't trip it" path. Even if you *don't* check, Pebbles
still won't crash — it degrades as described next.

## What happens if you use an unavailable capability

Pebbles **degrades gracefully — it does not crash.** Calling a capability on a
platform that lacks it does the sensible thing and logs a one-line warning:

- `window()` on web → ignored (single surface), warning logged.
- `pick_folder` on web → resolves as cancelled.
- `register_global_hotkey` on Wayland → returns an error you can handle.
- `router::back()` on desktop with no history → returns `false` (nothing to undo).

So an app written for desktop still *runs* on web; the platform-specific bits are
simply inert. Where a capability is behind a Cargo feature (`native-menus`,
`global-hotkeys`, `image-view`, `file-dialogs`, `tokio`), features are **additive but a
few are target-specific** — enable each on the targets it supports, not blindly on all.
In particular **`tokio` is native-only** (enabling it breaks the wasm build), and
**`file-dialogs` is desktop-only** (`rfd` pulls a desktop dialog backend that doesn't
apply on the web). `image-view` is cross-target (native uses a blocking HTTP client,
web the browser `fetch`). Match the feature to the target rather than assuming any
feature compiles everywhere.

## Per-widget notes

The affected widgets and APIs carry a **`# Platform support`** section in their
rustdoc, so the restriction is visible at the call site too:
`window()`, `App::menu`, `register_global_hotkey`, `pick_folder`,
`ImageView::network`. Run `cargo doc --open` to browse them.

## Troubleshooting the web build

**Blank white page.** Pebbles' default renderer (Vello Hybrid) is **WebGPU-only** — no WebGL2 fallback is compiled into it — so the browser must provide a WebGPU adapter. A blank canvas almost always means it could not, so enable it:

- **Chrome/Edge:** `chrome://flags` → **Unsafe WebGPU Support** = Enabled; on
  **Linux** also enable **Vulkan** — then relaunch. Verify at `chrome://gpu`
  (look for “WebGPU: Hardware accelerated”). Quick one-off test:
  `chromium --enable-unsafe-webgpu --enable-features=Vulkan http://localhost:8080`.
- **Firefox:** `about:config` → `dom.webgpu.enabled = true` (Nightly / recent).
- **Safari:** 26+ ships WebGPU on by default.

If WebGPU is on and it is still blank, open DevTools → Console: Pebbles installs a
panic hook, so a Rust panic prints there with a readable message.

**Slow first load.** The dev (debug) wasm is large; `pebbles run -d web --release`
produces a much smaller, faster bundle.

See also: [`WIDGETS.md`](WIDGETS.md) (the full catalog) and the platform runtime
matrix in [`README.md`](README.md#platform-support).
</content>
</invoke>
