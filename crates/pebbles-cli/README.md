# pebbles-cli

`pebbles` — the developer CLI for the Pebbles GUI framework.

Flutter-style tooling for a Rust desktop UI:

```text
pebbles create <name>   scaffold a new Pebbles project (app or widget package)
pebbles run             build + run in dev mode (rich logs, hot-restart on save)
pebbles build           build an optimized release artifact
pebbles bundle          package for Linux desktops (AppDir/AppImage + .desktop)
pebbles doctor          check your environment
```

## Linux packaging: `pebbles bundle`

`pebbles bundle [-p <package>] [--format appimage|appdir]` builds a release binary and
lays out `target/bundle/linux/<bin>/<bin>.AppDir/` with `AppRun`, `usr/bin/<bin>`, a
freedesktop entry (`usr/share/applications/<id>.desktop`) and hicolor icons
(`usr/share/icons/hicolor/<size>/apps/<id>.png|svg`). If `appimagetool` is on `PATH` it
also packs `<Name>-<arch>.AppImage`; nothing is downloaded — without it, the command to
finish is printed. Metadata comes from `pebbles.toml`:

```toml
[app]
name = "Counter"
id = "org.example.Counter"   # reverse-DNS; also the window's Wayland app_id / X11 WM_CLASS
icon = "assets/icon.png"     # PNG or SVG (a placeholder is generated if omitted)
comment = "A tiny counter"   # optional
categories = "Utility;"      # optional
```

The launcher sets `PEBBLES_APP_ID=<id>`, which the shell applies as the window app id
so the dock matches the window to the `.desktop` file (`App::app_id(..)` sets it in code).

Runtime dependencies on the target machine (from the system, not bundled): a Vulkan
driver (Mesa or vendor) — OpenGL ES 3 / GL via Mesa is the fallback; Wayland
(`libwayland-client`) or X11 (`libX11`, `libxcb`, `libXcursor`, `libXi`, `libXrandr`);
and `libxkbcommon`.

It is deliberately std-only — no clap, no notify. Arg parsing is hand-rolled,
file watching is a light mtime poll, and process control is `std::process`.

This crate ships the `pebbles` binary. It is the CLI companion to the 11-crate
framework whose libraries layer foundation → icons → render → {core → widgets, gpu} →
shell → umbrella `pebbles`.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
