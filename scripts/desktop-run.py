#!/usr/bin/env python3
"""Scripted desktop runs of the 08 §15 manual rows on a real window system (desktop-runs.yml).

One scenario per call: start an example with the scenario's env, drive it (OS-level resizes, a
covering window), timestamp every stderr line, sample the process RSS, take screenshots, stop
it, then judge the row's pass criteria from the log. Writes `<out>/<scenario>-<mode>.log`,
`.json` and screenshots, appends a Markdown row to $GITHUB_STEP_SUMMARY and exits non-zero on
a failed check.

    desktop-run.py --scenario c2 --mode threaded --bin target/soak/desktop --out runs

Scenarios (08 §15 / §17.8):
  c1      first frame: GPU line with the expected backend, first present, no ERROR, no panic
  c2      live resize 10 s (slow, then fast) by the OS (xdotool under openbox; a SendInput corner
          drag on Windows; a CGEvent corner drag on macOS) over a magenta backdrop: frames
          presented in every second the size changed, no black area in screenshots, frame time
          within budget after the first second;
          with --mode threaded this is M-RESIZE and also needs resize_sync_timeouts == 0
  multi   M-MULTI: three secondary windows of different sizes, each resized every 0.5 s for
          30 s: all keep presenting; no per-frame texture reallocation in [pebbles gpu]
  thread  M-THREAD: `multi` + the input storm typing into the main window: input-to-present
          latency and both windows' present rates (recorded)
  storm   input storm + secondary-window churn: no panic, presents every second, churn done
  occ     M-OCC/P12: three still secondary windows (`multi-static`), then a full-screen window
          covers everything for 12 s: occlusion reported, presents stop while covered and resume
          after uncover with no input
  c5      PEBBLES_SYNTH_GPU_LOSS=300 (+ the input storm, so frames keep coming) for --duration s:
          every reset rebuilt and presented again, no panic, the renderer census (soak CSV,
          inline only) flat across resets; screenshots every minute are kept for inspection
  p14     PEBBLES_PRESENT_MODE=mailbox: exactly one fallback warning, present=AutoVsync
  p19     PEBBLES_SYNTH_GPU_LOSS + PEBBLES_SYNTH_GPU_DEAD: rebuild attempts 1, 2, 4, 8, 10,
          10 s apart; the process stays alive
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import threading
import time

IS_WIN = sys.platform.startswith("win")
IS_MAC = sys.platform == "darwin"
IS_LINUX = sys.platform.startswith("linux")

RUN_RE = re.compile(
    r"\[pebbles run\] t=(?P<t>[\d.]+)s mode=(?P<mode>\w+) windows=(?P<windows>\d+) size=(?P<w>\d+)x(?P<h>\d+) "
    r"pos=(?P<pos>\S+) scale=(?P<scale>[\d.]+) presented=\+(?P<pm>\d+)/(?P<pmt>\d+) secondary=\+(?P<ps>\d+)/(?P<pst>\d+) "
    r"unpresented=\+(?P<un>\d+)/(?P<unt>\d+) occluded=(?P<occ>\d+)/(?P<occs>\d+) resize_sync_timeouts=(?P<rst>\d+) "
    r"gen=(?P<gen>\d+) gap_ms=(?P<gmax>[\d.]+)/(?P<gp95>[\d.]+) ui_ms=(?P<umax>[\d.]+)/(?P<up95>[\d.]+) "
    r"input_ms=(?P<inp>[\d.]+) configures=(?P<conf>\S*) not_presented=(?P<np>\S+)"
)
GPU_STATS_TEX_RE = re.compile(r"\[pebbles gpu\].* new=buf:\d+,tex:(\d+)")
EXPECTED_BACKEND = {"linux": "vulkan", "darwin": "metal", "win32": "dx12"}
# 10 §10 frame budget until M10 (08 §15 C2).
FRAME_BUDGET_MS = 32.0


def now():
    return time.monotonic()


class Run:
    """The app process, its timestamped log and the samples taken while it runs."""

    def __init__(self, cmd, env, log_path):
        self.t0 = now()
        self.lines = []  # (t, text)
        self.rss = []  # (t, kib)
        self.lock = threading.Lock()
        self.log = open(log_path, "w", encoding="utf-8", errors="replace")
        kwargs = {}
        if IS_WIN:
            kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
        self.proc = subprocess.Popen(
            cmd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            **kwargs,
        )
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        self.sampler = threading.Thread(target=self._sample, daemon=True)
        self.sampler.start()

    def _read(self):
        for raw in iter(self.proc.stdout.readline, b""):
            text = raw.decode("utf-8", "replace").rstrip("\r\n")
            t = now() - self.t0
            with self.lock:
                self.lines.append((t, text))
            self.log.write(f"+{t:9.3f} | {text}\n")
            self.log.flush()

    def _sample(self):
        try:
            import psutil
        except ImportError:
            return
        try:
            p = psutil.Process(self.proc.pid)
            while self.proc.poll() is None:
                self.rss.append((now() - self.t0, p.memory_info().rss // 1024))
                time.sleep(2.0)
        except Exception:
            pass

    def elapsed(self):
        return now() - self.t0

    def alive(self):
        return self.proc.poll() is None

    def snapshot(self):
        with self.lock:
            return list(self.lines)

    def wait_for(self, pattern, timeout):
        rx = re.compile(pattern)
        end = now() + timeout
        while now() < end and self.alive():
            for t, text in self.snapshot():
                if rx.search(text):
                    return t, text
            time.sleep(0.1)
        for t, text in self.snapshot():
            if rx.search(text):
                return t, text
        return None

    def last_run_line(self):
        for t, text in reversed(self.snapshot()):
            m = RUN_RE.search(text)
            if m:
                return t, m
        return None

    def stop(self):
        alive = self.alive()
        if alive:
            if IS_WIN:
                self.proc.kill()
            else:
                self.proc.terminate()
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(10)
        self.reader.join(5)
        self.log.close()
        return alive


# --- OS drivers -------------------------------------------------------------------------------


def window_rect(run):
    """(x, y, w, h) in physical px of the main window from the latest `[pebbles run]` line."""
    last = run.last_run_line()
    if not last:
        return None
    m = last[1]
    try:
        x, y = (int(v) for v in m["pos"].split(","))
    except ValueError:
        return None
    return x, y, int(m["w"]), int(m["h"])


def x11_window_id(run):
    for args in (["--pid", str(run.proc.pid)], ["--name", "Northwind|Counter|counter|desktop"]):
        try:
            out = subprocess.run(["xdotool", "search", "--onlyvisible", *args], capture_output=True, text=True,
                                 timeout=10).stdout.split()
        except (OSError, subprocess.TimeoutExpired):
            return None
        if out:
            return out[0]
    return None


def resize_path(seconds, base=(900, 640)):
    """(delay, w, h) steps: 5 s slow (small steps every 100 ms), then 5 s fast (big jumps every 16 ms)."""
    steps = []
    t = 0.0
    import math

    while t < seconds / 2:
        k = math.sin(t / (seconds / 2) * math.pi * 2)
        steps.append((0.1, int(base[0] + 220 * k), int(base[1] + 140 * k)))
        t += 0.1
    while t < seconds:
        k = math.sin(t * 9.0)
        steps.append((0.016, int(base[0] + 320 * k), int(base[1] + 200 * k)))
        t += 0.016
    return steps


def drive_resize_xdotool(run, seconds, notes):
    wid = x11_window_id(run)
    if not wid:
        notes.append("xdotool: window not found")
        return False
    notes.append(f"xdotool windowsize on X11 window {wid} (WM-mediated ConfigureRequest)")
    for delay, w, h in resize_path(seconds):
        subprocess.run(["xdotool", "windowsize", wid, str(w), str(h)], capture_output=True)
        time.sleep(delay)
    return True


WIN_DRAG_PS1 = r"""
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class U {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool SystemParametersInfo(uint a, uint p, IntPtr v, uint w);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
[U]::SetProcessDPIAware() | Out-Null
# Hosted runners ship with "Show window contents while dragging" off (an outline drag resizes only
# on release); a user's desktop has it on. SPI_SETDRAGFULLWINDOWS = 0x0025.
[U]::SystemParametersInfo(0x0025, 1, [IntPtr]::Zero, 0) | Out-Null
$p = Get-Process -Id __PID__
$h = $p.MainWindowHandle
for ($i = 0; $i -lt 50 -and $h -eq 0; $i++) { Start-Sleep -Milliseconds 200; $p.Refresh(); $h = $p.MainWindowHandle }
if ($h -eq 0) { Write-Output "no window"; exit 2 }
[U]::SetForegroundWindow($h) | Out-Null
# Fit the window on screen (the runner's display is small) so its sizing corner is reachable.
[U]::SetWindowPos($h, [IntPtr]::Zero, 40, 40, 760, 560, 0x0004) | Out-Null
Start-Sleep -Milliseconds 500
$r = New-Object U+RECT
[U]::GetWindowRect($h, [ref]$r) | Out-Null
Write-Output "rect $($r.L),$($r.T),$($r.R),$($r.B)"
# Grab the bottom-right sizing corner (a few px inside the outer frame) and drag it.
$x0 = $r.R - 4; $y0 = $r.B - 4
[U]::SetCursorPos($x0, $y0) | Out-Null
Start-Sleep -Milliseconds 300
[U]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
$t = 0.0
while ($t -lt __SECS__ / 2) {
  $k = [Math]::Sin($t / (__SECS__ / 2) * [Math]::PI * 2)
  [U]::SetCursorPos([int]($x0 + 220 * $k), [int]($y0 + 140 * $k)) | Out-Null
  Start-Sleep -Milliseconds 100; $t += 0.1
}
while ($t -lt __SECS__) {
  $k = [Math]::Sin($t * 9.0)
  [U]::SetCursorPos([int]($x0 + 320 * $k), [int]($y0 + 200 * $k)) | Out-Null
  Start-Sleep -Milliseconds 16; $t += 0.016
}
[U]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
$r2 = New-Object U+RECT
[U]::GetWindowRect($h, [ref]$r2) | Out-Null
Write-Output "released rect $($r2.L),$($r2.T),$($r2.R),$($r2.B)"
if ($r2.R -eq $r.R -and $r2.B -eq $r.B) {
  # The injected drag did not reach the sizing border (no interactive input on this session):
  # resize from outside instead, the same 10 s path, through SetWindowPos (WM_SIZE, no modal loop).
  Write-Output "drag had no effect; SetWindowPos fallback"
  $t = 0.0
  while ($t -lt __SECS__ / 2) {
    $k = [Math]::Sin($t / (__SECS__ / 2) * [Math]::PI * 2)
    [U]::SetWindowPos($h, [IntPtr]::Zero, 40, 40, [int](760 + 200 * $k), [int](560 + 120 * $k), 0x0004) | Out-Null
    Start-Sleep -Milliseconds 100; $t += 0.1
  }
  while ($t -lt __SECS__) {
    $k = [Math]::Sin($t * 9.0)
    [U]::SetWindowPos($h, [IntPtr]::Zero, 40, 40, [int](760 + 220 * $k), [int](560 + 150 * $k), 0x0004) | Out-Null
    Start-Sleep -Milliseconds 16; $t += 0.016
  }
}
"""


def drive_resize_windows(run, seconds, notes):
    script = WIN_DRAG_PS1.replace("__PID__", str(run.proc.pid)).replace("__SECS__", str(seconds))
    path = os.path.join(tempfile.gettempdir(), "pebbles-drag.ps1")
    with open(path, "w", encoding="utf-8") as f:
        f.write(script)
    out = subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", path],
                         capture_output=True, text=True, timeout=seconds + 60)
    notes.append("SendInput drag of the bottom-right frame corner (modal size-move loop): "
                 + " | ".join(out.stdout.split("\n")[:4]).strip() + out.stderr.strip()[:300])
    return out.returncode == 0


MAC_DRAG_SWIFT = r"""
import Foundation
import CoreGraphics
let pid = Int32(CommandLine.arguments[1])!
let secs = Double(CommandLine.arguments[2])!
var bounds: CGRect? = nil
for _ in 0..<50 {
  let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
  for w in list where (w[kCGWindowOwnerPID as String] as? Int32) == pid && (w[kCGWindowLayer as String] as? Int) == 0 {
    let b = w[kCGWindowBounds as String] as! [String: CGFloat]
    let r = CGRect(x: b["X"]!, y: b["Y"]!, width: b["Width"]!, height: b["Height"]!)
    if bounds == nil || r.width * r.height > bounds!.width * bounds!.height { bounds = r }
  }
  if bounds != nil { break }
  usleep(200_000)
}
guard let r = bounds else { print("no window"); exit(2) }
print("bounds \(r)")
print("trusted \(AXIsProcessTrusted())")
let src = CGEventSource(stateID: .hidSystemState)
func post(_ t: CGEventType, _ p: CGPoint) {
  CGEvent(mouseEventSource: src, mouseType: t, mouseCursorPosition: p, mouseButton: .left)!.post(tap: .cghidEventTap)
}
let p0 = CGPoint(x: r.maxX - 2, y: r.maxY - 2)
post(.mouseMoved, p0); usleep(300_000)
post(.leftMouseDown, p0); usleep(100_000)
var t = 0.0
while t < secs / 2 {
  let k = sin(t / (secs / 2) * Double.pi * 2)
  post(.leftMouseDragged, CGPoint(x: p0.x + 220 * k, y: p0.y + 140 * k)); usleep(100_000); t += 0.1
}
while t < secs {
  let k = sin(t * 9.0)
  post(.leftMouseDragged, CGPoint(x: p0.x + 320 * k, y: p0.y + 200 * k)); usleep(16_000); t += 0.016
}
post(.leftMouseUp, CGPoint(x: p0.x, y: p0.y))
print("released")
"""


MAC_DRAG_BIN = os.path.join(tempfile.gettempdir(), "pebbles_drag")


def prepare_mac_drag():
    path = MAC_DRAG_BIN + ".swift"
    with open(path, "w", encoding="utf-8") as f:
        f.write("import ApplicationServices\n" + MAC_DRAG_SWIFT)
    subprocess.run(["swiftc", "-O", path, "-o", MAC_DRAG_BIN], capture_output=True, timeout=600)


def drive_resize_mac(run, seconds, notes):
    if not os.path.exists(MAC_DRAG_BIN):
        prepare_mac_drag()
    out = subprocess.run([MAC_DRAG_BIN, str(run.proc.pid), str(seconds)], capture_output=True, text=True,
                         timeout=seconds + 60)
    notes.append("CGEvent drag of the bottom-right window corner: " + " | ".join(out.stdout.split("\n")[:4]).strip()
                 + out.stderr.strip()[-300:])
    return out.returncode == 0


def drive_resize(run, seconds, notes):
    if IS_LINUX:
        return drive_resize_xdotool(run, seconds, notes)
    if IS_WIN:
        return drive_resize_windows(run, seconds, notes)
    if IS_MAC:
        return drive_resize_mac(run, seconds, notes)
    return False


COVER_PY = r"""
import sys, tkinter as tk
secs = float(sys.argv[1])
root = tk.Tk()
w, h = root.winfo_screenwidth(), root.winfo_screenheight()
root.geometry(f"{w}x{h}+0+0")
root.attributes("-topmost", True)
try:
    root.attributes("-fullscreen", True)
except tk.TclError:
    pass
root.configure(bg="#202020")
root.lift()
root.focus_force()
print(f"cover {w}x{h}", flush=True)
root.after(int(secs * 1000), root.destroy)
root.mainloop()
print("uncovered", flush=True)
"""


BACKDROP_PY = r"""
import sys, tkinter as tk
root = tk.Tk()
w, h = root.winfo_screenwidth(), root.winfo_screenheight()
root.geometry(f"{w}x{h}+0+0")
root.configure(bg="#ff00ff")
root.title("pebbles backdrop")
print(f"backdrop {w}x{h}", flush=True)
root.after(int(float(sys.argv[1]) * 1000), root.destroy)
root.mainloop()
"""


def start_backdrop(seconds):
    """A magenta full-screen window under the app: black pixels in a screenshot are then the app's."""
    path = os.path.join(tempfile.gettempdir(), "pebbles_backdrop.py")
    with open(path, "w", encoding="utf-8") as f:
        f.write(BACKDROP_PY)
    proc = subprocess.Popen([sys.executable, path, str(seconds)], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    time.sleep(3.0)
    return proc


def cover_screen(seconds, notes):
    path = os.path.join(tempfile.gettempdir(), "pebbles_cover.py")
    with open(path, "w", encoding="utf-8") as f:
        f.write(COVER_PY)
    out = subprocess.run([sys.executable, path, str(seconds)], capture_output=True, text=True, timeout=seconds + 60)
    notes.append("tk full-screen cover: " + out.stdout.strip().replace("\n", " | ") + out.stderr.strip()[-300:])
    return out.returncode == 0


# --- screenshots ------------------------------------------------------------------------------


def screenshot(path):
    try:
        if IS_LINUX:
            r = subprocess.run(["import", "-window", "root", path], capture_output=True, timeout=20)
        elif IS_MAC:
            r = subprocess.run(["screencapture", "-x", path], capture_output=True, timeout=20)
        else:
            ps = (
                "Add-Type -AssemblyName System.Windows.Forms,System.Drawing;"
                "Add-Type 'using System.Runtime.InteropServices; public static class D "
                "{ [DllImport(\"user32.dll\")] public static extern bool SetProcessDPIAware(); }';"
                "[D]::SetProcessDPIAware() | Out-Null;"
                "$b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds;"
                "$i=New-Object System.Drawing.Bitmap $b.Width,$b.Height;"
                "$g=[System.Drawing.Graphics]::FromImage($i);$g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size);"
                f"$i.Save('{path}')"
            )
            r = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, timeout=30)
        return r.returncode == 0 and os.path.exists(path)
    except (OSError, subprocess.TimeoutExpired):
        return False


def region_stats(path, rect):
    """(fraction of near-black pixels, mean RGB) of the screenshot (inside `rect` when given), or None."""
    try:
        from PIL import Image
    except ImportError:
        return None
    try:
        img = Image.open(path).convert("RGB")
    except OSError:
        return None
    if rect:
        x, y, w, h = rect
        box = (max(0, x), max(0, y), min(img.width, x + w), min(img.height, y + h))
        if box[2] - box[0] > 16 and box[3] - box[1] > 16:
            img = img.crop(box)
    img = img.resize((max(1, img.width // 4), max(1, img.height // 4)))
    px = list(img.getdata())
    black = sum(1 for p in px if max(p) <= 12)
    mean = tuple(round(sum(c[i] for c in px) / len(px), 1) for i in range(3))
    return round(black / len(px), 4), mean


# --- log analysis -----------------------------------------------------------------------------


def run_lines(lines):
    out = []
    for t, text in lines:
        m = RUN_RE.search(text)
        if m:
            d = {k: m[k] for k in RUN_RE.groupindex}
            d["at"] = round(t, 3)
            for k in ("t", "scale", "gmax", "gp95", "umax", "up95", "inp"):
                d[k] = float(d[k])
            for k in ("windows", "w", "h", "pm", "pmt", "ps", "pst", "un", "unt", "occ", "occs", "rst", "gen"):
                d[k] = int(d[k])
            out.append(d)
    return out


def grep(lines, pattern):
    rx = re.compile(pattern)
    return [(round(t, 3), text) for t, text in lines if rx.search(text)]


def common(lines, alive):
    panics = grep(lines, r"panicked|RUST_BACKTRACE|stack backtrace")
    errors = grep(lines, r"\] ?\[?\s*\d+\.\d+s ERROR|^\[\s*\d+\.\d+s ERROR")
    gpu = grep(lines, r"GPU: .*backend=")
    return panics, errors, gpu


def presents_between(runs, t0, t1, key="pm"):
    """Per-second main (pm) or secondary (ps) presents of the run lines logged in [t0, t1]."""
    return [r[key] for r in runs if t0 <= r["at"] <= t1]


# --- scenarios --------------------------------------------------------------------------------


def scenario_env(name, mode):
    env = {
        "PEBBLES_LOG": "info",
        "PEBBLES_GPU_INFO": "1",
        "PEBBLES_FRAME_STATS": "1",
        "PEBBLES_RENDER_THREAD": "1" if mode == "threaded" else "0",
        "RUST_BACKTRACE": "1",
    }
    extra = {
        "multi": {"PEBBLES_WINDOW_CHURN": "multi"},
        "occ": {"PEBBLES_WINDOW_CHURN": "multi-static"},
        "thread": {"PEBBLES_WINDOW_CHURN": "multi", "PEBBLES_INPUT_STORM": "1"},
        "storm": {"PEBBLES_WINDOW_CHURN": "1", "PEBBLES_INPUT_STORM": "1"},
        # The loss is injected every N rendered frames: the input storm keeps frames coming.
        "c5": {"PEBBLES_SYNTH_GPU_LOSS": "300", "PEBBLES_INPUT_STORM": "1", "PEBBLES_SOAK_CSV": "__CSV__"},
        "p14": {"PEBBLES_PRESENT_MODE": "mailbox"},
        "p19": {"PEBBLES_SYNTH_GPU_LOSS": "120", "PEBBLES_SYNTH_GPU_DEAD": "1", "PEBBLES_INPUT_STORM": "1"},
    }.get(name, {})
    env.update(extra)
    return env


def check(results, ok, label, detail=""):
    results.append({"check": label, "pass": bool(ok), "detail": detail})


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scenario", required=True)
    ap.add_argument("--mode", choices=["inline", "threaded"], default="inline")
    ap.add_argument("--bin", required=True)
    ap.add_argument("--out", default="runs")
    ap.add_argument("--duration", type=float, default=0.0, help="override the scenario's run time (s)")
    ap.add_argument("--row", default="", help="the row label for the summary (e.g. 'L-ci Xvfb lavapipe')")
    ap.add_argument("--env", action="append", default=[], help="extra K=V")
    ap.add_argument("--tag", default="", help="file-name suffix for a variant run (e.g. gl)")
    a = ap.parse_args()

    os.makedirs(a.out, exist_ok=True)
    tag = f"{a.scenario}-{a.mode}" + (f"-{a.tag}" if a.tag else "")
    env = dict(os.environ)
    env.update(scenario_env(a.scenario, a.mode))
    for kv in a.env:
        k, _, v = kv.partition("=")
        env[k] = v
    if env.get("PEBBLES_SOAK_CSV") == "__CSV__":
        env["PEBBLES_SOAK_CSV"] = os.path.abspath(os.path.join(a.out, f"{tag}.csv"))
        if os.path.exists(env["PEBBLES_SOAK_CSV"]):
            os.remove(env["PEBBLES_SOAK_CSV"])
    duration = a.duration or {
        "c1": 15, "c2": 30, "multi": 45, "thread": 45, "storm": 100, "occ": 70, "c5": 600, "p14": 12, "p19": 50,
    }.get(a.scenario, 20)
    log_path = os.path.join(a.out, f"{tag}.log")
    notes, results, shots = [], [], []
    backdrop = start_backdrop(duration + 60) if a.scenario == "c2" else None
    if a.scenario == "c2" and IS_MAC:
        # Compile the drag tool before the app starts (swift compiles for ~10 s on first use).
        prepare_mac_drag()
    run = Run([a.bin], env, log_path)
    first = run.wait_for(r"first frame presented", 120)
    events = {"first_present": first[0] if first else None}

    def shot(label):
        p = os.path.join(a.out, f"{tag}-{label}.png")
        if screenshot(p):
            st = region_stats(p, None)
            shots.append({"label": label, "at": round(run.elapsed(), 2), "file": os.path.basename(p), "stats": st})
            return p
        return None

    if first:
        time.sleep(2.0)
        if a.scenario in ("c2",):
            shot("before")
            events["drive_start"] = run.elapsed()
            done = {"v": False}

            def shooter():
                i = 0
                while not done["v"]:
                    time.sleep(1.3)
                    if not done["v"]:
                        shot(f"during{i}")
                        i += 1

            th = threading.Thread(target=shooter, daemon=True)
            th.start()
            events["driver_ok"] = drive_resize(run, 10, notes)
            done["v"] = True
            events["drive_end"] = run.elapsed()
            th.join(5)
            time.sleep(2.0)
            shot("released")
        elif a.scenario == "occ":
            run.wait_for(r"WINDOW MULTI armed", 5)
            time.sleep(6.0)
            events["cover_start"] = run.elapsed()
            shot("before-cover")
            th = threading.Thread(target=lambda: events.__setitem__("cover_ok", cover_screen(12, notes)), daemon=True)
            th.start()
            time.sleep(3.0)
            shot("covered")
            th.join(60)
            events["cover_end"] = run.elapsed()
            time.sleep(3.0)
            shot("uncovered")
        elif a.scenario == "c5":
            time.sleep(1.0)
            shot("t0")
            while run.elapsed() < duration - 65 and run.alive():
                time.sleep(60)
                shot(f"t{int(run.elapsed())}")
        else:
            shot("first")
    while run.elapsed() < duration and run.alive():
        time.sleep(0.5)
    if a.scenario not in ("c5",):
        shot("end")
    alive = run.stop()
    if backdrop:
        backdrop.kill()
    lines = run.snapshot()
    runs = run_lines(lines)
    panics, errors, gpu = common(lines, alive)

    # -- generic checks (every row: C1 basics) --
    check(results, first is not None, "first frame presented",
          first[1].strip() if first else "no 'first frame presented' line")
    check(results, alive, "process alive until stopped", f"exit code {run.proc.returncode}" if not alive else "")
    check(results, not panics, "no panic", "; ".join(t for _, t in panics[:3]))
    backend = EXPECTED_BACKEND.get(sys.platform if not IS_LINUX else "linux")
    want_backend = env.get("WGPU_BACKEND") or backend
    gpu_ok = any(f"backend={want_backend}" in t for _, t in gpu)
    check(results, gpu_ok, f"GPU line names backend={want_backend}", gpu[0][1].strip() if gpu else "no GPU: line")
    expected_errors = a.scenario in ("p19", "c5")
    if not expected_errors:
        check(results, not errors, "no ERROR log", "; ".join(t for _, t in errors[:3]))
    # (The line says `inline` while a recovery has no sink yet; judge the steady state.)
    modes = [r["mode"] for r in runs]
    check(results, bool(modes) and max(set(modes), key=modes.count) == a.mode, f"frames rendered {a.mode}",
          f"{modes.count('inline')} inline / {modes.count('threaded')} threaded summary lines")

    summary = {}
    if runs:
        summary["last_run_line"] = runs[-1]
    # -- scenario checks --
    if a.scenario == "c1":
        check(results, runs and runs[-1]["pmt"] > 0, "frames presented", f"{runs[-1]['pmt'] if runs else 0}")
    elif a.scenario == "c2":
        t0, t1 = events.get("drive_start", 0), events.get("drive_end", 0)
        during = [r for r in runs if t0 + 1.0 <= r["at"] <= t1 + 0.5]
        sizes = {(r["w"], r["h"]) for r in runs if t0 <= r["at"] <= t1 + 0.5}
        check(results, len(sizes) >= 5, "the OS resized the window (distinct sizes seen in [pebbles run])",
              f"{len(sizes)} sizes; driver: {notes[-1] if notes else ''}")
        changed = [r for p_, r in zip(runs, runs[1:]) if t0 <= r["at"] <= t1 + 0.5 and (p_["w"], p_["h"]) != (r["w"], r["h"])]
        idle = [r for r in changed if r["pm"] == 0]
        check(results, changed and not idle, "frames presented in every second the window size changed",
              f"per-second presents while resizing: {[r['pm'] for r in changed]}")
        worst_ui = max((r["umax"] for r in during), default=0.0)
        check(results, worst_ui <= FRAME_BUDGET_MS,
              f"no UI frame above {FRAME_BUDGET_MS:.0f} ms after the first second (C2 budget)",
              f"max ui_ms {worst_ui:.1f}; p95 per second {[r['up95'] for r in during]}; "
              f"max present gap per second {[r['gmax'] for r in during]}")
        # Over the magenta backdrop, near-black pixels can only be unpainted window content.
        base_black = shots[0]["stats"][0] if shots and shots[0]["stats"] else 0.0
        blanks = [s for s in shots if s["stats"] and s["stats"][0] > base_black + 0.005]
        check(results, shots and not blanks, "no black (unpainted) window area in screenshots during/after the resize",
              f"{len(shots)} shots; black fraction of the screen (first = before the drag) "
              f"{[s['stats'][0] if s['stats'] else None for s in shots]}")
        if a.mode == "threaded":
            rst = runs[-1]["rst"] if runs else -1
            check(results, rst == 0, "resize_sync_timeouts == 0 (M-RESIZE)", f"resize_sync_timeouts={rst}")
        summary["sizes_seen"] = len(sizes)
    elif a.scenario in ("multi", "thread", "occ"):
        done = grep(lines, r"window multi done")
        if a.scenario != "occ":
            check(results, bool(done), "window multi done (3 windows opened, resized 60×, closed)",
                  done[0][1] if done else "")
        opened = [r for r in runs if r["windows"] == 4]
        sec = [r["ps"] for r in opened[1:-1]]
        if a.scenario != "occ":
            check(results, sec and min(sec) > 0, "secondary windows presented in every second",
                  f"secondary presents/s: {sec}")
        tex = [int(m.group(1)) for _, t in lines for m in [GPU_STATS_TEX_RE.search(t)] if m]
        realloc = sum(1 for v in tex if v > 0)
        check(results, tex and realloc / len(tex) < 0.25,
              "no per-frame texture reallocation (frames with new textures < 25 %)",
              f"{realloc} of {len(tex)} frames created textures (60 resizes × 3 windows + opens)")
        census = grep(lines, r"census: ")
        summary["census"] = census[-1][1] if census else None
        if a.scenario == "thread":
            main = [r["pm"] for r in opened[1:-1]]
            inp = [r["inp"] for r in opened[1:-1]]
            gp = [r["gp95"] for r in opened[1:-1]]
            summary["main_fps"] = main
            summary["secondary_fps"] = sec
            summary["input_to_present_ms"] = inp
            summary["main_gap_p95_ms"] = gp
            frames = [i / g for i, g in zip(inp, gp) if g > 0]
            check(results, frames and max(frames) <= 2.5,
                  "typed input presented by the next frame (input→present ≤ 2.5 frame intervals)",
                  f"max {max(frames) if frames else 'n/a'} intervals; input_ms {inp}")
        if a.scenario == "occ":
            c0, c1 = events.get("cover_start", 0), events.get("cover_end", 0)
            covered = [r for r in runs if c0 + 4 <= r["at"] <= c1 - 1]
            after = [r for r in runs if c1 + 0.5 <= r["at"] <= c1 + 4]
            summary["covered"] = [(r["ps"], r["occ"], r["occs"]) for r in covered]
            summary["after"] = [(r["ps"], r["occ"], r["occs"]) for r in after]
            occ_seen = any(r["occs"] > 0 or r["occ"] for r in covered)
            check(results, occ_seen, "the window system reported the windows occluded while covered",
                  f"(secondary presents, main occluded, secondary occluded) per s: {summary['covered']}")
            check(results, covered and max(r["ps"] for r in covered) <= 2,
                  "no presents (no spin) while covered (the spinner window animates otherwise)",
                  f"{[r['ps'] for r in covered]}")
            check(results, after and max(r["ps"] for r in after) > 0, "repaints after uncover with no input",
                  f"{[r['ps'] for r in after]}")
    elif a.scenario == "storm":
        done = grep(lines, r"window churn done")
        check(results, bool(done), "window churn done (20 cycles × 10 windows)", done[0][1] if done else "")
        storms = grep(lines, r"storm at \d+ steps")
        check(results, bool(storms), "input storm ran", storms[-1][1] if storms else "")
        main = [r["pm"] for r in runs[1:-1]]
        check(results, main and min(main) > 0, "main window presented every second", f"{main}")
    elif a.scenario == "c5":
        injected = grep(lines, r"SYNTH device loss injected")
        rebuilt = grep(lines, r"GPU stack rebuilt")
        failed = grep(lines, r"GPU stack rebuild failed")
        # Threaded: losses injected while the device is already lost coalesce into one reset
        # (the synthetic counter counts UI frames, which outrun presents there).
        enough = len(rebuilt) >= len(injected) - 1 if a.mode == "inline" else len(rebuilt) >= 3
        check(results, enough and not failed, "every injected loss rebuilt the stack (no failed rebuild)",
              f"{len(injected)} injected, {len(rebuilt)} rebuilt, {len(failed)} failed")
        # Presents after each rebuild: the generation advances and frames keep presenting.
        gens = {}
        for r in runs:
            gens.setdefault(r["gen"], 0)
            gens[r["gen"]] += r["pm"]
        dead = [g for g, n in gens.items() if n == 0 and g != max(gens)]
        summary["generations_without_present"] = dead
        tail = [r["pm"] for r in runs[-5:]]
        if a.mode == "inline":
            check(results, gens and not dead, "frames presented after every reset",
                  f"generations without a present: {dead}")
        else:
            check(results, sum(tail) > 0, "frames presented after the resets (last 5 s)",
                  f"presents/s {tail}; generations replaced before presenting (loss re-injected first): {len(dead)}")
        # Leaks are judged by the renderer census, never RSS (GpuCensus docs, 11 §2.9): the
        # per-target and per-device resources must not grow across resets.
        rss = run.rss
        if len(rss) > 60:
            early = [k for t, k in rss if 60 <= t <= 120]
            late = [k for t, k in rss if t >= rss[-1][0] - 60]
            if early and late:
                summary["rss_growth_pct_info"] = round(((sum(late) / len(late)) / (sum(early) / len(early)) - 1) * 100, 1)
        rows = []
        csv_path = env.get("PEBBLES_SOAK_CSV")
        if csv_path and os.path.exists(csv_path):
            import csv
            with open(csv_path, newline="", encoding="utf-8") as f:
                rows = [r for r in csv.DictReader(f) if r.get("frame_target_entries")]
        # (Byte totals follow the window size, which the storm changes; judge the counts.)
        keys = ["frame_target_entries", "image_entries", "image_bind_groups", "samplers",
                "pipelines", "layer_pool_textures", "gpu_mask_textures"]
        if len(rows) >= 4:
            first, last = rows[len(rows) // 4], rows[-1]
            grew = {k: (first[k], last[k]) for k in keys if int(last[k] or 0) > int(first[k] or 0)}
            summary["census_first_last"] = {k: (first[k], last[k]) for k in keys}
            check(results, not grew, "renderer census flat across resets (11 §2.9 soak CSV)",
                  f"{len(rows)} rows; grew: {grew or 'none'}; RSS growth (info) {summary.get('rss_growth_pct_info')} %")
        else:
            check(results, False, "renderer census flat across resets (11 §2.9 soak CSV)",
                  f"{len(rows)} census rows (the census is read on the UI thread: inline mode only); "
                  f"RSS growth (info) {summary.get('rss_growth_pct_info')} %")
        summary["screenshots"] = [s["file"] for s in shots]
    elif a.scenario == "p14":
        warn = grep(lines, r"present mode Mailbox unsupported")
        surf = grep(lines, r"surface: format=")
        honored = not warn and surf and all("present=Mailbox" in t for _, t in surf)
        fallback = len(warn) == 1 and surf and all("present=AutoVsync" in t for _, t in surf)
        check(results, honored or fallback,
              "mailbox honored where offered, else exactly one warning and present=AutoVsync (§4.3)",
              ("; ".join(t for _, t in warn) or "no warning") + " || " + "; ".join(t for _, t in surf))
        summary["p14"] = "honored" if honored else ("fallback" if fallback else "neither")
    elif a.scenario == "p19":
        lost = grep(lines, r"GPU device lost")
        attempts = grep(lines, r"GPU stack rebuild failed|GPU stack rebuilt|PEBBLES_SYNTH_GPU_DEAD")
        tries = [t for t, txt in grep(lines, r"GPU stack rebuild failed|GPU stack rebuilt")]
        gaps = [round(b - a_, 2) for a_, b in zip(tries, tries[1:])]
        summary["reset_times"] = tries
        summary["reset_gaps"] = gaps
        want = [1, 2, 4, 8, 10, 10]
        ok = len(gaps) >= len(want) and all(abs(g - w) <= 0.6 for g, w in zip(gaps, want))
        check(results, ok, "resets 1, 2, 4, 8, 10, 10 s apart", f"gaps {gaps}; first loss "
              f"{lost[0] if lost else 'none'}")
        guidance = grep(lines, r"GPU resets keep failing")
        check(results, bool(guidance), "guidance logged at the 5th failure", guidance[0][1] if guidance else "")
        summary["attempt_lines"] = attempts[:12]

    payload = {
        "scenario": a.scenario, "mode": a.mode, "row": a.row, "platform": sys.platform, "duration_s": duration,
        "events": events, "notes": notes, "checks": results, "summary": summary, "screenshots": shots,
        "gpu_line": gpu[0][1] if gpu else None, "panics": panics[:5], "errors": errors[:10],
        "run_lines": runs[-3:],
    }
    with open(os.path.join(a.out, f"{tag}.json"), "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2, default=str)
    failed = [r for r in results if not r["pass"]]
    md = [f"### {a.row} — {a.scenario} ({a.mode}): {'PASS' if not failed else 'FAIL'}", ""]
    if gpu:
        md.append(f"`{gpu[0][1].strip()}`")
        md.append("")
    md += ["| check | result | detail |", "|---|---|---|"]
    for r in results:
        detail = str(r["detail"]).replace("|", "\\|")[:600]
        md.append(f"| {r['check']} | {'pass' if r['pass'] else '**FAIL**'} | {detail} |")
    for n in notes:
        md.append(f"\n> {n[:500]}")
    text = "\n".join(md) + "\n\n"
    print(text)
    print(json.dumps(summary, default=str)[:4000])
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as f:
            f.write(text)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
