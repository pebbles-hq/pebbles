#!/usr/bin/env python3
"""M32 driver (Linux only; README.md). Runs each toolkit's app on each scene and prints a
markdown table: frame interval p50/p95, startup to first frame tick, idle CPU after the
animation stops, and resident memory.

    python3 tools/bench-compare/run.py pebbles=target/release/bench-compare \
        iced=tools/bench-compare/iced/target/release/m32-iced \
        flutter=tools/bench-compare/flutter/build/linux/x64/release/bundle/m32_flutter
"""

import os
import re
import subprocess
import sys
import threading
import time

SCENES = os.environ.get("M32_SCENES", "list_10k_scroll,dashboard,effects,text_heavy").split(",")
# Device scale factor for every app: winit (Pebbles, iced) reads WINIT_X11_SCALE_FACTOR, the
# Flutter GTK embedder reads GDK_SCALE. Each app echoes the dpr it saw in its M32 line.
SCALE = os.environ.get("M32_SCALE", "2")
SECS, WARM, IDLE = 12.0, 2.0, 5.0
TCK = os.sysconf("SC_CLK_TCK")


def scale_env():
    return {"WINIT_X11_SCALE_FACTOR": SCALE, "GDK_SCALE": SCALE, "GDK_DPI_SCALE": "1"}


def window_px(pid):
    """Physical size of the app's largest top-level X window (proves the scale factor took)."""
    try:
        out = subprocess.run(["xwininfo", "-root", "-children"], capture_output=True, text=True).stdout
    except OSError:
        return "?"
    best = (0, "?")
    for m in re.finditer(r'"(?:M32|m32)[^"]*":.*?(\d+)x(\d+)\+', out):
        w, h = int(m.group(1)), int(m.group(2))
        best = max(best, (w * h, f"{w}x{h}"))
    return best[1]


def cpu_ticks(pid):
    with open(f"/proc/{pid}/stat") as f:
        parts = f.read().rsplit(")", 1)[1].split()
    return int(parts[11]) + int(parts[12])  # utime + stime


def mem_kib(pid, key):
    with open(f"/proc/{pid}/status") as f:
        for line in f:
            if line.startswith(key + ":"):
                return int(line.split()[1])
    return 0


def run(toolkit, exe, scene):
    env = dict(os.environ, M32_SCENE=scene, M32_SECS=str(SECS), M32_WARM=str(WARM), **scale_env())
    t0 = time.time() * 1000.0
    p = subprocess.Popen([exe], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                         cwd=os.path.dirname(os.path.abspath(exe)))
    lines = []
    threading.Thread(target=lambda: lines.extend(iter(p.stdout.readline, "")), daemon=True).start()
    deadline = time.time() + SECS + 60
    while time.time() < deadline and not any(l.startswith("M32 ") for l in lines) and p.poll() is None:
        time.sleep(0.2)
    res = {"toolkit": toolkit, "scene": scene}
    if p.poll() is None:
        time.sleep(1.0)  # let the last frame settle
        res["win"] = window_px(p.pid)
        c0 = cpu_ticks(p.pid)
        time.sleep(IDLE)
        res["idle_cpu"] = 100.0 * (cpu_ticks(p.pid) - c0) / TCK / IDLE
        res["rss"] = mem_kib(p.pid, "VmRSS") / 1024.0
        res["hwm"] = mem_kib(p.pid, "VmHWM") / 1024.0
        p.kill()
    p.wait()
    time.sleep(0.5)
    for l in lines:
        if l.startswith("M32 "):
            res.update(dict(kv.split("=", 1) for kv in l.split()[1:]))
        elif l.startswith("M32-first-frame"):
            res["startup_ms"] = int(re.search(r"epoch_ms=(\d+)", l).group(1)) - t0
        elif l.startswith("M32-own"):
            res["own"] = l.strip()
    if "frames" not in res:
        res["error"] = "".join(lines[-15:])
    return res


def warm_up(exe):
    """One untimed launch per toolkit before any measurement (M34). Without it the first
    (toolkit, scene) pair pays the cold page cache for the binary, the Vulkan/GL driver and LLVM,
    so its startup measured disk I/O rather than the toolkit (dashboard, run first, read 1584 ms
    on CI while every later Pebbles launch took ~160 ms)."""
    env = dict(os.environ, M32_SCENE=SCENES[0], M32_SECS="1", M32_WARM="0.5", **scale_env())
    p = subprocess.Popen([exe], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                         cwd=os.path.dirname(os.path.abspath(exe)))
    deadline = time.time() + 60
    for line in iter(p.stdout.readline, ""):
        if line.startswith("M32 ") or time.time() > deadline:
            break
    p.kill()
    p.wait()
    time.sleep(0.5)


def main():
    apps = [(t, os.path.abspath(e)) for t, e in (a.split("=", 1) for a in sys.argv[1:])]
    for _, exe in apps:
        warm_up(exe)
    print("| scene | toolkit | dpr / window px | frames (10 s) | interval p50 ms | interval p95 ms | startup ms | idle CPU % | RSS MiB (peak) |")
    print("|---|---|---|---|---|---|---|---|---|")
    extra = []
    for scene in SCENES:
        for toolkit, exe in apps:
            r = run(toolkit, exe, scene)
            if "error" in r:
                print(f"| {scene} | {toolkit} | | error | | | | | |")
                extra.append(f"{toolkit} {scene}:\n{r['error']}")
                continue
            print(f"| {scene} | {toolkit} | {r.get('dpr', '-')} / {r.get('win', '?')} | {r['frames']} | {r['p50_ms']} | {r['p95_ms']} | "
                  f"{r.get('startup_ms', 0):.0f} | {r.get('idle_cpu', float('nan')):.1f} | "
                  f"{r.get('rss', 0):.0f} ({r.get('hwm', 0):.0f}) |", flush=True)
            if "own" in r:
                extra.append(r["own"])
    print()
    for e in extra:
        print(e)


if __name__ == "__main__":
    main()
