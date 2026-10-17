# Distributing a Pebbles app

> How to ship a built app to each of the four v0.1.0 runtime targets: **Linux ·
> macOS · Windows · Web**. See [PLATFORMS.md](PLATFORMS.md) for per-capability
> support.
>
> **`pebbles build` compiles the release artifact; it is not yet a bundler.** It runs
> the optimized build for a target — `pebbles build` (desktop → `target/release/<name>`)
> or `pebbles build -d web` (a static wasm bundle → `dist/`) — but the per-OS *packaging*
> below (`.desktop` file, `.app` bundle, icon embedding, code-signing) is still the
> **manual** 0.1.0 path. A one-command bundler is planned.

A scaffolded app (`pebbles new`) is a normal Cargo binary. Build the optimized release
with the CLI (which wraps `cargo`/Trunk and prints the artifact path + size):

```bash
pebbles build                    # desktop → target/release/<name>
```

or drop to Cargo directly for the same result:

```bash
cargo build --release            # → target/release/<name>
```

The crate's default render backend is baked into its `Cargo.toml` (`default = ["vello-hybrid"]`). To ship the first-party renderer instead (experimental until it becomes the default):

```bash
cargo build --release --no-default-features --features wgpu
```

---

## Linux

`cargo build --release` produces a **runnable bare binary** at
`target/release/<name>` — copy it anywhere and run it; there is nothing else to
install for a local run.

To make it a first-class desktop app (launcher entry + icon), install the binary
and add a `.desktop` file:

```bash
install -Dm755 target/release/myapp  ~/.local/bin/myapp
install -Dm644 assets/icon.png       ~/.local/share/icons/hicolor/512x512/apps/myapp.png
```

```ini
# ~/.local/share/applications/myapp.desktop
[Desktop Entry]
Type=Application
Name=My App
Exec=myapp
Icon=myapp
Categories=Utility;
Terminal=false
```

Then refresh the caches (`update-desktop-database ~/.local/share/applications`,
`gtk-update-icon-cache`). System-wide install uses the same layout under
`/usr/local/bin` and `/usr/share`.

**Future:** single-file **AppImage** and **Flatpak** packaging are post-0.1.0.

---

## Windows

`cargo build --release` produces `target\release\<name>.exe`.

**Suppress the console window.** A plain binary opens a console window that
flashes behind the GUI. Add the crate-level attribute at the top of `main.rs`
(the `pebbles new` template ships it commented out):

```rust
#![windows_subsystem = "windows"]
```

This only takes effect in release builds; keep debug runs on the console so you
still see logs during development.

**Embed the taskbar / titlebar icon.** An `.exe` has no icon unless one is
compiled into its resources. Add a build dependency and a `build.rs`:

```toml
# Cargo.toml
[target.'cfg(windows)'.build-dependencies]
winres = "0.1"
```

```rust
// build.rs
fn main() {
    #[cfg(windows)]
    winres::WindowsResource::new()
        .set_icon("assets/icon.ico")   // a multi-size .ico (16–256 px)
        .compile()
        .unwrap();
}
```

Convert a PNG to a multi-resolution `.ico` with ImageMagick
(`magick icon.png -define icon:auto-resize=16,32,48,64,128,256 icon.ico`). Ship
the `.exe` on its own, or wrap it in an installer (WiX / Inno Setup / NSIS) —
that's out of scope for 0.1.0.

---

## macOS

`cargo build --release` produces a bare Mach-O binary that **runs from a
terminal**, but with no `.app` bundle it has no dock name, no dock icon, and no
Finder identity. For a real macOS app, assemble a `.app` bundle by hand:

```
MyApp.app/
└── Contents/
    ├── Info.plist
    ├── MacOS/
    │   └── MyApp            # the release binary, renamed
    └── Resources/
        └── AppIcon.icns
```

```xml
<!-- Contents/Info.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>CFBundleName</key>            <string>MyApp</string>
  <key>CFBundleIdentifier</key>      <string>com.example.myapp</string>
  <key>CFBundleVersion</key>         <string>0.1.0</string>
  <key>CFBundleShortVersionString</key> <string>0.1.0</string>
  <key>CFBundleExecutable</key>      <string>MyApp</string>
  <key>CFBundleIconFile</key>        <string>AppIcon</string>
  <key>CFBundlePackageType</key>     <string>APPL</string>
  <key>NSHighResolutionCapable</key> <true/>
</dict></plist>
```

Build the `.icns` from a set of PNGs with `iconutil -c icns AppIcon.iconset`
(after populating an `AppIcon.iconset/` with the required sizes).

