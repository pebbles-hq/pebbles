#!/usr/bin/env python3
"""08 P04 AC1 on B7 (Firefox, WebGL2): WebGL context loss and restore on the counter web build.

Serves a built `dist/`, opens `?pebbles-force-webgl=1` in Playwright's Firefox, clicks the `+`
button three times through the canvas (found in a screenshot: the right-most filled blob of the
lowest UI row), then calls `WEBGL_lose_context.loseContext()`, waits, `restoreContext()`.
Pass: the console logs 'WebGL context lost; waiting for restore' and then 'GPU stack rebuilt',
no panic or page error, no reload, and the canvas after the restore matches the canvas before
the loss (the count survived) while differing from the canvas before the clicks.

    web-context-loss.py --dist examples/counter/dist --out runs
"""

import argparse
import functools
import http.server
import io
import json
import os
import sys
import threading
import time


def serve(dist, port):
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=dist)
    handler.log_message = lambda *a, **k: None
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", port), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def load(png):
    from PIL import Image

    return Image.open(io.BytesIO(png)).convert("RGB")


def diff(a, b):
    from PIL import ImageChops, ImageStat

    if a.size != b.size:
        return 255.0
    return round(sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3, 3)


def find_plus(img):
    """Center (CSS px of the canvas image) of the right-most filled blob in the lowest UI band."""
    w, h = img.size
    px = img.load()
    bg = px[2, 2]

    def far(p):
        return sum(abs(p[i] - bg[i]) for i in range(3)) > 120

    rows = [y for y in range(h) if sum(1 for x in range(0, w, 2) if far(px[x, y])) > 4]
    if not rows:
        return None
    # Bands of consecutive non-background rows; the controls row is the lowest band.
    bands, start = [], rows[0]
    for a, b in zip(rows, rows[1:] + [None]):
        if b is None or b - a > 3:
            bands.append((start, a))
            start = b
    y0, y1 = bands[-1]
    ym = (y0 + y1) // 2
    xs = [x for x in range(w) if far(px[x, ym])]
    if not xs:
        return None
    # The right-most run of far pixels at the band's middle row: the filled `+` button.
    runs, s = [], xs[0]
    for a, b in zip(xs, xs[1:] + [None]):
        if b is None or b - a > 2:
            runs.append((s, a))
            s = b
    x0, x1 = runs[-1]
    # A filled button is wide; if the last run is a thin glyph stroke, widen to the band.
    return ((x0 + x1) // 2, ym), bands, runs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dist", required=True)
    ap.add_argument("--out", default="runs")
    ap.add_argument("--port", type=int, default=8124)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    from playwright.sync_api import sync_playwright

    serve(a.dist, a.port)
    url = f"http://127.0.0.1:{a.port}/?pebbles-force-webgl=1"
    lines, checks, info = [], [], {}
    with sync_playwright() as p:
        # Headed under Xvfb: headless Firefox on a GPU-less runner offers no WebGL at all.
        browser = p.firefox.launch(headless=False, firefox_user_prefs={
            "webgl.force-enabled": True, "webgl.disable-fail-if-major-performance-caveat": True})
        page = browser.new_page(viewport={"width": 900, "height": 700})
        t0 = time.monotonic()
        page.on("console", lambda m: lines.append((round(time.monotonic() - t0, 3), m.type, m.text)))
        page.on("pageerror", lambda e: lines.append((round(time.monotonic() - t0, 3), "pageerror", str(e))))
        before_clicks = before_loss = during = after = None
        try:
            page.goto(url)
            end = time.monotonic() + 60
            while time.monotonic() < end and not any("GPU:" in t for _, _, t in lines):
                page.wait_for_timeout(250)
            page.wait_for_timeout(2000)
            canvas = page.locator("canvas").first
            box = canvas.bounding_box()
            before_clicks = load(canvas.screenshot())
            before_clicks.save(os.path.join(a.out, "ctxloss-0-initial.png"))
            found = find_plus(before_clicks)
            info["plus"] = found[0] if found else None
            info["bands"] = found[1] if found else None
            info["runs"] = found[2] if found else None
            if found:
                # The screenshot is in CSS px at dpr 1 (the default context).
                for _ in range(3):
                    canvas.click(position={"x": found[0][0], "y": found[0][1]}, timeout=10000)
                    page.wait_for_timeout(300)
            page.wait_for_timeout(1000)
            before_loss = load(canvas.screenshot())
            before_loss.save(os.path.join(a.out, "ctxloss-1-after-clicks.png"))
            # A reload would drop this marker (and the count with it).
            page.evaluate("window.__pebblesNoReload = 1")
            lose = page.evaluate(
                "(() => { const c = document.querySelector('canvas'); const gl = c.getContext('webgl2');"
                " if (!gl) return 'no webgl2 context'; const e = gl.getExtension('WEBGL_lose_context');"
                " if (!e) return 'no WEBGL_lose_context'; window.__pebblesLose = e; e.loseContext(); return 'lost'; })()")
            info["lose"] = lose
            page.wait_for_timeout(3000)
            during = load(canvas.screenshot())
            during.save(os.path.join(a.out, "ctxloss-2-lost.png"))
            lost_lines = [t for _, _, t in lines if "WebGL context lost; waiting for restore" in t]
            info["restore"] = page.evaluate("(() => { window.__pebblesLose.restoreContext(); return 'restored'; })()")
            end = time.monotonic() + 30
            n_lost = len(lines)
            while time.monotonic() < end and not any("GPU stack rebuilt" in t for _, _, t in lines[n_lost:]):
                page.wait_for_timeout(250)
            page.wait_for_timeout(2000)
            after = load(canvas.screenshot())
            after.save(os.path.join(a.out, "ctxloss-3-restored.png"))
            info["marker_kept"] = page.evaluate("window.__pebblesNoReload === 1")
            info["canvas_box"] = box
            info["user_agent"] = page.evaluate("navigator.userAgent")
        except Exception as e:  # noqa: BLE001 - record and judge what we got
            info["error"] = str(e)[:600]
        browser.close()

    with open(os.path.join(a.out, "ctxloss.console.log"), "w", encoding="utf-8") as f:
        for t, kind, text in lines:
            f.write(f"+{t:8.3f} {kind}: {text}\n")
    gpu = [t for _, _, t in lines if "GPU:" in t]
    lost_lines = [t for _, _, t in lines if "WebGL context lost; waiting for restore" in t]
    rebuilt = [t for _, _, t in lines if "GPU stack rebuilt" in t]
    panics = [t for _, k, t in lines if "panicked" in t or k == "pageerror"]
    d_clicks = diff(before_clicks, before_loss) if before_clicks and before_loss else 0.0
    d_restore = diff(before_loss, after) if before_loss and after else 255.0
    checks = [
        ("script ran to the end", "error" not in info, info.get("error", "")),
        ("WebGL2 backend (B7 Firefox)", any("backend=gl" in t for t in gpu), gpu[0] if gpu else "no GPU line"),
        ("`+` clicked 3× through the canvas changed the frame", info.get("plus") is not None and d_clicks > 0.05,
         f"+ at {info['plus']}; mean abs diff initial→after clicks {d_clicks}"),
        ("loseContext() logs 'WebGL context lost; waiting for restore'", info.get("lose") == "lost" and bool(lost_lines),
         f"{info.get('lose')}; {lost_lines[:1]}"),
        ("restoreContext() logs 'GPU stack rebuilt'", bool(rebuilt), rebuilt[0] if rebuilt else "none"),
        ("no panic / page error", not panics, "; ".join(panics[:3])),
        ("no reload (a window marker set before the loss is still there)", info.get("marker_kept") is True,
         f"marker kept: {info.get('marker_kept')}"),
        ("canvas repainted after restore and the count survived (≈ frame before the loss)",
         d_restore < 1.0, f"mean abs diff before loss→after restore {d_restore}"),
    ]
    ok = all(c[1] for c in checks)
    md = [f"### P04 AC1 B7 Firefox WebGL context loss/restore: {'PASS' if ok else 'FAIL'}", "",
          f"`{info.get('user_agent')}`", "", "| check | result | detail |", "|---|---|---|"]
    md += [f"| {c} | {'pass' if k else '**FAIL**'} | {str(d).replace('|', '/')[:500]} |" for c, k, d in checks]
    text = "\n".join(md) + "\n"
    print(text)
    print(json.dumps(info, default=str))
    with open(os.path.join(a.out, "ctxloss.json"), "w", encoding="utf-8") as f:
        json.dump({"info": info, "checks": checks}, f, indent=2, default=str)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as f:
            f.write(text + "\n")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
