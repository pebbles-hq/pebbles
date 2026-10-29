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
and `libxkbcommon` (+ `libxkbcommon-x11` on X11). Verified in minimal `ubuntu:24.04` and
`fedora:latest` containers (mesa + Xvfb only): the counter AppDir also needed `libXcursor`, `libXi` and
`libxkbcommon-x11` there, then started on lavapipe; desktop installs ship all three.

It is deliberately std-only — no clap, no notify. Arg parsing is hand-rolled,
file watching is a light mtime poll, and process control is `std::process`.

This crate ships the `pebbles` binary. It is the CLI companion to the 11-crate
framework whose libraries layer foundation → icons → render → {core → widgets, gpu} →
shell → umbrella `pebbles`.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.

## Soak runs: `pebbles soak`

Long-running memory and frame-time soaks (wgpu plan 11 §2.9 / §6.3: SOAK-TOUR, SOAK-DAILY).

```sh
# 60-minute memory soak (SOAK-TOUR: gallery tour + input storm, strict GPU validation)
cargo build --profile soak -p gallery
pebbles soak run --bin target/soak/gallery --tour

# Daily use: start the app through the soak wrapper and use it normally
pebbles soak run --bin target/soak/gallery

# The trend at any time (exit code 1 on a failure)
pebbles soak summary
```

Each run writes into `$XDG_STATE_HOME/pebbles-soak/<YYYY-MM-DD>/` (default
`~/.local/state/pebbles-soak`; `--dir` overrides): `soak.csv` (`PEBBLES_SOAK_CSV`: RSS, GPU
allocator bytes, UI and renderer census every 600 frames) and `app.log` (the app's output with
`PEBBLES_FRAME_STATS` frame-time lines). Days are UTC and the log rotates at midnight.
`--minutes N` stops the app after N minutes; `--strict` sets `PEBBLES_GPU_STRICT=1`. Build with
`--profile soak` (release + debug assertions) or the census columns stay empty.

`summary` judges every run of at least 30 minutes by the §2.9 rules — census counts: max over
the last 10 minutes ≤ max over minutes 10–20; GPU allocator bytes at the end ≤ minute 10 × 1.02;
RSS at the end ≤ minute 10 × 1.05 (a warning only: the census decides) — fails on any panic or
strict-mode GPU error, and flags day-over-day growth (peak RSS or GPU bytes up more than 10 %, or
the median p95 UI frame time up more than 50 % and 2 ms) across the days on record.

`pebbles soak systemd --bin <app>` prints a systemd `--user` service (starts the app under the
soak wrapper with the graphical session) and a daily summary timer, with the enable commands.
Nothing is installed.