**Distribution (Gatekeeper).** An unsigned `.app` downloaded from the internet is
blocked on other Macs. To ship it, sign with a Developer ID certificate and
notarize:

```bash
codesign --deep --force --options runtime \
  --sign "Developer ID Application: Your Name (TEAMID)" MyApp.app
xcrun notarytool submit MyApp.zip --keychain-profile "AC_PASSWORD" --wait
xcrun stapler staple MyApp.app
```

Local builds run fine unsigned; signing + notarization only matter for
distributing to other machines. Universal (arm64 + x86_64) binaries need a
separate `lipo` step — single-arch is fine for 0.1.0.

---

## Web

Pebbles builds to WebAssembly and runs in **WebGPU** browsers (Chrome/Edge,
Safari 26+, Firefox with WebGPU on). There is **no WebGL2 fallback** — a browser
without WebGPU shows a "needs WebGPU" message, not your app.

The one-command path is `pebbles build -d web` — it checks the wasm target and Trunk,
then emits a static bundle to `dist/` and prints its size:

```bash
pebbles build -d web                          # → dist/ (deploy anywhere static)
```

Under the hood it drives [Trunk](https://trunkrs.dev) — the same toolchain
`pebbles run -d web` uses. To run the equivalent by hand (e.g. to override the
`--public-url` for a sub-path deploy):

```bash
rustup target add wasm32-unknown-unknown      # once
cargo install --locked trunk                   # once
trunk build ./index.html --release --public-url / --dist ./dist
```

That mirrors the working `pebbles-docs` GitHub Pages deploy (its
`.github/workflows/deploy-pages.yml`). `--release` runs `wasm-opt`; `--public-url /` is for a site served from the
domain root — change it to `/myapp/` for a sub-path deploy (e.g. a project Pages
site). The output in `dist/` (the `.wasm`, the JS glue, `index.html`) is plain
static files — deploy them to any static host.

> **`--release` takes an optional value in Trunk 0.21**, so put the positional
> `index.html` *first* and flags after, exactly as above — otherwise Trunk parses
> the path as the flag's value and errors.

**`index.html`.** `pebbles run -d web` generates a managed page under
`.pebbles/web/` (gitignored, regenerated each run), so it's not suitable to
deploy. For a reproducible build, **commit your own `index.html`** at the crate
root — Trunk uses a project's own `index.html` untouched. A minimal one:

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no" />
  <title>My App</title>
  <link data-trunk rel="rust" href="Cargo.toml" data-bin="myapp" />
</head>
<body></body>
</html>
```

(Copy the canvas CSS and the WebGPU-missing message from the managed page in
`.pebbles/web/index.html` if you want them.)

### Host requirements

- **`.wasm` MIME type** must be `application/wasm`. Most hosts do this already;
  it's required for streaming compile. Serve `dist/` verbatim.
- **`.nojekyll` on GitHub Pages.** Jekyll strips files/dirs that start with `_`,
  which drops the `*_bg.wasm` bundle. Add an empty `.nojekyll` at the site root
  (the GitHub Actions `peaceiris/actions-gh-pages` step adds it automatically).
- **Cache headers.** The hashed `*_bg.wasm` / JS assets are content-addressed —
  serve them `Cache-Control: public, max-age=31536000, immutable`, but keep
  `index.html` short-lived (`no-cache`) so a redeploy is picked up.
- **COOP/COEP are NOT needed.** WebGPU does not require cross-origin isolation, so
  you do **not** need `Cross-Origin-Opener-Policy` / `Cross-Origin-Embedder-Policy`
  headers (those are only for `SharedArrayBuffer` / wasm threads).

### SPA fallback (required for deep links)

Pebbles' web router is **path-based** — routes are real URL paths
(`/settings`, `/docs/button`) mirrored into `window.history`. A static host only
has a file for `/` (`index.html`); a **deep link or hard reload to `/settings`
404s** unless the host serves `index.html` for unknown paths.

- **GitHub Pages:** commit a `404.html` that is a **byte-for-byte copy of
  `index.html`**. Pages serves `404.html` for any path it can't resolve, so the
  app boots and the router reads the URL. Add it to the build output:

  ```bash
  cp dist/index.html dist/404.html
  ```

- **Other hosts:** use the host's SPA rewrite rule instead — e.g. Netlify
  `/* /index.html 200`, Vercel a catch-all rewrite to `/index.html`, or nginx
  `try_files $uri /index.html;`.

Without this, only the root URL works; every sub-route breaks on refresh.
</content>
</invoke>
