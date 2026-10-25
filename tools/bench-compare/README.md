# bench-compare (M32, informative)

Matched scenes drawn by three toolkits and measured with one method, so the numbers are
comparable with each other on the same machine. They never gate anything (10 §M32).

| Path | What |
|---|---|
| `src/main.rs` | Pebbles app (workspace member `bench-compare`) |
| `iced/` | iced 0.13.1 app, standalone Cargo project (own `[workspace]`) |
| `flutter/` | Flutter app (`lib/main.dart`); `flutter create --platforms=linux .` adds the embedder |
| `run.py` | Linux driver: runs every (toolkit, scene) and prints a markdown table |

## Scenes (`M32_SCENE`), window 1200×800

- `dashboard` — 6 rounded cards (14 px radius, 24 px blurred drop shadow), two text labels each
  (one changes every frame) and a 61-point line chart path that animates every frame.
- `rects_10k` — a 100×100 grid of 8×8 solid rects in two colours, translated every frame.
- `list_10k_scroll` — 10 000 rows of two text runs, 48 px each, scrolled at a constant 1200 px/s
  through the toolkit's scroll controller. Pebbles and Flutter use their virtualized list builder;
  iced has no virtualized list, so it lays out a 10 000-row column (its normal idiom).

`effects` and `text_heavy` from the card are not built yet (iced has no blur/backdrop).

## Method (identical in all three apps)

- Every frame tick (Pebbles `create_loop_while`, Flutter `Ticker`, iced `window::frames()`)
  takes a wall-clock timestamp. Intervals between consecutive ticks from `M32_WARM` (2 s) to
  `M32_SECS` (12 s) give p50/p95 **frame interval**. With vsync this is capped at the refresh
  interval; under Xvfb there is no real display, so it tracks the frame cost.
- **Startup** = epoch time of the first frame tick − the driver's spawn time. Before measuring,
  the driver launches each app once untimed so no toolkit pays the runner's cold page cache
  (binary, Vulkan/GL driver, LLVM) inside its first measured scene (M34).
- After the window the animation stops; the driver waits 1 s, then reads **idle CPU** (process
  utime+stime over 5 s) and **RSS** / peak RSS from `/proc`.
- Flutter also prints its own `FrameTiming.totalSpan` p50/p95 (`M32-own` line).

## Run

CI: `gh workflow run bench -R pebbles-hq/pebbles --ref <branch>` (job `compare`, ubuntu-24.04,
Xvfb; Pebbles and iced on lavapipe, Flutter on llvmpipe GL). Locally on Linux:

```bash
cargo build --release -p bench-compare
cargo build --release --manifest-path tools/bench-compare/iced/Cargo.toml
(cd tools/bench-compare/flutter && flutter create --platforms=linux . && flutter build linux --release)
python3 tools/bench-compare/run.py pebbles=target/release/bench-compare \
  iced=tools/bench-compare/iced/target/release/m32-iced \
  flutter=tools/bench-compare/flutter/build/linux/x64/release/bundle/m32_flutter
```
