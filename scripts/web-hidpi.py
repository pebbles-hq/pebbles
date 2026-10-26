#!/usr/bin/env python3
"""08 §15 W-HIDPI / P13 AC2 in headless Chrome: the counter web build at devicePixelRatio 2.

Serves a built `dist/` (built with debug assertions so the surface keeps its configure census),
opens it through Playwright with `device_scale_factor=2`, turns on the input storm
(`?pebbles-input-storm=1`) so the app renders well over 100 frames, and reads the `[pebbles run]` console lines
(`?pebbles-frame-stats=1`). Pass: at least 100 frames presented after the first summary line and
the surface's `configures=` count never changes while the size stays the same (no per-frame reconfigure, P13).
The surface's clamp warning is a `pebbles_foundation::log` line, which wasm does not echo to the
console, so the "one clamp warning" half of W-HIDPI is not observable here (the clamp itself is:
the backing store stays at max_texture_dimension_2d).

    web-hidpi.py --dist examples/counter/dist --out runs
"""

import argparse
import functools
import http.server
import json
import os
import re
import sys
import threading
import time

RUN_RE = re.compile(r"\[pebbles run\].* size=(\d+)x(\d+) .* presented=\+(\d+)/(\d+) .* configures=(\S+)")

CASES = [
    # (name, query, viewport width, extra Chrome args)
    ("webgl2-dpr2", "?pebbles-force-webgl=1&pebbles-frame-stats=1&pebbles-input-storm=1", 1280,
     ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"]),
    # Wider than max_texture_dimension_2d / 2 at dpr 2 (8192 on SwiftShader): the clamp path.
    ("webgl2-dpr2-huge", "?pebbles-force-webgl=1&pebbles-frame-stats=1&pebbles-input-storm=1", 4400,
     ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"]),
    ("webgpu-dpr2", "?pebbles-frame-stats=1&pebbles-input-storm=1", 1280,
     ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=swiftshader",
      "--use-webgpu-adapter=swiftshader", "--enable-unsafe-swiftshader"]),
]


def serve(dist, port):
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=dist)
    handler.log_message = lambda *a, **k: None
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", port), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def run_case(p, url, name, width, args, out):
    # A real device scale factor (not Playwright's emulation): the window is `width` × 800 CSS px.
    browser = p.chromium.launch(channel="chrome", headless=True, args=[
        "--no-sandbox", "--force-device-scale-factor=2", f"--window-size={width},800", *args])
    ctx = browser.new_context(no_viewport=True)
    page = ctx.new_page()
    t0 = time.monotonic()
    lines = []
    page.on("console", lambda m: lines.append((round(time.monotonic() - t0, 3), m.type, m.text)))
    page.on("pageerror", lambda e: lines.append((round(time.monotonic() - t0, 3), "pageerror", str(e))))
    page.goto(url)
    end = time.monotonic() + 60
    while time.monotonic() < end and not any("[pebbles run]" in t for _, _, t in lines):
        page.wait_for_timeout(250)
    dpr = page.evaluate("window.devicePixelRatio")
    canvas = page.evaluate(
        "(() => { const c = document.querySelector('canvas'); return c ? [c.width, c.height, c.clientWidth,"
        " c.clientHeight] : null })()")
    # The input storm (`?pebbles-input-storm=1`) keeps frames coming: wait for 120 presents after
    # the first summary line (a SwiftShader canvas at 8192 px is slow), at most 120 s.
    end = time.monotonic() + 120
    while time.monotonic() < end:
        runs = [m for _, _, t in lines for m in [RUN_RE.search(t)] if m]
        if len(runs) > 1 and int(runs[-1].group(4)) - int(runs[0].group(4)) >= 120:
            break
        page.wait_for_timeout(1000)
    browser.close()
    with open(os.path.join(out, f"{name}.console.log"), "w", encoding="utf-8") as f:
        for t, kind, text in lines:
            f.write(f"+{t:8.3f} {kind}: {text}\n")
    runs = [m for _, _, t in lines for m in [RUN_RE.search(t)] if m]
    gpu = [t for _, _, t in lines if "GPU:" in t]
    panics = [t for _, k, t in lines if "panicked" in t or k == "pageerror"]
    lost = [t for _, _, t in lines if "GPU device lost" in t]
    # Chrome warns when the canvas is configured with a format other than its preferred one
    # (an extra copy per frame); the surface takes the preferred format on WebGPU.
    fmt_warn = [t for _, _, t in lines if "than is preferred by this device" in t or "extra copy" in t]
    surf = [t for _, _, t in lines if "surface: format=" in t]
    confs = [m.group(5) for m in runs]
    # A reconfigure is only allowed where the canvas size changed (the storm resizes rarely).
    spurious = [(a.group(0)[-80:], b.group(0)[-80:]) for a, b in zip(runs, runs[1:])
                if a.group(5) != b.group(5) and a.group(1, 2) == b.group(1, 2)]
    presented_after_first = int(runs[-1].group(4)) - int(runs[0].group(4)) if len(runs) > 1 else 0
    checks = [
        ("devicePixelRatio == 2", dpr == 2, f"dpr={dpr}"),
        ("canvas backing store = CSS size × dpr (clamped to max_tex)", bool(canvas) and
         canvas[0] == min(round(canvas[2] * dpr), 8192) and canvas[1] == min(round(canvas[3] * dpr), 8192),
         f"canvas width,height,clientWidth,clientHeight = {canvas}"),
        ("GPU line", bool(gpu), gpu[0] if gpu else "none"),
        ("no panic / page error", not panics, "; ".join(panics[:3])),
        ("no preferred-canvas-format warning", not fmt_warn,
         f"{len(fmt_warn)} lines; first: {fmt_warn[0] if fmt_warn else '-'}; surface: {surf[:1]}"),
        ("no GPU device lost", not lost, f"{len(lost)} lines; first: {lost[0] if lost else '-'}"),
        (">= 100 frames presented after the first summary", presented_after_first >= 100,
         f"{presented_after_first} frames over {len(runs)} summary lines"),
        ("configure count unchanged at an unchanged size over those frames (P13)", len(runs) > 1 and not spurious,
         f"configures per line: {confs}; sizes: {[m.group(1) + 'x' + m.group(2) for m in runs]}"),
    ]
    return {"case": name, "url": url, "dpr": dpr, "canvas": canvas, "gpu": gpu[:1],
            "run_lines": [m.group(0) for m in runs], "checks": checks}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dist", required=True)
    ap.add_argument("--out", default="runs")
    ap.add_argument("--port", type=int, default=8123)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    from playwright.sync_api import sync_playwright

    serve(a.dist, a.port)
    results, md, failed = [], [], False
    with sync_playwright() as p:
        for name, query, width, args in CASES:
            r = run_case(p, f"http://127.0.0.1:{a.port}/{query}", name, width, args, a.out)
            results.append(r)
            ok = all(c[1] for c in r["checks"])
            # WebGPU in headless Chrome on a CI VM is best effort: record, don't gate.
            failed |= not ok and not name.startswith("webgpu")
            md += [f"### W-HIDPI {name}: {'PASS' if ok else 'FAIL'}", "", f"`{r['gpu'][0] if r['gpu'] else 'no GPU line'}`",
                   "", "| check | result | detail |", "|---|---|---|"]
            md += [f"| {c} | {'pass' if ok_ else '**FAIL**'} | {str(d).replace('|', '/')[:500]} |"
                   for c, ok_, d in r["checks"]]
            md.append("")
    with open(os.path.join(a.out, "web-hidpi.json"), "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2)
    text = "\n".join(md) + "\n"
    print(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as f:
            f.write(text)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
